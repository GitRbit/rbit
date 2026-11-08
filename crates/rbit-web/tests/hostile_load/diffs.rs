//! Many-file commits with huge lines requested concurrently: every diff stops at its file,
//! byte and line limits, so a response stays small however the commit is shaped, and an
//! ordinary read is served meanwhile.

use std::fmt::Write as _;

use axum::http::StatusCode;

use super::fixture::{Fixture, ORDINARY_PATH, assert_served_alongside, commit, get, git, under_load};

/// More files than one diff shows (300).
const FILES: usize = 360;
const REQUESTS: usize = 8;
/// Fifty files of the first 300 hold one 300 KiB line each: 15 MiB of diff text.
const HUGE_LINE: usize = 300 * 1024;
/// What one page may weigh: the diff's 2 MiB of text plus markup for 300 files.
const MAX_PAGE_BYTES: usize = 6 * 1024 * 1024;

fn files(content: impl Fn(usize) -> Vec<u8>) -> Vec<(String, Vec<u8>)> {
    (0..FILES).map(|file| (format!("many/{file:03}.txt"), content(file))).collect()
}

/// The changed side of `file`: a few short lines, or one huge line (under, or for some
/// files over, the 512 KiB file limit).
fn changed(file: usize) -> Vec<u8> {
    match file % 60 {
        1 => format!("{}\n", "o".repeat(600 * 1024)).into_bytes(),
        _ if file.is_multiple_of(6) => format!("{}\n", "h".repeat(HUGE_LINE)).into_bytes(),
        _ => (0..20)
            .fold(String::new(), |mut text, line| {
                writeln!(text, "file {file} changed line {line}").unwrap();
                text
            })
            .into_bytes(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_many_file_diffs_with_huge_lines_stay_within_their_limits() {
    let mut head = String::new();
    let fixture = Fixture::new(
        |_| {},
        |root| {
            commit(root, &files(|file| format!("file {file}\n").into_bytes()), "Many files");
            commit(root, &files(changed), "Change every file");
            head = git(root, &["rev-parse", "HEAD"]);
        },
    )
    .await;
    let path = format!("/hostile/load/commit/{head}");
    let hostile = (0..REQUESTS).map(|_| get(&path)).collect();
    let (replies, ordinary) = under_load(&fixture.router(), hostile, get(ORDINARY_PATH)).await;
    assert_served_alongside(&replies, &ordinary);
    for reply in &replies {
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert!(reply.body.len() < MAX_PAGE_BYTES, "a commit page of {} bytes", reply.body.len());
        assert!(reply.body.contains("This change is very large, so some diffs are not shown."));
        assert!(reply.body.contains("Diff too large to display."));
        assert!(reply.body.contains("many/000.txt") && !reply.body.contains("many/359.txt"), "the listing stops at 300 files");
        assert!(reply.body.contains("file 299 changed line 19"), "short diffs after the huge ones are still shown");
    }
}
