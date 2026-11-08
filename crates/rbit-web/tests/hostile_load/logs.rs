//! A runner flooding one task's log with concurrent appends: storage stops at
//! `[actions] max_log_bytes`, oversized calls are refused before any work, and log reads
//! issued meanwhile stay within their line and byte budgets.

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use rbit_core::actions::TaskId;
use rbit_services::actions::logs::LogRange;

use super::fixture::ORDINARY_BOUND;
use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{logs, payload, poll, seed, spec, template},
};

/// A small cap, so the flood passes it quickly.
const MAX_LOG_BYTES: u64 = 64 * 1024;
const FLOODERS: usize = 8;
const ROUNDS: usize = 6;
const LINES_PER_CALL: usize = 500;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_concurrent_log_flood_is_cut_at_the_cap_while_reads_stay_bounded() {
    let fixture = Fixture::with(|config| {
        config.actions.long_poll_seconds = 1;
        config.actions.max_log_bytes = MAX_LOG_BYTES;
    })
    .await;
    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let task = spec(&poll(&fixture, &credential, Some("flood")).await.1).task_id.0;
    let fixture = std::sync::Arc::new(fixture);

    // More lines than one call may carry are refused before the task is even read.
    let oversized = "x\n".repeat(rbit_services::actions::logs::MAX_LINES_PER_CALL + 1);
    let (status, _) = logs(&fixture, &credential, task, 0, &oversized).await;
    assert!(status.is_client_error(), "{status}");

    // Every flooder appends from the offset it last learned; losers of a race get a gap
    // (409) or have their overlap dropped, as a retrying runner would.
    let line = "y".repeat(200);
    let flooders: Vec<_> = (0..FLOODERS)
        .map(|_| {
            let (fixture, credential, chunk) =
                (std::sync::Arc::clone(&fixture), credential.clone(), format!("{line}\n").repeat(LINES_PER_CALL));
            tokio::spawn(async move {
                let mut from = 0;
                for _ in 0..ROUNDS {
                    let (status, json) = logs(&fixture, &credential, task, from, &chunk).await;
                    assert!(matches!(status, StatusCode::OK | StatusCode::CONFLICT), "{status}: {json}");
                    from = json["next_line"].as_u64().unwrap();
                }
            })
        })
        .collect();
    let repo = fixture.repo().await;
    let actions = fixture.actions();
    let mut reads = 0;
    while !flooders.iter().all(tokio::task::JoinHandle::is_finished) {
        let started = Instant::now();
        let window = actions.task_log(&repo, TaskId(task), LogRange::Tail, 50, 16 * 1024).await.unwrap();
        assert!(started.elapsed() < ORDINARY_BOUND, "a log read took {:?} during the flood", started.elapsed());
        assert!(window.lines.len() <= 50 && window.lines.iter().map(|line| line.len() + 1).sum::<usize>() <= 16 * 1024);
        reads += 1;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    for flooder in flooders {
        flooder.await.unwrap();
    }
    assert!(reads > 0);

    let stored = fixture.database.actions_runs().task(TaskId(task)).await.unwrap().unwrap();
    assert!(stored.log_truncated, "the flood passed the cap");
    assert!(u64::try_from(stored.log_lines).unwrap() > MAX_LOG_BYTES / 201, "lines past the cap are still counted");
    // One marker line may follow the last kept line.
    let bound = MAX_LOG_BYTES + 128;
    assert!(u64::try_from(stored.log_bytes).unwrap() <= bound, "{} bytes recorded", stored.log_bytes);
    let file = stored_bytes(&fixture, task).await;
    assert!(file <= bound, "{file} bytes on disk");
    // After the cap, further appends are still answered and add nothing.
    let (status, json) = logs(&fixture, &credential, task, u64::try_from(stored.log_lines).unwrap(), &format!("{line}\n")).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(stored_bytes(&fixture, task).await, file);
}

/// The size of the task's log file on disk.
async fn stored_bytes(fixture: &Fixture, task: i64) -> u64 {
    let raw = fixture.actions().raw_log(&fixture.repo().await, TaskId(task)).await.unwrap().unwrap();
    assert!(!raw.gzip, "the task is still running");
    raw.file.metadata().await.unwrap().len()
}
