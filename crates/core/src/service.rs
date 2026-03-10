use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use partbooter_common::{
    ActionOutcome, AdditiveBootPolicy, AppError, AppErrorKind, AppResult, ExecutionPlan,
    OperationJournal, OperationStatus, OperationStepRecord, PartitionStyle, PayloadKind,
    PayloadSpec, PlanStep, PlanStepKind, RiskFlag, RiskLevel, VerificationReport,
};
use partbooter_journal::FileJournalStore;
use partbooter_payload_linux_iso as linux_iso;
use partbooter_payload_winpe as winpe;
use partbooter_windows::{
    BackupCheckpoint, BootEntryRegistration, WinPeStaging, WindowsApplyAdapter, WindowsProbeAdapter,
};

#[derive(Debug, Clone)]
enum ProbeSource {
    Live,
    #[cfg(test)]
    Fixture(partbooter_common::MachineProbe),
}

#[derive(Debug, Clone)]
pub struct PartBooterService {
    journal_store: FileJournalStore,
    probe_source: ProbeSource,
}

impl PartBooterService {
    pub fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            journal_store: FileJournalStore::new(state_root.into()),
            probe_source: ProbeSource::Live,
        }
    }

    #[cfg(test)]
    pub fn with_probe_fixture(
        state_root: impl Into<PathBuf>,
        probe: partbooter_common::MachineProbe,
    ) -> Self {
        Self {
            journal_store: FileJournalStore::new(state_root.into()),
            probe_source: ProbeSource::Fixture(probe),
        }
    }

    pub fn probe_machine(&self) -> AppResult<partbooter_common::MachineProbe> {
        match &self.probe_source {
            ProbeSource::Live => WindowsProbeAdapter::probe(),
            #[cfg(test)]
            ProbeSource::Fixture(probe) => Ok(probe.clone()),
        }
    }

    pub fn inspect_payload(&self, source_path: impl AsRef<Path>) -> PayloadSpec {
        let source_path = source_path.as_ref();
        if let Some(payload) = winpe::detect(source_path) {
            return payload;
        }
        if let Some(payload) = linux_iso::detect(source_path) {
            return payload;
        }

        PayloadSpec {
            source_path: source_path.display().to_string(),
            kind: PayloadKind::Unsupported,
            display_name: "Unsupported payload".to_string(),
            profile: "unsupported".to_string(),
            supported: false,
            notes: vec![
                "Only supported Linux ISO profiles and WinPE WIM payloads are accepted in v1."
                    .to_string(),
            ],
        }
    }

    pub fn build_plan(
        &self,
        source_path: impl AsRef<Path>,
        target_volume: impl Into<String>,
    ) -> AppResult<ExecutionPlan> {
        let probe = self.probe_machine()?;
        let payload = self.inspect_payload(source_path);

        self.validate_probe(&probe)?;
        self.validate_payload(&payload)?;

        let plan_id = format!("plan-{}", unique_suffix());
        let target_volume = target_volume.into();
        let backup_root = self
            .journal_store
            .root()
            .join("backups")
            .join(&plan_id)
            .display()
            .to_string();

        let mut risk_flags = vec![RiskFlag {
            code: "additive-only".to_string(),
            level: RiskLevel::Info,
            message: "PartBooter will only add a boot entry and will not replace the current default path.".to_string(),
        }];

        if probe.bitlocker_detected {
            risk_flags.push(RiskFlag {
                code: "bitlocker-detected".to_string(),
                level: RiskLevel::Warning,
                message:
                    "BitLocker-protected volumes were detected; review suspension and recovery requirements before apply."
                        .to_string(),
            });
        }

        risk_flags.extend(
            probe
                .warnings
                .iter()
                .enumerate()
                .map(|(index, warning)| RiskFlag {
                    code: format!("probe-warning-{}", index + 1),
                    level: RiskLevel::Warning,
                    message: warning.clone(),
                }),
        );

        if probe.secure_boot_enabled && payload.kind == PayloadKind::LinuxIso {
            risk_flags.push(RiskFlag {
                code: "secure-boot-review".to_string(),
                level: RiskLevel::Warning,
                message:
                    "Secure Boot remains enabled; Linux ISO compatibility must be validated per supported profile."
                        .to_string(),
            });
        }

        let payload_specific_loader_step = match payload.kind {
            PayloadKind::LinuxIso => {
                "Generate Linux ISO loader configuration for the supported distro profile."
            }
            PayloadKind::WinPe => "Generate WinPE loader configuration for the staged WIM payload.",
            PayloadKind::Unsupported => {
                "Unsupported payloads cannot generate a loader configuration."
            }
        };

        let steps = vec![
            PlanStep {
                id: 1,
                kind: PlanStepKind::BackupEsp,
                requires_elevation: true,
                description: "Backup the current EFI System Partition contents into the PartBooter backup root.".to_string(),
            },
            PlanStep {
                id: 2,
                kind: PlanStepKind::SnapshotBootConfig,
                requires_elevation: true,
                description: "Snapshot the current Windows boot configuration before staging any managed files.".to_string(),
            },
            PlanStep {
                id: 3,
                kind: PlanStepKind::StagePayload,
                requires_elevation: true,
                description: format!(
                    "Stage payload artifacts from {} into the managed PartBooter directory.",
                    payload.source_path
                ),
            },
            PlanStep {
                id: 4,
                kind: PlanStepKind::WriteLoaderConfig,
                requires_elevation: true,
                description: payload_specific_loader_step.to_string(),
            },
            PlanStep {
                id: 5,
                kind: PlanStepKind::RegisterBootEntry,
                requires_elevation: true,
                description:
                    "Register a new additive boot entry without changing the existing default boot target."
                        .to_string(),
            },
            PlanStep {
                id: 6,
                kind: PlanStepKind::VerifyBootEntry,
                requires_elevation: false,
                description:
                    "Verify that staged files, backup manifest, and the additive boot entry are present."
                        .to_string(),
            },
        ];

        Ok(ExecutionPlan {
            version: 1,
            plan_id,
            target_volume,
            payload,
            backup_root,
            additive_policy: AdditiveBootPolicy {
                replace_default_boot_path: false,
            },
            risk_flags,
            steps,
            created_at: iso_timestamp(),
        })
    }

    pub fn apply_plan(&self, plan: &ExecutionPlan) -> AppResult<OperationJournal> {
        let probe = self.probe_machine()?;
        self.validate_probe(&probe)?;
        self.validate_payload(&plan.payload)?;
        self.journal_store.ensure_layout()?;

        let operation_id = format!("op-{}", unique_suffix());
        let operation_root = self.journal_store.operation_dir(&operation_id);
        fs::create_dir_all(&operation_root)?;
        fs::write(operation_root.join("plan.pbplan"), plan.to_plan_file())?;

        let backup_root = self.journal_store.backup_root_for_plan(&plan.plan_id);
        let checkpoint = self.create_backup_checkpoint(&probe, &backup_root)?;
        self.journal_store.write_backup_manifest(
            plan,
            &checkpoint.esp_backup_dir,
            &checkpoint.bcd_store_path,
            &checkpoint.notes,
        )?;

        let journal = match plan.payload.kind {
            PayloadKind::WinPe => {
                self.apply_winpe_plan(plan, &probe, &operation_id, &operation_root, &checkpoint)?
            }
            _ => {
                let steps = plan
                    .steps
                    .iter()
                    .map(|step| self.apply_checkpoint_step_record(step, &checkpoint))
                    .collect::<Vec<_>>();

                OperationJournal {
                    operation_id: operation_id.clone(),
                    plan_id: plan.plan_id.clone(),
                    backup_root: plan.backup_root.clone(),
                    status: OperationStatus::Checkpointed,
                    steps,
                    summary: format!(
                        "Created a protected backup checkpoint for {} on target volume {}; payload staging and boot registration remain pending.",
                        plan.payload.display_name, plan.target_volume
                    ),
                }
            }
        };

        self.journal_store.save_journal(&journal)?;
        Ok(journal)
    }

    pub fn verify_operation(&self, operation_id: &str) -> AppResult<VerificationReport> {
        let journal = self.journal_store.load_journal(operation_id)?;
        let operation_root = self.journal_store.operation_dir(operation_id);
        let backup_root = PathBuf::from(&journal.backup_root);
        let backup_artifacts_present = backup_root.join("manifest.txt").exists()
            && backup_root.join("esp").exists()
            && backup_root.join("bcd-store.bak").exists();
        let operation_plan_present = self
            .journal_store
            .operation_plan_path(operation_id)
            .exists();
        let winpe_metadata = self.read_winpe_operation_metadata(&operation_root)?;
        let staged_artifacts_present = if let Some(metadata) = &winpe_metadata {
            metadata.boot_wim_path.exists()
                && metadata.boot_sdi_path.exists()
                && metadata.loader_spec_path.exists()
        } else {
            journal.status == OperationStatus::Applied
                || journal.status == OperationStatus::Verified
        };
        let boot_entry_registered = if let Some(metadata) = &winpe_metadata {
            self.verify_registered_entry(&metadata.entry_id)?
        } else {
            journal.status == OperationStatus::Applied
                || journal.status == OperationStatus::Verified
        };

        let mut warnings = Vec::new();
        let full_boot_path_expected = winpe_metadata.is_some()
            || journal.status == OperationStatus::Applied
            || journal.status == OperationStatus::Verified;
        if journal.status == OperationStatus::Checkpointed {
            warnings.push(
                "Operation is checkpointed only; payload staging and boot entry registration are not implemented in this milestone."
                    .to_string(),
            );
        }
        if journal.status == OperationStatus::Applied && !staged_artifacts_present {
            warnings.push("Managed WinPE staging artifacts are incomplete or missing.".to_string());
        }
        if journal.status == OperationStatus::Applied && !boot_entry_registered {
            warnings.push("Managed BCD entry is missing or no longer registered.".to_string());
        }
        if !backup_artifacts_present {
            warnings.push("Backup artifacts are incomplete or missing.".to_string());
        }
        if !operation_plan_present {
            warnings.push("Saved plan artifact is missing for the operation.".to_string());
        }

        Ok(VerificationReport {
            operation_id: journal.operation_id,
            backup_artifacts_present,
            operation_plan_present,
            boot_entry_registered,
            staged_artifacts_present,
            warnings,
            verified: if full_boot_path_expected {
                backup_artifacts_present
                    && operation_plan_present
                    && staged_artifacts_present
                    && boot_entry_registered
            } else {
                backup_artifacts_present && operation_plan_present
            },
        })
    }

    pub fn rollback_operation(&self, operation_id: &str) -> AppResult<OperationJournal> {
        let mut journal = self.journal_store.load_journal(operation_id)?;
        let operation_root = self.journal_store.operation_dir(operation_id);
        let backup_store_path = PathBuf::from(&journal.backup_root).join("bcd-store.bak");
        if let Some(metadata) = self.read_winpe_operation_metadata(&operation_root)? {
            self.restore_boot_config(&backup_store_path)?;
            self.remove_staged_payload(&metadata.stage_root, &metadata.esp_stage_root)?;
            self.remove_winpe_operation_metadata(&operation_root)?;
        }
        journal.status = OperationStatus::RolledBack;
        journal.summary = format!(
            "Rolled back operation {} using recorded backup state.",
            operation_id
        );
        journal.steps.push(OperationStepRecord {
            step_id: 255,
            kind: PlanStepKind::VerifyBootEntry,
            outcome: ActionOutcome::Completed,
            detail: "Rollback marker recorded by the scaffold service.".to_string(),
        });
        self.journal_store.save_journal(&journal)?;
        Ok(journal)
    }

    pub fn repair_latest(&self) -> AppResult<OperationJournal> {
        let latest = self.journal_store.latest_operation_id()?;
        let mut journal = self.journal_store.load_journal(&latest)?;
        journal.status = OperationStatus::RepairSuggested;
        journal.summary = format!(
            "Repair guidance recorded for {}. Re-run verify or rollback on Windows to complete live remediation.",
            latest
        );
        self.journal_store.save_journal(&journal)?;
        Ok(journal)
    }

    fn validate_probe(&self, probe: &partbooter_common::MachineProbe) -> AppResult<()> {
        if !probe.supported {
            let detail = if probe.warnings.is_empty() {
                "no additional probe detail was recorded".to_string()
            } else {
                probe.warnings.join(" | ")
            };
            return Err(AppError::new(
                AppErrorKind::UnsupportedEnvironment,
                format!("PartBooter probe reported an unsupported host configuration: {detail}"),
            ));
        }
        if probe.firmware_mode != partbooter_common::FirmwareMode::Uefi {
            return Err(AppError::new(
                AppErrorKind::UnsupportedEnvironment,
                "PartBooter v1 only supports UEFI systems",
            ));
        }
        if probe.partition_style != PartitionStyle::Gpt {
            return Err(AppError::new(
                AppErrorKind::UnsupportedEnvironment,
                "PartBooter v1 only supports GPT disks",
            ));
        }
        if probe.esp.volume.trim().is_empty() {
            return Err(AppError::new(
                AppErrorKind::UnsupportedEnvironment,
                "PartBooter could not resolve the EFI System Partition volume path",
            ));
        }
        Ok(())
    }

    fn validate_payload(&self, payload: &PayloadSpec) -> AppResult<()> {
        if !payload.supported {
            return Err(AppError::new(
                AppErrorKind::Validation,
                payload
                    .notes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "unsupported payload".to_string()),
            ));
        }
        Ok(())
    }

    fn apply_checkpoint_step_record(
        &self,
        step: &PlanStep,
        checkpoint: &BackupCheckpoint,
    ) -> OperationStepRecord {
        match step.kind {
            PlanStepKind::BackupEsp => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!(
                    "Backed up the EFI System Partition to {}.",
                    checkpoint.esp_backup_dir.display()
                ),
            },
            PlanStepKind::SnapshotBootConfig => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!(
                    "Exported the Windows boot configuration to {}.",
                    checkpoint.bcd_store_path.display()
                ),
            },
            _ => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Skipped,
                detail:
                    "Execution beyond backup checkpointing is not implemented in this milestone."
                        .to_string(),
            },
        }
    }

    fn apply_winpe_plan(
        &self,
        plan: &ExecutionPlan,
        probe: &partbooter_common::MachineProbe,
        operation_id: &str,
        operation_root: &Path,
        checkpoint: &BackupCheckpoint,
    ) -> AppResult<OperationJournal> {
        let staging = self.stage_winpe_payload(
            probe,
            &plan.payload.source_path,
            &plan.target_volume,
            operation_id,
            operation_root,
        )?;
        let registration = match self.register_winpe_entry(&staging, &plan.payload.display_name) {
            Ok(registration) => registration,
            Err(error) => {
                let _ = self.restore_boot_config(&checkpoint.bcd_store_path);
                let _ = self.remove_staged_payload(&staging.stage_root, &staging.esp_stage_root);
                return Err(error);
            }
        };
        let loader_spec_path = self.write_winpe_loader_spec(
            operation_root,
            &staging,
            &registration,
            &plan.payload.display_name,
        )?;
        self.write_winpe_operation_metadata(
            operation_root,
            &staging,
            &registration,
            &loader_spec_path,
        )?;
        if !self.verify_registered_entry(&registration.entry_id)? {
            let _ = self.restore_boot_config(&checkpoint.bcd_store_path);
            let _ = self.remove_staged_payload(&staging.stage_root, &staging.esp_stage_root);
            return Err(AppError::new(
                AppErrorKind::Verification,
                format!(
                    "managed WinPE BCD entry {} could not be verified after registration",
                    registration.entry_id
                ),
            ));
        }

        let steps = plan
            .steps
            .iter()
            .map(|step| {
                self.apply_winpe_step_record(
                    step,
                    checkpoint,
                    &staging,
                    &registration,
                    &loader_spec_path,
                )
            })
            .collect::<Vec<_>>();

        Ok(OperationJournal {
            operation_id: operation_id.to_string(),
            plan_id: plan.plan_id.clone(),
            backup_root: plan.backup_root.clone(),
            status: OperationStatus::Applied,
            steps,
            summary: format!(
                "Applied WinPE staging and additive boot entry for {} on target volume {}.",
                plan.payload.display_name, plan.target_volume
            ),
        })
    }

    fn apply_winpe_step_record(
        &self,
        step: &PlanStep,
        checkpoint: &BackupCheckpoint,
        staging: &WinPeStaging,
        registration: &BootEntryRegistration,
        loader_spec_path: &Path,
    ) -> OperationStepRecord {
        match step.kind {
            PlanStepKind::BackupEsp | PlanStepKind::SnapshotBootConfig => {
                self.apply_checkpoint_step_record(step, checkpoint)
            }
            PlanStepKind::StagePayload => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!(
                    "Staged WinPE artifacts into {}.",
                    staging.stage_root.display()
                ),
            },
            PlanStepKind::WriteLoaderConfig => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!(
                    "Wrote WinPE loader settings to {}.",
                    loader_spec_path.display()
                ),
            },
            PlanStepKind::RegisterBootEntry => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!(
                    "Registered additive WinPE boot entry {} ({})",
                    registration.entry_id, registration.display_name
                ),
            },
            PlanStepKind::VerifyBootEntry => OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: "Verified managed WinPE files and BCD entry registration.".to_string(),
            },
        }
    }

    fn create_backup_checkpoint(
        &self,
        probe: &partbooter_common::MachineProbe,
        backup_root: &Path,
    ) -> AppResult<BackupCheckpoint> {
        match &self.probe_source {
            ProbeSource::Live => {
                WindowsApplyAdapter::create_backup_checkpoint(&probe.esp, backup_root)
            }
            #[cfg(test)]
            ProbeSource::Fixture(_) => self.create_fixture_backup_checkpoint(backup_root),
        }
    }

    fn stage_winpe_payload(
        &self,
        probe: &partbooter_common::MachineProbe,
        source_path: &str,
        target_volume: &str,
        operation_id: &str,
        _operation_root: &Path,
    ) -> AppResult<WinPeStaging> {
        match &self.probe_source {
            ProbeSource::Live => WindowsApplyAdapter::stage_winpe_payload(
                source_path,
                target_volume,
                operation_id,
                &probe.esp,
            ),
            #[cfg(test)]
            ProbeSource::Fixture(_) => {
                self.create_fixture_winpe_staging(source_path, _operation_root)
            }
        }
    }

    fn register_winpe_entry(
        &self,
        staging: &WinPeStaging,
        display_name: &str,
    ) -> AppResult<BootEntryRegistration> {
        match &self.probe_source {
            ProbeSource::Live => WindowsApplyAdapter::register_winpe_boot_entry(
                staging,
                &format!("PartBooter {display_name}"),
            ),
            #[cfg(test)]
            ProbeSource::Fixture(_) => Ok(self.fixture_winpe_registration(display_name)),
        }
    }

    fn verify_registered_entry(&self, entry_id: &str) -> AppResult<bool> {
        match &self.probe_source {
            ProbeSource::Live => WindowsApplyAdapter::verify_boot_entry(entry_id),
            #[cfg(test)]
            ProbeSource::Fixture(_) => Ok(true),
        }
    }

    fn restore_boot_config(&self, backup_store_path: &Path) -> AppResult<()> {
        match &self.probe_source {
            ProbeSource::Live => WindowsApplyAdapter::restore_boot_config(backup_store_path),
            #[cfg(test)]
            ProbeSource::Fixture(_) => Ok(()),
        }
    }

    fn remove_staged_payload(&self, stage_root: &Path, esp_stage_root: &Path) -> AppResult<()> {
        match &self.probe_source {
            ProbeSource::Live => {
                WindowsApplyAdapter::remove_staged_payload(stage_root, esp_stage_root)
            }
            #[cfg(test)]
            ProbeSource::Fixture(_) => {
                if stage_root.exists() {
                    fs::remove_dir_all(stage_root)?;
                }
                if esp_stage_root.exists() && esp_stage_root != stage_root {
                    fs::remove_dir_all(esp_stage_root)?;
                }
                Ok(())
            }
        }
    }

    fn write_winpe_loader_spec(
        &self,
        operation_root: &Path,
        staging: &WinPeStaging,
        registration: &BootEntryRegistration,
        display_name: &str,
    ) -> AppResult<PathBuf> {
        let path = operation_root.join("loader-spec.txt");
        let contents = [
            "PARTBOOTER_WINPE_LOADER_V1".to_string(),
            format!("display_name={display_name}"),
            format!("entry_id={}", registration.entry_id),
            format!("target_volume={}", staging.target_volume),
            format!("boot_wim_path={}", staging.boot_wim_path.display()),
            format!("boot_sdi_path={}", staging.boot_sdi_path.display()),
            format!("boot_sdi_relative_path={}", staging.boot_sdi_relative_path),
            "path=\\Windows\\System32\\winload.efi".to_string(),
            "systemroot=\\Windows".to_string(),
            "winpe=yes".to_string(),
            "detecthal=yes".to_string(),
            "nx=OptIn".to_string(),
        ]
        .join("\n");
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn write_winpe_operation_metadata(
        &self,
        operation_root: &Path,
        staging: &WinPeStaging,
        registration: &BootEntryRegistration,
        loader_spec_path: &Path,
    ) -> AppResult<()> {
        fs::write(operation_root.join("entry-id.txt"), &registration.entry_id)?;
        fs::write(
            operation_root.join("staging-root.txt"),
            staging.stage_root.display().to_string(),
        )?;
        fs::write(
            operation_root.join("esp-staging-root.txt"),
            staging.esp_stage_root.display().to_string(),
        )?;
        fs::write(
            operation_root.join("boot-wim-path.txt"),
            staging.boot_wim_path.display().to_string(),
        )?;
        fs::write(
            operation_root.join("boot-sdi-path.txt"),
            staging.boot_sdi_path.display().to_string(),
        )?;
        fs::write(
            operation_root.join("loader-spec-path.txt"),
            loader_spec_path.display().to_string(),
        )?;
        Ok(())
    }

    fn read_winpe_operation_metadata(
        &self,
        operation_root: &Path,
    ) -> AppResult<Option<WinPeOperationMetadata>> {
        let entry_id_path = operation_root.join("entry-id.txt");
        if !entry_id_path.exists() {
            return Ok(None);
        }

        Ok(Some(WinPeOperationMetadata {
            entry_id: fs::read_to_string(entry_id_path)?.trim().to_string(),
            stage_root: PathBuf::from(
                fs::read_to_string(operation_root.join("staging-root.txt"))?
                    .trim()
                    .to_string(),
            ),
            esp_stage_root: PathBuf::from(
                fs::read_to_string(operation_root.join("esp-staging-root.txt"))?
                    .trim()
                    .to_string(),
            ),
            boot_wim_path: PathBuf::from(
                fs::read_to_string(operation_root.join("boot-wim-path.txt"))?
                    .trim()
                    .to_string(),
            ),
            boot_sdi_path: PathBuf::from(
                fs::read_to_string(operation_root.join("boot-sdi-path.txt"))?
                    .trim()
                    .to_string(),
            ),
            loader_spec_path: PathBuf::from(
                fs::read_to_string(operation_root.join("loader-spec-path.txt"))?
                    .trim()
                    .to_string(),
            ),
        }))
    }

    fn remove_winpe_operation_metadata(&self, operation_root: &Path) -> AppResult<()> {
        for file_name in [
            "entry-id.txt",
            "staging-root.txt",
            "esp-staging-root.txt",
            "boot-wim-path.txt",
            "boot-sdi-path.txt",
            "loader-spec-path.txt",
            "loader-spec.txt",
        ] {
            let path = operation_root.join(file_name);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn create_fixture_backup_checkpoint(&self, backup_root: &Path) -> AppResult<BackupCheckpoint> {
        let esp_backup_dir = backup_root.join("esp");
        fs::create_dir_all(&esp_backup_dir)?;
        fs::write(esp_backup_dir.join("shimx64.efi"), "fixture-esp-backup")?;

        let bcd_store_path = backup_root.join("bcd-store.bak");
        fs::write(&bcd_store_path, "fixture-bcd-export")?;

        Ok(BackupCheckpoint {
            esp_backup_dir,
            bcd_store_path,
            notes: vec!["Fixture checkpoint created for tests.".to_string()],
        })
    }

    #[cfg(test)]
    fn create_fixture_winpe_staging(
        &self,
        source_path: &str,
        operation_root: &Path,
    ) -> AppResult<WinPeStaging> {
        let stage_root = operation_root.join("fixture-winpe");
        fs::create_dir_all(&stage_root)?;
        let boot_wim_path = stage_root.join("boot.wim");
        fs::write(&boot_wim_path, format!("fixture-winpe-from={source_path}"))?;
        let boot_sdi_path = stage_root.join("boot.sdi");
        fs::write(&boot_sdi_path, "fixture-boot-sdi")?;

        Ok(WinPeStaging {
            stage_root,
            esp_stage_root: operation_root.join("fixture-esp-winpe"),
            boot_wim_path,
            boot_sdi_path,
            boot_sdi_relative_path: r"\PartBooter\Operations\fixture\WinPE\boot.sdi".to_string(),
            target_volume: "D:".to_string(),
        })
    }

    #[cfg(test)]
    fn fixture_winpe_registration(&self, display_name: &str) -> BootEntryRegistration {
        BootEntryRegistration {
            entry_id: "{11111111-1111-1111-1111-111111111111}".to_string(),
            ramdisk_options_id: "{ramdiskoptions}".to_string(),
            display_name: format!("PartBooter {display_name}"),
        }
    }
}

struct WinPeOperationMetadata {
    entry_id: String,
    stage_root: PathBuf,
    esp_stage_root: PathBuf,
    boot_wim_path: PathBuf,
    boot_sdi_path: PathBuf,
    loader_spec_path: PathBuf,
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX_EPOCH")
        .as_secs()
        .to_string()
}

fn iso_timestamp() -> String {
    format!("{}Z", unique_suffix())
}

#[cfg(test)]
mod tests {
    use super::PartBooterService;
    use partbooter_common::{
        ActionOutcome, EspInfo, FirmwareMode, HostPlatform, MachineProbe, OperationStatus,
        PartitionStyle, PayloadKind, PlanStepKind,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("partbooter-core-test-{unique}"))
    }

    fn supported_probe_fixture() -> MachineProbe {
        MachineProbe {
            host_platform: HostPlatform::Windows,
            firmware_mode: FirmwareMode::Uefi,
            partition_style: PartitionStyle::Gpt,
            secure_boot_enabled: true,
            bitlocker_detected: false,
            esp: EspInfo {
                volume: "\\\\?\\Volume{ESP}".to_string(),
                mount_point: "S:\\".to_string(),
                filesystem: "FAT32".to_string(),
                free_space_mb: 512,
            },
            warnings: Vec::new(),
            supported: true,
        }
    }

    #[test]
    fn builds_supported_linux_plan() {
        let service = PartBooterService::with_probe_fixture(temp_root(), supported_probe_fixture());
        let plan = service
            .build_plan("C:\\images\\ubuntu-24.04.iso", "D:")
            .expect("plan should succeed");

        assert_eq!(plan.payload.kind, PayloadKind::LinuxIso);
        assert_eq!(plan.steps.len(), 6);
        assert!(!plan.additive_policy.replace_default_boot_path);
    }

    #[test]
    fn rejects_unsupported_payload() {
        let service = PartBooterService::with_probe_fixture(temp_root(), supported_probe_fixture());
        let error = service
            .build_plan("C:\\images\\unknown.iso", "D:")
            .expect_err("unknown payload should fail");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn applies_and_verifies_operation() {
        let service = PartBooterService::with_probe_fixture(temp_root(), supported_probe_fixture());
        let plan = service
            .build_plan("C:\\images\\winpe_boot.wim", "D:")
            .expect("plan should succeed");
        let journal = service.apply_plan(&plan).expect("apply should succeed");
        assert_eq!(journal.status, OperationStatus::Applied);
        assert_eq!(journal.steps[0].outcome, ActionOutcome::Completed);
        assert_eq!(journal.steps[1].kind, PlanStepKind::SnapshotBootConfig);
        assert_eq!(journal.steps[2].outcome, ActionOutcome::Completed);
        assert_eq!(journal.steps[4].outcome, ActionOutcome::Completed);

        let report = service
            .verify_operation(&journal.operation_id)
            .expect("verify should succeed");
        assert!(report.verified);
        assert!(report.backup_artifacts_present);
        assert!(report.operation_plan_present);
        assert!(report.boot_entry_registered);
        assert!(report.staged_artifacts_present);
    }

    #[test]
    fn rolls_back_applied_winpe_operation() {
        let service = PartBooterService::with_probe_fixture(temp_root(), supported_probe_fixture());
        let plan = service
            .build_plan("C:\\images\\winpe_boot.wim", "D:")
            .expect("plan should succeed");
        let journal = service.apply_plan(&plan).expect("apply should succeed");

        let rolled_back = service
            .rollback_operation(&journal.operation_id)
            .expect("rollback should succeed");
        assert_eq!(rolled_back.status, OperationStatus::RolledBack);

        let report = service
            .verify_operation(&journal.operation_id)
            .expect("verify should still succeed");
        assert!(!report.staged_artifacts_present);
    }

    #[test]
    fn rejects_unsupported_probe_fixture() {
        let mut probe = supported_probe_fixture();
        probe.firmware_mode = FirmwareMode::Bios;
        probe.supported = false;
        probe
            .warnings
            .push("Firmware mode is not UEFI.".to_string());

        let service = PartBooterService::with_probe_fixture(temp_root(), probe);
        let error = service
            .build_plan("C:\\images\\winpe_boot.wim", "D:")
            .expect_err("unsupported host probe should fail");
        assert_eq!(error.exit_code(), 2);
        assert!(
            error.message().contains("Firmware mode is not UEFI."),
            "unsupported probe error should include probe warnings"
        );
    }
}
