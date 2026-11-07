//! Finite known fixture family; no production shell or unsafe signal handlers.

use super::{identity::Identity, wire};
use anyhow::{Context, Result, ensure};
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

pub(super) fn run(args: &[OsString]) -> Result<()> {
    let role = args.first().and_then(|arg| arg.to_str()).context("fixture role missing")?;
    ensure!(["root", "helper", "grandchild", "sibling"].contains(&role), "unknown fixture role");
    let directory = PathBuf::from(args.get(1).context("fixture directory missing")?);
    let scenario = args.get(2).and_then(|arg| arg.to_str()).context("fixture scenario missing")?;
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let entered = runtime.enter();
    let mut term =
        if scenario == "escalation" { Some(tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?) } else { None };
    drop(entered);
    let identity = Identity::capture(i32::try_from(std::process::id())?)?;
    wire::write(&directory.join(format!("{role}.json")), &identity)?;
    let next = match role {
        "root" => Some("helper"),
        "helper" => Some("grandchild"),
        _ => None,
    };
    let _child = next
        .map(|next| {
            Command::new(std::env::current_exe()?)
                .arg("fixture-unix")
                .arg(next)
                .arg(&directory)
                .arg(scenario)
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
        })
        .transpose()?;
    println!("{role} ready");
    let mut escaped = false;
    loop {
        if scenario == "escape" && role == "grandchild" && !escaped && directory.join("escape-now").exists() {
            // The controller already owns this fixture's pidfd before escape.
            rustix::process::setsid()?;
            wire::write(&directory.join("escaped.json"), &Identity::capture(i32::try_from(std::process::id())?)?)?;
            escaped = true;
        }
        if role == "root" && directory.join("exit-root").exists() {
            return Ok(());
        }
        if let Some(term) = &mut term {
            let received = runtime.block_on(async { tokio::time::timeout(Duration::from_millis(10), term.recv()).await });
            if received.is_ok() && !directory.join(format!("{role}-term")).exists() {
                std::fs::write(directory.join(format!("{role}-term")), b"TERM handled; fixture remains alive")?;
            }
        } else {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
