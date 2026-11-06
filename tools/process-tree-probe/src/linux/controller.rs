//! Every case observes an unrelated owned sibling outside the tested process group.

use super::{
    anchor::AnchoredGroup,
    case,
    identity::{FixtureProcess, Identity},
    wire,
};
use anyhow::{Context, Result, ensure};
use rustix::process::Signal;
use std::{
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
};

pub(super) async fn verify(output: Option<PathBuf>) -> Result<()> {
    let directory = tempfile::Builder::new().prefix("process group proof-Δ-").tempdir()?;
    let sibling_dir = directory.path().join("sibling");
    std::fs::create_dir(&sibling_dir)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("fixture-unix")
        .arg("sibling")
        .arg(&sibling_dir)
        .arg("cancel")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .process_group(0);
    let sibling = AnchoredGroup::new(command.spawn()?)?;
    let identity: Identity = wire::read(&sibling_dir.join("sibling.json")).await?;
    let sibling_process = FixtureProcess::open(identity)?;
    let mut cases = Vec::new();
    for scenario in ["root-first", "cancel", "escalation", "lost-anchor", "escape"] {
        let case_dir = directory.path().join(scenario);
        std::fs::create_dir(&case_dir)?;
        let mut evidence = case::run(scenario, &case_dir).await.with_context(|| format!("Linux scenario {scenario}"))?;
        ensure!(!sibling_process.exited()?, "unrelated sibling was terminated");
        ensure!(sibling.identity()? == sibling_process.identity, "unrelated sibling identity changed");
        evidence["unrelated_sibling_survived"] = true.into();
        cases.push(evidence);
    }
    sibling.signal(Signal::KILL)?;
    wire::until(|| sibling_process.exited()).await?;
    sibling.reap()?;
    let report = serde_json::json!({
        "development_only":true, "platform":"linux", "rustix":"1.1.5",
        "kernel":std::fs::read_to_string("/proc/sys/kernel/osrelease")?.trim(),
        "scope":"known fixture process-group/anchor observations; not production containment",
        "cases":cases,
        "limits":["no general group emptiness or hostile descendant containment proof",
            "ECHILD is not membership evidence", "setsid escapes group and requires quarantine",
            "pidfd and pipe evidence applies only to the finite handshake-adopted fixtures",
            "all group signals precede anchor consumption", "no universal Unix portability claim"]
    });
    if let Some(path) = output {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
