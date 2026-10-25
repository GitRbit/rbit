//! Actions stream S: what a workflow task's push may not do (§11.4) — move or delete an
//! existing tag, or bring commits that change workflow files — judged on the pushed pack
//! before git stores anything.

use std::{
    io::{Cursor, Write},
    process::{Command, Stdio},
};

use rbit_core::RefUpdate;

use crate::actions_fixture::{Fixture, git};

/// The pack of the commits in `work` reachable from `new` but not from `old`.
fn pack(fixture: &Fixture, new: &str, old: &str) -> Vec<u8> {
    let mut child = Command::new("git")
        .args(["pack-objects", "--stdout", "--revs", "-q"])
        .current_dir(&fixture.work)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(format!("{new}\n^{old}\n").as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    output.stdout
}

fn update(name: &str, before: Option<&str>, after: Option<&str>) -> RefUpdate {
    RefUpdate { name: name.to_owned(), before: before.map(str::to_owned), after: after.map(str::to_owned) }
}

#[tokio::test]
async fn task_pushes_may_not_change_workflows_or_move_tags() {
    let fixture = Fixture::new().await;
    let actions = fixture.actions();
    let base = fixture.commit(&[("README.md", "hello\n"), (".github/workflows/ci.yml", "on: push\n")], "Base");
    fixture.push_ref("refs/heads/main").await;
    let repo = fixture.repo(&fixture.alice).await;

    let harmless = fixture.commit(&[("src/lib.rs", "fn main() {}\n")], "Code");
    let spooled = actions.spool_task_push(Cursor::new(pack(&fixture, &harmless, &base)), 0).await.unwrap();
    let refusals =
        actions.task_push_refusals(&repo, &[update("refs/heads/main", Some(&base), Some(&harmless))], &spooled, true).await.unwrap();
    assert!(refusals.is_empty(), "{refusals:?}");
    drop(spooled);

    for (path, contents) in [(".github/workflows/ci.yml", "on: [push, pull_request]\n"), (".rbit/workflows/new.yml", "on: push\n")] {
        git(&fixture.work, &["reset", "-q", "--hard", &base]);
        let sneaky = fixture.commit(&[(path, contents)], "Workflow");
        let spooled = actions.spool_task_push(Cursor::new(pack(&fixture, &sneaky, &base)), 0).await.unwrap();
        for target in [update("refs/heads/main", Some(&base), Some(&sneaky)), update("refs/heads/new", None, Some(&sneaky))] {
            let refusals = actions.task_push_refusals(&repo, std::slice::from_ref(&target), &spooled, true).await.unwrap();
            assert_eq!(refusals.len(), 1, "{path} via {}", target.name);
            assert!(refusals[0].reason.contains("workflow files"), "{}", refusals[0].reason);
        }
    }
    git(&fixture.work, &["reset", "-q", "--hard", &base]);
    let removed = fixture.commit(&[(".github/workflows/ci.yml", "")], "Remove CI");
    let spooled = actions.spool_task_push(Cursor::new(pack(&fixture, &removed, &base)), 0).await.unwrap();
    let refusals =
        actions.task_push_refusals(&repo, &[update("refs/heads/main", Some(&base), Some(&removed))], &spooled, true).await.unwrap();
    assert_eq!(refusals.len(), 1, "deleting a workflow is changing it");
    drop(spooled);

    // Tags: a new one is fine, moving or deleting an existing one is not.
    let tags = [
        update("refs/tags/v2", None, Some(&base)),
        update("refs/tags/v1", Some(&base), Some(&removed)),
        update("refs/tags/v0", Some(&base), None),
    ];
    let spooled = actions.spool_task_push(Cursor::new(Vec::new()), 0).await.unwrap();
    let refusals = actions.task_push_refusals(&repo, &tags, &spooled, false).await.unwrap();
    let names: Vec<_> = refusals.iter().map(|refusal| refusal.name.as_str()).collect();
    assert_eq!(names, ["refs/tags/v1", "refs/tags/v0"]);
    drop(spooled);

    // A pack that does not index refuses everything rather than letting it through.
    let spooled = actions.spool_task_push(Cursor::new(b"PACK garbage".to_vec()), 0).await.unwrap();
    let refusals =
        actions.task_push_refusals(&repo, &[update("refs/heads/main", Some(&base), Some(&removed))], &spooled, true).await.unwrap();
    assert_eq!(refusals.len(), 1);
    let spooled_dir = fixture.dir.path().join("data").join("tmp").join("actions-pushes");
    drop(spooled);
    assert_eq!(std::fs::read_dir(spooled_dir).unwrap().count(), 0, "spools are removed with their push");
}
