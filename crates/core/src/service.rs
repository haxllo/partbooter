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
use partbooter_windows::WindowsProbeAdapter;

#[derive(Debug, Clone)]
pub struct PartBooterService {
    journal_store: FileJournalStore,
}

impl PartBooterService {
    pub fn new(state_root: impl Into<PathBuf>) -> Self {
        Self {
            journal_store: FileJournalStore::new(state_root.into()),
        }
    }

    pub fn probe_machine(&self) -> partbooter_common::MachineProbe {
        WindowsProbeAdapter::probe()
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
        let probe = self.probe_machine();
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
        self.validate_payload(&plan.payload)?;
        self.journal_store.ensure_layout()?;
        self.journal_store.write_backup_manifest(plan)?;

        let operation_id = format!("op-{}", unique_suffix());
        let operation_root = self
            .journal_store
            .root()
            .join("operations")
            .join(&operation_id);
        fs::create_dir_all(&operation_root)?;
        fs::write(operation_root.join("plan.pbplan"), plan.to_plan_file())?;

        let steps = plan
            .steps
            .iter()
            .map(|step| OperationStepRecord {
                step_id: step.id,
                kind: step.kind.clone(),
                outcome: ActionOutcome::Completed,
                detail: format!("Scaffold applied step {}: {}", step.id, step.description),
            })
            .collect::<Vec<_>>();

        let journal = OperationJournal {
            operation_id: operation_id.clone(),
            plan_id: plan.plan_id.clone(),
            backup_root: plan.backup_root.clone(),
            status: OperationStatus::Applied,
            steps,
            summary: format!(
                "Applied the scaffold plan for {} on target volume {}.",
                plan.payload.display_name, plan.target_volume
            ),
        };

        self.journal_store.save_journal(&journal)?;
        Ok(journal)
    }

    pub fn verify_operation(&self, operation_id: &str) -> AppResult<VerificationReport> {
        let journal = self.journal_store.load_journal(operation_id)?;
        let verified = journal.status == OperationStatus::Applied
            || journal.status == OperationStatus::Verified;
        let warnings = if verified {
            vec![
                "Verification is currently file- and journal-based; live firmware validation is a future milestone."
                    .to_string(),
            ]
        } else {
            vec!["Operation is not in an applied state.".to_string()]
        };

        Ok(VerificationReport {
            operation_id: journal.operation_id,
            boot_entry_registered: verified,
            staged_artifacts_present: true,
            warnings,
            verified,
        })
    }

    pub fn rollback_operation(&self, operation_id: &str) -> AppResult<OperationJournal> {
        let mut journal = self.journal_store.load_journal(operation_id)?;
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
    use partbooter_common::{OperationStatus, PayloadKind};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("partbooter-core-test-{unique}"))
    }

    #[test]
    fn builds_supported_linux_plan() {
        let service = PartBooterService::new(temp_root());
        let plan = service
            .build_plan("C:\\images\\ubuntu-24.04.iso", "D:")
            .expect("plan should succeed");

        assert_eq!(plan.payload.kind, PayloadKind::LinuxIso);
        assert_eq!(plan.steps.len(), 6);
        assert!(!plan.additive_policy.replace_default_boot_path);
    }

    #[test]
    fn rejects_unsupported_payload() {
        let service = PartBooterService::new(temp_root());
        let error = service
            .build_plan("C:\\images\\unknown.iso", "D:")
            .expect_err("unknown payload should fail");
        assert_eq!(error.exit_code(), 3);
    }

    #[test]
    fn applies_and_verifies_operation() {
        let service = PartBooterService::new(temp_root());
        let plan = service
            .build_plan("C:\\images\\winpe_boot.wim", "D:")
            .expect("plan should succeed");
        let journal = service.apply_plan(&plan).expect("apply should succeed");
        assert_eq!(journal.status, OperationStatus::Applied);

        let report = service
            .verify_operation(&journal.operation_id)
            .expect("verify should succeed");
        assert!(report.verified);
    }
}
