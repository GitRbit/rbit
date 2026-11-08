//! Pathological files highlighted concurrently: at most four highlighting jobs run, and
//! every other cold page is served at once with its lines unhighlighted instead of
//! queueing or being refused, including an ordinary read issued meanwhile.

use axum::http::StatusCode;

use super::fixture::{Fixture, ORDINARY_PATH, assert_served_alongside, commit, get, under_load};

/// Distinct files requested at once: twice the highlighter's four jobs.
const FILES: usize = 8;

/// A Rust file just under the 512 KiB highlighting limit whose lines stay just under the
/// 5,000-character line limit, every one deeply nested, and distinct per `file`.
fn pathological(file: usize) -> Vec<u8> {
    let line = format!("fn hostile_fn_{file}() {{ {} }}\n", "(".repeat(2_400) + &")".repeat(2_400));
    let mut text = String::new();
    while text.len() + line.len() < 500 * 1024 {
        text.push_str(&line);
    }
    text.into_bytes()
}

/// Highlighted Rust marks `fn` as a storage type; unhighlighted lines carry no spans.
fn highlighted(body: &str) -> bool {
    body.contains("<span class=\"t\">fn</span>")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_pathological_highlighting_is_admitted_or_served_plain() {
    let files: Vec<_> = (0..FILES).map(|file| (format!("hostile/{file}.rs"), pathological(file))).collect();
    let fixture = Fixture::new(|_| {}, |root| commit(root, &files, "Pathological highlighting")).await;
    let hostile = (0..FILES).map(|file| get(&format!("/hostile/load/blob/main/hostile/{file}.rs"))).collect();
    let (replies, ordinary) = under_load(&fixture.router(), hostile, get(ORDINARY_PATH)).await;
    assert_served_alongside(&replies, &ordinary);
    assert!(ordinary.body.contains("An ordinary file."));
    let mut plain = 0;
    for (file, reply) in replies.iter().enumerate() {
        assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
        assert!(reply.body.contains(&format!("hostile_fn_{file}")), "the file's own lines are shown");
        plain += usize::from(!highlighted(&reply.body));
    }
    assert!(plain > 0, "excess highlighting jobs are served plain, not queued");
    assert!(FILES - plain <= 4, "at most four highlighting jobs ran at once");
    // Plain answers are not cached: once the jobs are done, every file highlights.
    for file in 0..FILES {
        let reply = fixture.get(&format!("/hostile/load/blob/main/hostile/{file}.rs")).await;
        assert!(highlighted(&reply.body), "file {file} is highlighted when capacity is free");
    }
}
