use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;

use partbooter_common::{AppError, AppErrorKind, ExecutionPlan};
use partbooter_core::PartBooterService;

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
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
        "probe" => {
            let json = args.iter().any(|arg| arg == "--json");
            let probe = service.probe_machine();
            if json {
                println!("{}", probe.to_json());
            } else {
                println!("host platform: {}", probe.host_platform.as_str());
                println!("firmware mode: {}", probe.firmware_mode.as_str());
                println!("partition style: {}", probe.partition_style.as_str());
                println!("secure boot: {}", probe.secure_boot_enabled);
                println!("bitlocker detected: {}", probe.bitlocker_detected);
                println!("esp volume: {}", probe.esp.volume);
            }
            Ok(())
        }
        "plan" => {
            let payload = get_flag_value(&args, "--payload")?;
            let target = get_flag_value(&args, "--target")?;
            let json = args.iter().any(|arg| arg == "--json");
            let out_path = optional_flag_value(&args, "--out");

            let plan = service.build_plan(payload, target)?;
            if json {
                println!("{}", plan.to_json());
            } else {
                println!("plan id: {}", plan.plan_id);
                println!("payload: {}", plan.payload.display_name);
                println!("target volume: {}", plan.target_volume);
            }

            if let Some(path) = out_path {
                fs::write(&path, plan.to_plan_file())?;
                println!("wrote plan file to {}", path);
            }
            Ok(())
        }
        "apply" => {
            let plan_path = get_flag_value(&args, "--plan")?;
            let plan = read_plan_file(plan_path)?;
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

fn read_plan_file(path: impl Into<String>) -> Result<ExecutionPlan, AppError> {
    let path = path.into();
    let content = fs::read_to_string(&path)?;
    ExecutionPlan::from_plan_file(&content)
}

fn get_flag_value(args: &[String], flag: &str) -> Result<String, AppError> {
    optional_flag_value(args, flag).ok_or_else(|| {
        AppError::new(
            AppErrorKind::Usage,
            format!("missing required flag {flag}; run with a supported command signature"),
        )
    })
}

fn optional_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn default_state_root() -> PathBuf {
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".partbooter")
}

fn usage_error() -> AppError {
    AppError::new(
        AppErrorKind::Usage,
        "usage: partbooter probe [--json] | plan --payload <path> --target <volume> [--json] [--out <file>] | apply --plan <file> | verify --operation <id> | rollback --operation <id> | repair --latest",
    )
}
