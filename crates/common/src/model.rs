use crate::error::{AppError, AppErrorKind, AppResult};
use crate::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostPlatform {
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareMode {
    Uefi,
    Bios,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionStyle {
    Gpt,
    Mbr,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadKind {
    LinuxIso,
    WinPe,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanStepKind {
    BackupEsp,
    SnapshotBootConfig,
    StagePayload,
    WriteLoaderConfig,
    RegisterBootEntry,
    VerifyBootEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionOutcome {
    Pending,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationStatus {
    Planned,
    Applied,
    Verified,
    RolledBack,
    RepairSuggested,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdditiveBootPolicy {
    pub replace_default_boot_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EspInfo {
    pub volume: String,
    pub mount_point: String,
    pub filesystem: String,
    pub free_space_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineProbe {
    pub host_platform: HostPlatform,
    pub firmware_mode: FirmwareMode,
    pub partition_style: PartitionStyle,
    pub secure_boot_enabled: bool,
    pub bitlocker_detected: bool,
    pub esp: EspInfo,
    pub warnings: Vec<String>,
    pub supported: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadSpec {
    pub source_path: String,
    pub kind: PayloadKind,
    pub display_name: String,
    pub profile: String,
    pub supported: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskFlag {
    pub code: String,
    pub level: RiskLevel,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub id: u8,
    pub kind: PlanStepKind,
    pub requires_elevation: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub version: u8,
    pub plan_id: String,
    pub target_volume: String,
    pub payload: PayloadSpec,
    pub backup_root: String,
    pub additive_policy: AdditiveBootPolicy,
    pub risk_flags: Vec<RiskFlag>,
    pub steps: Vec<PlanStep>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationStepRecord {
    pub step_id: u8,
    pub kind: PlanStepKind,
    pub outcome: ActionOutcome,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationJournal {
    pub operation_id: String,
    pub plan_id: String,
    pub backup_root: String,
    pub status: OperationStatus,
    pub steps: Vec<OperationStepRecord>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub operation_id: String,
    pub boot_entry_registered: bool,
    pub staged_artifacts_present: bool,
    pub warnings: Vec<String>,
    pub verified: bool,
}

impl HostPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Windows => "windows",
        }
    }
}

impl FirmwareMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Uefi => "uefi",
            Self::Bios => "bios",
        }
    }
}

impl PartitionStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Gpt => "gpt",
            Self::Mbr => "mbr",
            Self::Unknown => "unknown",
        }
    }
}

impl PayloadKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LinuxIso => "linux-iso",
            Self::WinPe => "winpe",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "linux-iso" => Some(Self::LinuxIso),
            "winpe" => Some(Self::WinPe),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Blocker => "blocker",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "blocker" => Some(Self::Blocker),
            _ => None,
        }
    }
}

impl PlanStepKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BackupEsp => "backup-esp",
            Self::SnapshotBootConfig => "snapshot-boot-config",
            Self::StagePayload => "stage-payload",
            Self::WriteLoaderConfig => "write-loader-config",
            Self::RegisterBootEntry => "register-boot-entry",
            Self::VerifyBootEntry => "verify-boot-entry",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "backup-esp" => Some(Self::BackupEsp),
            "snapshot-boot-config" => Some(Self::SnapshotBootConfig),
            "stage-payload" => Some(Self::StagePayload),
            "write-loader-config" => Some(Self::WriteLoaderConfig),
            "register-boot-entry" => Some(Self::RegisterBootEntry),
            "verify-boot-entry" => Some(Self::VerifyBootEntry),
            _ => None,
        }
    }
}

impl ActionOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }
}

impl OperationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Applied => "applied",
            Self::Verified => "verified",
            Self::RolledBack => "rolled-back",
            Self::RepairSuggested => "repair-suggested",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "planned" => Some(Self::Planned),
            "applied" => Some(Self::Applied),
            "verified" => Some(Self::Verified),
            "rolled-back" => Some(Self::RolledBack),
            "repair-suggested" => Some(Self::RepairSuggested),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

impl MachineProbe {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("hostPlatform", json::string(self.host_platform.as_str())),
            ("firmwareMode", json::string(self.firmware_mode.as_str())),
            (
                "partitionStyle",
                json::string(self.partition_style.as_str()),
            ),
            (
                "secureBootEnabled",
                json::bool_value(self.secure_boot_enabled).to_string(),
            ),
            (
                "bitlockerDetected",
                json::bool_value(self.bitlocker_detected).to_string(),
            ),
            (
                "esp",
                json::object(&[
                    ("volume", json::string(&self.esp.volume)),
                    ("mountPoint", json::string(&self.esp.mount_point)),
                    ("filesystem", json::string(&self.esp.filesystem)),
                    ("freeSpaceMb", self.esp.free_space_mb.to_string()),
                ]),
            ),
            (
                "warnings",
                json::array(
                    &self
                        .warnings
                        .iter()
                        .map(|warning| json::string(warning))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("supported", json::bool_value(self.supported).to_string()),
        ])
    }
}

impl PayloadSpec {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("sourcePath", json::string(&self.source_path)),
            ("kind", json::string(self.kind.as_str())),
            ("displayName", json::string(&self.display_name)),
            ("profile", json::string(&self.profile)),
            ("supported", json::bool_value(self.supported).to_string()),
            (
                "notes",
                json::array(
                    &self
                        .notes
                        .iter()
                        .map(|note| json::string(note))
                        .collect::<Vec<_>>(),
                ),
            ),
        ])
    }
}

impl RiskFlag {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("code", json::string(&self.code)),
            ("level", json::string(self.level.as_str())),
            ("message", json::string(&self.message)),
        ])
    }
}

impl PlanStep {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("id", self.id.to_string()),
            ("kind", json::string(self.kind.as_str())),
            (
                "requiresElevation",
                json::bool_value(self.requires_elevation).to_string(),
            ),
            ("description", json::string(&self.description)),
        ])
    }
}

impl ExecutionPlan {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("version", self.version.to_string()),
            ("planId", json::string(&self.plan_id)),
            ("targetVolume", json::string(&self.target_volume)),
            ("payload", self.payload.to_json()),
            ("backupRoot", json::string(&self.backup_root)),
            (
                "additivePolicy",
                json::object(&[(
                    "replaceDefaultBootPath",
                    json::bool_value(self.additive_policy.replace_default_boot_path).to_string(),
                )]),
            ),
            (
                "riskFlags",
                json::array(
                    &self
                        .risk_flags
                        .iter()
                        .map(RiskFlag::to_json)
                        .collect::<Vec<_>>(),
                ),
            ),
            (
                "steps",
                json::array(&self.steps.iter().map(PlanStep::to_json).collect::<Vec<_>>()),
            ),
            ("createdAt", json::string(&self.created_at)),
        ])
    }

    pub fn to_plan_file(&self) -> String {
        let mut lines = vec![
            "PARTBOOTER_PLAN_V1".to_string(),
            format!("version\t{}", self.version),
            format!("plan_id\t{}", json::encode_field(&self.plan_id)),
            format!("target_volume\t{}", json::encode_field(&self.target_volume)),
            format!("backup_root\t{}", json::encode_field(&self.backup_root)),
            format!("created_at\t{}", json::encode_field(&self.created_at)),
            format!(
                "replace_default_boot_path\t{}",
                self.additive_policy.replace_default_boot_path
            ),
            format!(
                "payload\t{}\t{}\t{}\t{}\t{}",
                self.payload.kind.as_str(),
                json::encode_field(&self.payload.source_path),
                json::encode_field(&self.payload.display_name),
                json::encode_field(&self.payload.profile),
                self.payload.supported
            ),
        ];

        for note in &self.payload.notes {
            lines.push(format!("payload_note\t{}", json::encode_field(note)));
        }
        for risk in &self.risk_flags {
            lines.push(format!(
                "risk\t{}\t{}\t{}",
                risk.level.as_str(),
                json::encode_field(&risk.code),
                json::encode_field(&risk.message)
            ));
        }
        for step in &self.steps {
            lines.push(format!(
                "step\t{}\t{}\t{}\t{}",
                step.id,
                step.kind.as_str(),
                step.requires_elevation,
                json::encode_field(&step.description)
            ));
        }
        lines.join("\n")
    }

    pub fn from_plan_file(input: &str) -> AppResult<Self> {
        let mut version = None;
        let mut plan_id = None;
        let mut target_volume = None;
        let mut backup_root = None;
        let mut created_at = None;
        let mut replace_default_boot_path = false;
        let mut payload_kind = None;
        let mut payload_source_path = None;
        let mut payload_display_name = None;
        let mut payload_profile = None;
        let mut payload_supported = None;
        let mut payload_notes = Vec::new();
        let mut risk_flags = Vec::new();
        let mut steps = Vec::new();

        let mut lines = input.lines();
        match lines.next() {
            Some("PARTBOOTER_PLAN_V1") => {}
            _ => {
                return Err(AppError::new(
                    AppErrorKind::Validation,
                    "invalid PartBooter plan file header",
                ));
            }
        }

        for line in lines {
            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.is_empty() {
                continue;
            }
            match parts[0] {
                "version" if parts.len() == 2 => version = parts[1].parse::<u8>().ok(),
                "plan_id" if parts.len() == 2 => plan_id = json::decode_field(parts[1]),
                "target_volume" if parts.len() == 2 => target_volume = json::decode_field(parts[1]),
                "backup_root" if parts.len() == 2 => backup_root = json::decode_field(parts[1]),
                "created_at" if parts.len() == 2 => created_at = json::decode_field(parts[1]),
                "replace_default_boot_path" if parts.len() == 2 => {
                    replace_default_boot_path = parts[1] == "true";
                }
                "payload" if parts.len() == 6 => {
                    payload_kind = PayloadKind::from_str(parts[1]);
                    payload_source_path = json::decode_field(parts[2]);
                    payload_display_name = json::decode_field(parts[3]);
                    payload_profile = json::decode_field(parts[4]);
                    payload_supported = Some(parts[5] == "true");
                }
                "payload_note" if parts.len() == 2 => {
                    payload_notes.push(json::decode_field(parts[1]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid payload note encoding")
                    })?);
                }
                "risk" if parts.len() == 4 => {
                    let level = RiskLevel::from_str(parts[1]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid risk level")
                    })?;
                    let code = json::decode_field(parts[2]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid risk code encoding")
                    })?;
                    let message = json::decode_field(parts[3]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid risk message encoding")
                    })?;
                    risk_flags.push(RiskFlag {
                        code,
                        level,
                        message,
                    });
                }
                "step" if parts.len() == 5 => {
                    let id = parts[1]
                        .parse::<u8>()
                        .map_err(|_| AppError::new(AppErrorKind::Validation, "invalid step id"))?;
                    let kind = PlanStepKind::from_str(parts[2]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid step kind")
                    })?;
                    let requires_elevation = parts[3] == "true";
                    let description = json::decode_field(parts[4]).ok_or_else(|| {
                        AppError::new(
                            AppErrorKind::Validation,
                            "invalid step description encoding",
                        )
                    })?;
                    steps.push(PlanStep {
                        id,
                        kind,
                        requires_elevation,
                        description,
                    });
                }
                _ => {}
            }
        }

        let payload = PayloadSpec {
            source_path: payload_source_path.ok_or_else(|| {
                AppError::new(AppErrorKind::Validation, "missing payload source path")
            })?,
            kind: payload_kind
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing payload kind"))?,
            display_name: payload_display_name.ok_or_else(|| {
                AppError::new(AppErrorKind::Validation, "missing payload display name")
            })?,
            profile: payload_profile.ok_or_else(|| {
                AppError::new(AppErrorKind::Validation, "missing payload profile")
            })?,
            supported: payload_supported.ok_or_else(|| {
                AppError::new(AppErrorKind::Validation, "missing payload support state")
            })?,
            notes: payload_notes,
        };

        Ok(Self {
            version: version
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing plan version"))?,
            plan_id: plan_id
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing plan id"))?,
            target_volume: target_volume
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing target volume"))?,
            payload,
            backup_root: backup_root
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing backup root"))?,
            additive_policy: AdditiveBootPolicy {
                replace_default_boot_path,
            },
            risk_flags,
            steps,
            created_at: created_at
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing plan timestamp"))?,
        })
    }
}

impl OperationStepRecord {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("stepId", self.step_id.to_string()),
            ("kind", json::string(self.kind.as_str())),
            ("outcome", json::string(self.outcome.as_str())),
            ("detail", json::string(&self.detail)),
        ])
    }
}

impl OperationJournal {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("operationId", json::string(&self.operation_id)),
            ("planId", json::string(&self.plan_id)),
            ("backupRoot", json::string(&self.backup_root)),
            ("status", json::string(self.status.as_str())),
            (
                "steps",
                json::array(
                    &self
                        .steps
                        .iter()
                        .map(OperationStepRecord::to_json)
                        .collect::<Vec<_>>(),
                ),
            ),
            ("summary", json::string(&self.summary)),
        ])
    }

    pub fn to_record_file(&self) -> String {
        let mut lines = vec![
            "PARTBOOTER_OPERATION_V1".to_string(),
            format!("operation_id\t{}", json::encode_field(&self.operation_id)),
            format!("plan_id\t{}", json::encode_field(&self.plan_id)),
            format!("backup_root\t{}", json::encode_field(&self.backup_root)),
            format!("status\t{}", self.status.as_str()),
            format!("summary\t{}", json::encode_field(&self.summary)),
        ];
        for step in &self.steps {
            lines.push(format!(
                "step\t{}\t{}\t{}\t{}",
                step.step_id,
                step.kind.as_str(),
                step.outcome.as_str(),
                json::encode_field(&step.detail)
            ));
        }
        lines.join("\n")
    }

    pub fn from_record_file(input: &str) -> AppResult<Self> {
        let mut operation_id = None;
        let mut plan_id = None;
        let mut backup_root = None;
        let mut status = None;
        let mut summary = None;
        let mut steps = Vec::new();

        let mut lines = input.lines();
        match lines.next() {
            Some("PARTBOOTER_OPERATION_V1") => {}
            _ => {
                return Err(AppError::new(
                    AppErrorKind::Validation,
                    "invalid PartBooter operation file header",
                ));
            }
        }

        for line in lines {
            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.is_empty() {
                continue;
            }
            match parts[0] {
                "operation_id" if parts.len() == 2 => operation_id = json::decode_field(parts[1]),
                "plan_id" if parts.len() == 2 => plan_id = json::decode_field(parts[1]),
                "backup_root" if parts.len() == 2 => backup_root = json::decode_field(parts[1]),
                "status" if parts.len() == 2 => status = OperationStatus::from_str(parts[1]),
                "summary" if parts.len() == 2 => summary = json::decode_field(parts[1]),
                "step" if parts.len() == 5 => {
                    let step_id = parts[1].parse::<u8>().map_err(|_| {
                        AppError::new(AppErrorKind::Validation, "invalid operation step id")
                    })?;
                    let kind = PlanStepKind::from_str(parts[2]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid operation step kind")
                    })?;
                    let outcome = ActionOutcome::from_str(parts[3]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid operation step outcome")
                    })?;
                    let detail = json::decode_field(parts[4]).ok_or_else(|| {
                        AppError::new(AppErrorKind::Validation, "invalid operation step detail")
                    })?;
                    steps.push(OperationStepRecord {
                        step_id,
                        kind,
                        outcome,
                        detail,
                    });
                }
                _ => {}
            }
        }

        Ok(Self {
            operation_id: operation_id
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing operation id"))?,
            plan_id: plan_id
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing plan id"))?,
            backup_root: backup_root
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing backup root"))?,
            status: status
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing status"))?,
            steps,
            summary: summary
                .ok_or_else(|| AppError::new(AppErrorKind::Validation, "missing summary"))?,
        })
    }
}

impl VerificationReport {
    pub fn to_json(&self) -> String {
        json::object(&[
            ("operationId", json::string(&self.operation_id)),
            (
                "bootEntryRegistered",
                json::bool_value(self.boot_entry_registered).to_string(),
            ),
            (
                "stagedArtifactsPresent",
                json::bool_value(self.staged_artifacts_present).to_string(),
            ),
            (
                "warnings",
                json::array(
                    &self
                        .warnings
                        .iter()
                        .map(|warning| json::string(warning))
                        .collect::<Vec<_>>(),
                ),
            ),
            ("verified", json::bool_value(self.verified).to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_round_trip_keeps_payload_and_steps() {
        let plan = ExecutionPlan {
            version: 1,
            plan_id: "plan-123".to_string(),
            target_volume: "D:".to_string(),
            payload: PayloadSpec {
                source_path: "C:\\images\\ubuntu.iso".to_string(),
                kind: PayloadKind::LinuxIso,
                display_name: "Ubuntu Live".to_string(),
                profile: "ubuntu-live".to_string(),
                supported: true,
                notes: vec!["Supports loopback staging".to_string()],
            },
            backup_root: ".partbooter/backups/plan-123".to_string(),
            additive_policy: AdditiveBootPolicy {
                replace_default_boot_path: false,
            },
            risk_flags: vec![RiskFlag {
                code: "secure-boot-review".to_string(),
                level: RiskLevel::Warning,
                message: "Secure Boot review remains required.".to_string(),
            }],
            steps: vec![PlanStep {
                id: 1,
                kind: PlanStepKind::BackupEsp,
                requires_elevation: true,
                description: "Backup the current ESP.".to_string(),
            }],
            created_at: "2026-03-10T00:00:00Z".to_string(),
        };

        let encoded = plan.to_plan_file();
        let decoded = ExecutionPlan::from_plan_file(&encoded).expect("plan should decode");
        assert_eq!(decoded.plan_id, "plan-123");
        assert_eq!(decoded.payload.kind, PayloadKind::LinuxIso);
        assert_eq!(decoded.steps.len(), 1);
        assert_eq!(decoded.risk_flags.len(), 1);
    }

    #[test]
    fn operation_round_trip_keeps_status() {
        let operation = OperationJournal {
            operation_id: "op-1".to_string(),
            plan_id: "plan-1".to_string(),
            backup_root: ".partbooter/backups/op-1".to_string(),
            status: OperationStatus::Applied,
            steps: vec![OperationStepRecord {
                step_id: 1,
                kind: PlanStepKind::StagePayload,
                outcome: ActionOutcome::Completed,
                detail: "Payload staged.".to_string(),
            }],
            summary: "Applied successfully".to_string(),
        };

        let encoded = operation.to_record_file();
        let decoded =
            OperationJournal::from_record_file(&encoded).expect("operation should decode");
        assert_eq!(decoded.operation_id, "op-1");
        assert_eq!(decoded.status, OperationStatus::Applied);
        assert_eq!(decoded.steps.len(), 1);
    }
}
