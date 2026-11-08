//! Blame on long histories requested concurrently: each distinct calculation is either
//! completed or refused at once (429) instead of queueing behind the Git walk, refusals
//! leave no reservation behind, and an ordinary read is served meanwhile. How many run
//! at once depends on timing here; `crates/rbit-services/src/blame/tests.rs` holds the
//! four-job admission deterministically.

use axum::http::StatusCode;

use super::fixture::{Fixture, ORDINARY_PATH, assert_refused, assert_served_alongside, fast_history, get, under_load};

/// Distinct files blamed at once: twice the service's four jobs.
const FILES: usize = 8;
/// Commits in the shared history; every one touches every file.
const COMMITS: usize = 400;
const LINES: usize = 400;

fn history(commit: usize) -> Vec<(String, Vec<u8>)> {
    (0..FILES)
        .map(|file| {
            // Each commit rewrites a different line of each file, so blame walks the whole
            // history to attribute every line.
            let text: String = (0..LINES)
                .map(|line| {
                    if line == commit % LINES {
                        format!("file {file} line {line} changed in {commit}\n")
                    } else {
                        format!("file {file} line {line}\n")
                    }
                })
                .collect();
            (format!("history/{file}.txt"), text.into_bytes())
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_blames_on_long_histories_are_admitted_or_refused() {
    let fixture = Fixture::new(|_| {}, |root| fast_history(root, COMMITS, history)).await;
    let hostile = (0..FILES).map(|file| get(&format!("/hostile/load/blame/main/history/{file}.txt"))).collect();
    let (replies, ordinary) = under_load(&fixture.router(), hostile, get(ORDINARY_PATH)).await;
    assert_served_alongside(&replies, &ordinary);
    let mut blamed = 0;
    for reply in &replies {
        match reply.status {
            StatusCode::OK => {
                assert!(reply.body.contains("changed in"), "a completed blame shows its lines");
                blamed += 1;
            }
            StatusCode::TOO_MANY_REQUESTS => assert_refused(reply, StatusCode::TOO_MANY_REQUESTS),
            status => panic!("unexpected {status}: {}", reply.body),
        }
    }
    assert!(blamed > 0, "some distinct blames completed");
    // Refused requests left no reservation behind: every file can be blamed now.
    for file in 0..FILES {
        let reply = fixture.get(&format!("/hostile/load/blame/main/history/{file}.txt")).await;
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    }
}
