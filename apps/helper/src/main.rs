use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use partbooter_common::{AppError, AppErrorKind, ExecutionPlan};
use partbooter_core::PartBooterService;

fn main() {
    if let Err(error) = run() {
        eprintln!("helper error: {error}");
        process::exit(error.exit_code());
    }
}

fn run() -> Result<(), AppError> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err(usage_error());
    }

    let service = PartBooterService::new(default_state_root());

    match args[0].as_str() {
        "execute-plan" => {
            let plan_path = get_flag_value(&args, "--plan")?;
            let content = fs::read_to_string(plan_path)?;
            let plan = ExecutionPlan::from_plan_file(&content)?;
            let journal = service.apply_plan(&plan)?;
            println!("{}", journal.to_json());
            Ok(())
        }
        "verify" => {
            let operation_id = get_flag_value(&args, "--operation")?;
            let report = service.verify_operation(&operation_id)?;
            println!("{}", report.to_json());
            Ok(())
        }
        "rollback" => {
            let operation_id = get_flag_value(&args, "--operation")?;
            let journal = service.rollback_operation(&operation_id)?;
            println!("{}", journal.to_json());
            Ok(())
        }
        "repair" => {
            if !args.iter().any(|arg| arg == "--latest") {
                return Err(AppError::new(
                    AppErrorKind::Usage,
                    "repair currently requires --latest",
                ));
            }
            let journal = service.repair_latest()?;
            println!("{}", journal.to_json());
            Ok(())
        }
        _ => Err(usage_error()),
    }
}

fn get_flag_value(args: &[String], flag: &str) -> Result<String, AppError> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
        .ok_or_else(|| AppError::new(AppErrorKind::Usage, format!("missing required flag {flag}")))
}

fn default_state_root() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".partbooter")
}

fn usage_error() -> AppError {
    AppError::new(
        AppErrorKind::Usage,
        "usage: partbooter-helper execute-plan --plan <file> | verify --operation <id> | rollback --operation <id> | repair --latest",
    )
}
