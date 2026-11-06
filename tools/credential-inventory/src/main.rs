//! Never loads server configuration, opens application stores, or prints raw errors.
use std::path::PathBuf;
use std::process::ExitCode;
#[tokio::main]
async fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let snapshot = match (arguments.next(), arguments.next(), arguments.next()) {
        (Some(flag), Some(path), None) if flag == "--snapshot" => PathBuf::from(path),
        _ => {
            eprintln!("usage: rbit-credential-inventory --snapshot OFFLINE_COPY");
            return ExitCode::from(2);
        }
    };
    match rbit_credential_inventory::scan_snapshot(&snapshot).await {
        Ok(report) => {
            if let Ok(json) = serde_json::to_string_pretty(&report) {
                println!("{json}");
                if report.complete { ExitCode::SUCCESS } else { ExitCode::from(3) }
            } else {
                eprintln!("REPORT_ENCODING_FAILED");
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{}", error.code());
            ExitCode::FAILURE
        }
    }
}
