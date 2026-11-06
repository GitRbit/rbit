//! Scenario Jobs exclude the sibling; one controller safety root covers the entire probe.

use crate::wire::{self, Completion, Containment, Evidence, Identity, Ready};
use anyhow::{Context, Result, ensure};
use processkit::ProcessGroup;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::process::{Child, Command};

pub(crate) async fn verify(output: Option<PathBuf>) -> Result<()> {
    let directory = tempfile::Builder::new().prefix("process tree proof-Δ-").tempdir()?;
    let sibling_dir = directory.path().join("sibling");
    std::fs::create_dir(&sibling_dir)?;
    let mut sibling = Command::new(std::env::current_exe()?)
        .arg("fixture")
        .arg("sibling")
        .arg(&sibling_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let identity: Identity = wire::read(&sibling_dir.join("sibling.json")).await?;
    let outcome = cases(directory.path(), &mut sibling, &identity).await;
    // Only this owned direct Child is killed; no PID-based cleanup targets strangers.
    let _ = sibling.kill().await;
    let _ = sibling.wait().await;
    let evidence = outcome?;
    let report = serde_json::json!({
        "development_only":true,"platform":"windows","win32job":"2.0.3","processkit":"3.3.4",
        "scope":"safe root Job plus per-operation Job; not production adoption", "cases":evidence,
        "sibling_scope":"outside scenario Jobs, inside controller safety root",
        "limits":["wait/query failure is unproved, not stopped","fault wait/query/resume are simulated contract faults",
            "noninherited root handle is behaviorally proved by abrupt-parent kill, not GetHandleInformation"]
    });
    if let Some(path) = output {
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn cases(directory: &Path, sibling: &mut Child, identity: &Identity) -> Result<Vec<Evidence>> {
    let mut evidence = Vec::new();
    for scenario in ["gap", "live", "root-exits", "cancel", "fault-assign", "fault-wait", "fault-query", "fault-resume"] {
        let case_dir = directory.join(scenario);
        std::fs::create_dir(&case_dir)?;
        evidence.push(case(scenario, &case_dir, sibling, identity).await.with_context(|| format!("scenario {scenario}"))?);
    }
    Ok(evidence)
}

async fn case(scenario: &str, directory: &Path, sibling: &mut Child, sibling_identity: &Identity) -> Result<Evidence> {
    let outer = ProcessGroup::new()?;
    let error_file = std::fs::File::create(directory.join("parent.stderr"))?;
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("parent").arg(scenario).arg(directory).stdin(Stdio::null()).stdout(Stdio::null()).stderr(error_file);
    let mut parent = outer.spawn(command)?;
    let result = observe(scenario, directory, &outer, &mut parent, sibling, sibling_identity).await;
    // Cleanup owns an OS Job handle, never a potentially reused PID. Even a failed
    // root-containment hypothesis cannot strand the suspended fixture process.
    let kill = if outer.members()?.is_empty() { Ok(()) } else { outer.kill_all() };
    let drained = wire::until(|| Ok(outer.members()?.is_empty())).await;
    let reaped = parent.wait().await;
    if let Err(error) = result {
        let details = std::fs::read_to_string(directory.join("parent.stderr")).unwrap_or_default();
        return Err(error.context(details));
    }
    kill?;
    drained?;
    reaped?;
    result
}

async fn observe(
    scenario: &str,
    directory: &Path,
    outer: &ProcessGroup,
    parent: &mut Child,
    sibling: &mut Child,
    sibling_identity: &Identity,
) -> Result<Evidence> {
    let ready: Ready = wire::read(&directory.join("ready.json")).await?;
    let members = outer.members()?;
    ensure!(members.contains(&ready.parent.pid), "parent did not join outer host Job");
    ensure!(!members.contains(&sibling_identity.pid), "unrelated sibling entered proof Job");
    for child in &ready.descendants {
        ensure!(ready.inherited_members.contains(&(child.pid as usize)), "root Job missed a descendant");
    }
    let mut evidence = if scenario == "gap" || scenario == "live" {
        ensure!(ready.before_nested_assignment == (scenario == "gap"), "gap fixture was not actually before nested assignment");
        parent.kill().await?;
        let _ = parent.wait().await?;
        wire::until(|| {
            for child in &ready.descendants {
                if child.alive()? {
                    return Ok(false);
                }
            }
            Ok(true)
        })
        .await?;
        // The outer Job remains open and received no kill. In the gap case there
        // is no per-operation Job either, isolating root last-handle-close behavior.
        wire::until(|| Ok(outer.members()?.is_empty())).await?;
        Evidence {
            scenario: scenario.into(),
            containment: Containment {
                nested_host_compatible: true,
                before_nested_inherited: ready.before_nested_assignment,
                root_handle_noninheritance_behavior: scenario == "gap",
            },
            root_wait_alone_insufficient: false,
            completion: Completion { members_empty: true, child_reaped: true, pumps_destroyed: false },
            admission_closed: false,
            unrelated_survived: false,
            disposition: "abrupt-parent-os-cleanup-observed".into(),
        }
    } else {
        std::fs::write(directory.join("stop"), b"stop")?;
        let report: Evidence = wire::read(&directory.join("outcome.json")).await?;
        ensure!(parent.wait().await?.success(), "parent proof failed");
        wire::until(|| Ok(outer.members()?.is_empty())).await?;
        if scenario.starts_with("fault-") {
            ensure!(report.admission_closed && report.disposition.starts_with("quarantined"), "fault was mislabeled as verified");
        } else {
            ensure!(
                report.disposition == "verified"
                    && report.completion.members_empty
                    && report.completion.child_reaped
                    && report.completion.pumps_destroyed,
                "normal cleanup evidence incomplete"
            );
        }
        report
    };
    ensure!(sibling.try_wait()?.is_none() && sibling_identity.alive()?, "unrelated sibling did not survive");
    evidence.unrelated_survived = true;
    Ok(evidence)
}
