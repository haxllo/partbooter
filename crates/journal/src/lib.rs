use std::fs;
use std::path::{Path, PathBuf};

use partbooter_common::{AppError, AppErrorKind, AppResult, ExecutionPlan, OperationJournal};

#[derive(Debug, Clone)]
pub struct FileJournalStore {
    root: PathBuf,
}

impl FileJournalStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_layout(&self) -> AppResult<()> {
        fs::create_dir_all(self.operations_dir())?;
        fs::create_dir_all(self.backups_dir())?;
        Ok(())
    }

    pub fn write_backup_manifest(
        &self,
        plan: &ExecutionPlan,
        esp_backup_dir: &Path,
        bcd_store_path: &Path,
        notes: &[String],
    ) -> AppResult<PathBuf> {
        let backup_root = self.backups_dir().join(&plan.plan_id);
        fs::create_dir_all(&backup_root)?;

        let manifest_path = backup_root.join("manifest.txt");
        let mut lines = vec![
            "PARTBOOTER_BACKUP_MANIFEST_V1".to_string(),
            format!("plan_id={}", plan.plan_id),
            format!("payload={}", plan.payload.source_path),
            format!("target_volume={}", plan.target_volume),
            format!("esp_backup_dir={}", esp_backup_dir.display()),
            format!("bcd_store_path={}", bcd_store_path.display()),
        ];
        for note in notes {
            lines.push(format!("note={note}"));
        }
        let manifest = lines.join("\n");
        fs::write(&manifest_path, manifest)?;
        Ok(manifest_path)
    }

    pub fn save_journal(&self, journal: &OperationJournal) -> AppResult<PathBuf> {
        self.ensure_layout()?;
        let path = self
            .operations_dir()
            .join(format!("{}.pbop", journal.operation_id));
        fs::write(&path, journal.to_record_file())?;
        Ok(path)
    }

    pub fn load_journal(&self, operation_id: &str) -> AppResult<OperationJournal> {
        let path = self.operations_dir().join(format!("{operation_id}.pbop"));
        let content = fs::read_to_string(path)?;
        OperationJournal::from_record_file(&content)
    }

    pub fn operation_dir(&self, operation_id: &str) -> PathBuf {
        self.operations_dir().join(operation_id)
    }

    pub fn operation_plan_path(&self, operation_id: &str) -> PathBuf {
        self.operation_dir(operation_id).join("plan.pbplan")
    }

    pub fn backup_root_for_plan(&self, plan_id: &str) -> PathBuf {
        self.backups_dir().join(plan_id)
    }

    pub fn latest_operation_id(&self) -> AppResult<String> {
        let entries = fs::read_dir(self.operations_dir())?;
        let mut latest = None;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("pbop") {
                continue;
            }
            let metadata = entry.metadata()?;
            let modified = metadata.modified().map_err(|error| {
                AppError::new(
                    AppErrorKind::Io,
                    format!("failed to read operation timestamp: {error}"),
                )
            })?;

            let operation_id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(ToOwned::to_owned)
                .ok_or_else(|| {
                    AppError::new(AppErrorKind::Validation, "invalid operation filename")
                })?;

            match &latest {
                Some((current_modified, _)) if &modified <= current_modified => {}
                _ => latest = Some((modified, operation_id)),
            }
        }

        latest.map(|(_, operation_id)| operation_id).ok_or_else(|| {
            AppError::new(
                AppErrorKind::Validation,
                "no recorded operations exist in the journal store",
            )
        })
    }

    fn operations_dir(&self) -> PathBuf {
        self.root.join("operations")
    }

    fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }
}

#[cfg(test)]
mod tests {
    use super::FileJournalStore;
    use partbooter_common::{
        ActionOutcome, OperationJournal, OperationStatus, OperationStepRecord, PlanStepKind,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("partbooter-journal-test-{unique}"))
    }

    #[test]
    fn saves_and_loads_operation_journal() {
        let root = temp_root();
        let store = FileJournalStore::new(&root);
        let journal = OperationJournal {
            operation_id: "op-42".to_string(),
            plan_id: "plan-42".to_string(),
            backup_root: ".partbooter/backups/plan-42".to_string(),
            status: OperationStatus::Applied,
            steps: vec![OperationStepRecord {
                step_id: 1,
                kind: PlanStepKind::BackupEsp,
                outcome: ActionOutcome::Completed,
                detail: "ESP backed up".to_string(),
            }],
            summary: "Operation completed".to_string(),
        };

        store.save_journal(&journal).expect("save should succeed");
        let loaded = store.load_journal("op-42").expect("load should succeed");
        assert_eq!(loaded.operation_id, "op-42");
        assert_eq!(loaded.status, OperationStatus::Applied);
    }
}
