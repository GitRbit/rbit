//! Known-fixture ownership remains occupied through root wait and stdout destruction.

use super::{
    anchor::AnchoredGroup,
    completion::FixtureCompletion,
    identity::{FixtureProcess, Identity},
    wire,
};
use anyhow::{Context, Result, ensure};
use rustix::process::{Signal, WaitId, WaitIdOptions, waitid};
use std::{
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::Arc,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

struct Case {
    anchor: Option<AnchoredGroup>,
    fixtures: Vec<FixtureProcess>,
    pump: Option<JoinHandle<std::io::Result<u64>>>,
    gate: Arc<Semaphore>,
    permit: Option<OwnedSemaphorePermit>,
    completed: bool,
}
impl Drop for Case {
    fn drop(&mut self) {
        if !self.completed {
            self.gate.close();
            for fixture in &self.fixtures {
                let _ = fixture.kill();
            }
            if let Some(pump) = &self.pump {
                pump.abort();
            }
        }
        // The owned anchor then requests cleanup while still unreaped. No drop
        // path reports success or signals a number after anchor consumption.
    }
}

pub(super) async fn run(scenario: &str, directory: &Path) -> Result<serde_json::Value> {
    let gate = Arc::new(Semaphore::new(1));
    let permit = Arc::clone(&gate).acquire_owned().await?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("fixture-unix")
        .arg("root")
        .arg(directory)
        .arg(scenario)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .process_group(0);
    let mut owner = Case {
        anchor: Some(AnchoredGroup::new(command.spawn()?)?),
        fixtures: Vec::new(),
        pump: None,
        gate,
        permit: Some(permit),
        completed: false,
    };
    let stdout = owner.anchor.as_mut().context("anchor missing")?.take_stdout()?;
    let mut stdout = tokio::process::ChildStdout::from_std(stdout)?;
    owner.pump = Some(tokio::spawn(async move { tokio::io::copy(&mut stdout, &mut tokio::io::sink()).await }));
    for role in ["root", "helper", "grandchild"] {
        let identity: Identity = wire::read(&directory.join(format!("{role}.json"))).await?;
        owner.fixtures.push(FixtureProcess::open(identity)?);
    }
    let leader = owner.fixtures[0].identity.clone();
    ensure!(leader.pid == leader.pgid && leader.pid > 1, "root did not lead its own group");
    ensure!(owner.fixtures.iter().all(|fixture| fixture.identity.pgid == leader.pgid), "fixture did not inherit group");
    let report = observe(&mut owner, scenario, directory, &leader).await?;
    owner.completed = true;
    drop(owner.permit.take());
    Ok(report)
}

async fn observe(owner: &mut Case, scenario: &str, directory: &Path, leader: &Identity) -> Result<serde_json::Value> {
    let root_first = matches!(scenario, "root-first" | "lost-anchor");
    if root_first {
        std::fs::write(directory.join("exit-root"), b"exit")?;
        wire::until(|| owner.anchor.as_mut().context("anchor missing")?.exited_waitable()).await?;
        for _ in 0..3 {
            let anchor = owner.anchor.as_mut().context("anchor missing")?;
            ensure!(anchor.exited_waitable()? && anchor.identity()? == *leader, "WNOWAIT did not retain anchor identity");
            ensure!(!owner.fixtures[1].exited()? && !owner.fixtures[2].exited()?, "helpers must outlive root");
            ensure!(!owner.pump.as_ref().context("pump missing")?.is_finished(), "helpers must retain stdout");
            ensure!(owner.gate.available_permits() == 0, "admission released before helper/pump completion");
        }
    }
    if scenario == "escape" {
        std::fs::write(directory.join("escape-now"), b"escape")?;
        let escaped: Identity = wire::read(&directory.join("escaped.json")).await?;
        ensure!(
            escaped.pid == owner.fixtures[2].identity.pid && escaped.start_time == owner.fixtures[2].identity.start_time,
            "escape identity changed"
        );
        ensure!(escaped.pgid != leader.pgid, "setsid fixture did not escape");
    }
    let mut escalated = false;
    if scenario == "lost-anchor" {
        owner.anchor.take().context("anchor missing")?.reap()?;
        let status = waitid(WaitId::Pid(leader.pid()?), WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG);
        ensure!(matches!(status, Err(rustix::io::Errno::CHILD)), "expected actual ECHILD after deliberate anchor consumption");
        owner.gate.close();
        // No group signal is possible from here: only stable owned fixture pidfds.
        for fixture in &owner.fixtures {
            fixture.kill()?;
        }
    } else {
        owner.anchor.as_ref().context("anchor missing")?.signal(Signal::TERM)?;
        if scenario == "escalation" {
            for role in ["root", "helper", "grandchild"] {
                wire::until(|| Ok(directory.join(format!("{role}-term")).exists())).await?;
            }
            for fixture in &owner.fixtures {
                ensure!(!fixture.exited()?, "TERM fixture did not remain alive");
            }
            ensure!(owner.gate.available_permits() == 0, "TERM request released admission");
            owner.anchor.as_ref().context("anchor missing")?.signal(Signal::KILL)?;
            escalated = true;
        }
        if scenario == "escape" {
            wire::until(|| Ok(owner.fixtures[0].exited()? && owner.fixtures[1].exited()?)).await?;
            ensure!(
                !owner.fixtures[2].exited()? && !owner.pump.as_ref().context("pump missing")?.is_finished(),
                "escape must keep completion unproved"
            );
            ensure!(owner.gate.available_permits() == 0, "escaped fixture released admission");
            owner.gate.close();
            owner.fixtures[2].kill()?;
        }
    }
    wire::until(|| {
        for fixture in &owner.fixtures {
            if !fixture.exited()? {
                return Ok(false);
            }
        }
        Ok(true)
    })
    .await?;
    if let Some(anchor) = owner.anchor.take() {
        anchor.reap()?;
    }
    // Subreaper cleanup targets only known helpers. ECHILD is acceptable here
    // solely because independent pidfd readiness already established fixture exit.
    for fixture in &owner.fixtures[1..] {
        match waitid(WaitId::Pid(fixture.identity.pid()?), WaitIdOptions::EXITED | WaitIdOptions::NOHANG) {
            Ok(Some(_)) | Err(rustix::io::Errno::CHILD) => {}
            other => anyhow::bail!("targeted fixture reap unproved: {other:?}"),
        }
    }
    tokio::time::timeout(wire::WAIT, owner.pump.take().context("pump missing")?).await.context("stdout pump did not finish")???;
    let _receipt = FixtureCompletion::observe(true, true, true)?;
    Ok(serde_json::json!({
        "scenario":scenario, "root_reaped":true, "known_fixture_pidfds_exited":true, "pump_completed":true,
        "root_first_anchor_retained":root_first, "escalated":escalated, "admission_closed":owner.gate.is_closed(),
        "disposition":if owner.gate.is_closed() { "quarantined-even-after-fixture-cleanup" } else { "known-fixture-completion" },
        "general_group_empty_claimed":false, "hostile_descendant_containment_claimed":false
    }))
}
