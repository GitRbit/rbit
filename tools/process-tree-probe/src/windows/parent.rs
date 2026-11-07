//! Parent roles exercise inherited containment and independent per-operation cleanup.

use crate::{
    contract::{Admission, Fault, VerifiedCompletion},
    wire::{self, Completion, Containment, Evidence, Identity, Ready},
};
use anyhow::{Context, Result, ensure};
use processkit::ProcessGroup;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Semaphore;

pub(crate) async fn run(args: &[OsString]) -> Result<()> {
    let scenario = args.first().and_then(|arg| arg.to_str()).context("scenario missing")?;
    let directory = PathBuf::from(args.get(1).context("scenario directory missing")?);
    let gate = Arc::new(Semaphore::new(1));
    let quarantined = Arc::new(AtomicBool::new(false));
    let lease = Admission::new(Arc::clone(&gate), Arc::clone(&quarantined)).await;
    if scenario == "gap" || scenario == "fault-assign" {
        return suspended_gap(scenario, &directory, lease, &gate).await;
    }

    let group = ProcessGroup::new()?;
    let mut command = tokio::process::Command::new(std::env::current_exe()?);
    command.arg("fixture").arg("root").arg(&directory).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = group.spawn(command)?;
    let stdout = child.stdout.take().context("fixture stdout missing")?;
    let pump = tokio::spawn(async move { tokio::io::copy(&mut tokio::io::BufReader::new(stdout), &mut tokio::io::sink()).await });
    let descendants = descendants(&directory).await?;
    let members = group.members()?;
    for descendant in &descendants {
        ensure!(members.contains(&descendant.pid), "per-operation Job missed a descendant");
    }
    wire::write(
        &directory.join("ready.json"),
        &Ready {
            parent: Identity::capture(std::process::id())?,
            descendants,
            inherited_members: super::root::members()?,
            before_nested_assignment: false,
        },
    )?;

    let mut root_wait_alone_insufficient = false;
    let mut root_reaped = false;
    if scenario == "root-exits" {
        std::fs::write(directory.join("exit-root"), b"exit")?;
        child.wait().await?;
        root_reaped = true;
        ensure!(!group.members()?.is_empty(), "fixture helpers must outlive root");
        ensure!(gate.available_permits() == 0, "root wait released admission without empty/pump proof");
        root_wait_alone_insufficient = true;
    }
    wire::until(|| Ok(directory.join("stop").exists())).await?;
    group.kill_all()?;
    wire::until(|| Ok(group.members()?.is_empty())).await?;
    if !root_reaped {
        child.wait().await?;
    }
    let _copied = pump.await??;
    let proof =
        VerifiedCompletion::observe(child.id().is_none(), group.members()?.is_empty(), true).context("native evidence incomplete")?;
    let injected = match scenario {
        "fault-wait" => Some(Fault::Wait),
        "fault-query" => Some(Fault::Query),
        "fault-resume" => Some(Fault::Resume),
        _ => None,
    };
    if let Some(fault) = injected {
        lease.quarantine(fault);
    } else {
        lease.complete(proof);
    }
    ensure!(quarantined.load(Ordering::SeqCst) == gate.is_closed(), "quarantine did not close admission");
    wire::write(
        &directory.join("outcome.json"),
        &Evidence {
            scenario: scenario.into(),
            containment: Containment {
                nested_host_compatible: true,
                before_nested_inherited: false,
                root_handle_noninheritance_behavior: false,
            },
            root_wait_alone_insufficient,
            completion: Completion { members_empty: true, child_reaped: true, pumps_destroyed: true },
            admission_closed: gate.is_closed(),
            unrelated_survived: false,
            disposition: if injected.is_some() { "quarantined-simulated-fault" } else { "verified" }.into(),
        },
    )?;
    Ok(())
}

async fn suspended_gap(scenario: &str, directory: &Path, lease: Admission, gate: &Semaphore) -> Result<()> {
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    const CREATE_SUSPENDED: u32 = 0x0000_0004;
    let mut child = std::process::Command::new(std::env::current_exe()?)
        .arg("fixture")
        .arg("root")
        .arg(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_SUSPENDED)
        .spawn()?;
    let identity = Identity::capture(child.id())?;
    let inherited_members = super::root::members()?;
    ensure!(inherited_members.contains(&(child.id() as usize)), "child not inherited before nested assignment");
    // Preserve a real owned process handle throughout this rendezvous.
    ensure!(!child.as_raw_handle().is_null(), "child handle missing");
    wire::write(
        &directory.join("ready.json"),
        &Ready {
            parent: Identity::capture(std::process::id())?,
            descendants: vec![identity],
            inherited_members,
            before_nested_assignment: true,
        },
    )?;
    if scenario == "fault-assign" {
        wire::until(|| Ok(directory.join("stop").exists())).await?;
        let rejected = win32job::Job::create()?.assign_process(0);
        ensure!(rejected.is_err(), "invalid native assignment unexpectedly succeeded");
        lease.quarantine(Fault::Assign);
        ensure!(gate.is_closed(), "ambiguous assignment did not close admission");
        wire::write(
            &directory.join("outcome.json"),
            &Evidence {
                scenario: scenario.into(),
                containment: Containment {
                    nested_host_compatible: true,
                    before_nested_inherited: true,
                    root_handle_noninheritance_behavior: false,
                },
                root_wait_alone_insufficient: false,
                completion: Completion { members_empty: false, child_reaped: false, pumps_destroyed: false },
                admission_closed: true,
                unrelated_survived: false,
                disposition: "quarantined-native-assignment-fault".into(),
            },
        )?;
        // Exiting closes the root Job and kills the still-suspended child. No drain claim.
        return Ok(());
    }
    loop {
        ensure!(child.try_wait()?.is_none(), "suspended fixture exited before parent was terminated");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn descendants(directory: &Path) -> Result<Vec<Identity>> {
    let mut identities = Vec::new();
    for role in ["root", "helper", "grandchild"] {
        identities.push(wire::read(&directory.join(format!("{role}.json"))).await?);
    }
    Ok(identities)
}
