//! Deeply nested and expanding Markdown, requested concurrently as repository files, as a
//! README, as issue previews and as release notes: nesting is flattened before the
//! sanitiser sees it (whose cost was quadratic in depth: hours for 1 MiB of `>`), every
//! response arrives within a bound and stays proportional to its input, and an ordinary
//! read issued meanwhile is served.

use std::time::Duration;

use axum::http::StatusCode;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use rbit_services::releases::ReleaseInput;

use super::fixture::{Fixture, ORDINARY_PATH, assert_served_alongside, commit, form, get, under_load};

/// `[repository] max_render_bytes` for this fixture.
const MAX_RENDER_BYTES: usize = 128 * 1024;
/// Issue text is at most 64 KiB.
const PREVIEW_BYTES: usize = 60 * 1024;
/// Release notes are at most 125,000 characters.
const NOTES_BYTES: usize = 120_000;
/// Rendered pages may not grow beyond this multiple of their input.
const MAX_EXPANSION: usize = 16;
/// Before the nesting bounds, 128 KiB of `>` alone took about two minutes.
const RESPONSE_BOUND: Duration = Duration::from_secs(30);

/// Hostile documents of at most `bytes`, distinct per `seed` so none is a cache hit.
fn documents(bytes: usize, seed: usize) -> Vec<String> {
    let fill = |unit: &str| unit.repeat((bytes - 64) / unit.len());
    let reference = |unit: &str| {
        let url = "u".repeat(bytes / 4);
        format!("[r]: /{url}\n\n{}", unit.repeat((bytes - url.len() - 64) / unit.len()))
    };
    [fill(">"), fill("1. "), fill("<div>"), fill("<b id=1>x"), fill("<a><b><i><em><span>"), reference("[r] "), reference("![r] ")]
        .into_iter()
        .map(|body| format!("<!-- {seed} -->\n{body}"))
        .collect()
}

fn preview(body: &str) -> axum::http::Request<axum::body::Body> {
    let body = format!("body={}", utf8_percent_encode(body, NON_ALPHANUMERIC));
    form("/issues/markdown-preview?owner=hostile&repo=load", &body, None)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_hostile_markdown_stays_bounded_on_every_rendering_path() {
    let documents_in_files = documents(MAX_RENDER_BYTES, 0);
    let count = documents_in_files.len();
    let mut files: Vec<_> =
        documents_in_files.iter().enumerate().map(|(index, text)| (format!("docs/{index}.md"), text.clone().into_bytes())).collect();
    // The README of a directory renders on its tree page.
    files.push(("readme/README.md".to_owned(), documents(MAX_RENDER_BYTES, 2).swap_remove(0).into_bytes()));
    let fixture =
        Fixture::new(|config| config.repository.max_render_bytes = MAX_RENDER_BYTES, |root| commit(root, &files, "Hostile documents"))
            .await;
    let repo = fixture.services.repositories.open(Some(&fixture.user), "hostile", "load").await.unwrap();
    let notes = documents(NOTES_BYTES, 3).swap_remove(1);
    let release = ReleaseInput { tag_name: "v1".into(), name: "Nested".into(), notes, target: "main".into(), ..ReleaseInput::default() };
    fixture.services.releases.create(&repo, &fixture.user, release).await.unwrap();

    let mut hostile: Vec<_> = (0..count).map(|index| get(&format!("/hostile/load/blob/main/docs/{index}.md"))).collect();
    hostile.push(get("/hostile/load/tree/main/readme"));
    hostile.push(get("/hostile/load/releases"));
    let previews = documents(PREVIEW_BYTES, 1);
    hostile.extend(previews.iter().map(|body| preview(body)));
    let inputs: Vec<usize> =
        (0..count).map(|_| MAX_RENDER_BYTES).chain([MAX_RENDER_BYTES, NOTES_BYTES]).chain(previews.iter().map(|_| PREVIEW_BYTES)).collect();
    let (replies, ordinary) = under_load(&fixture.router(), hostile, get(ORDINARY_PATH)).await;
    assert_served_alongside(&replies, &ordinary);
    for (index, (reply, input)) in replies.iter().zip(inputs).enumerate() {
        assert_eq!(reply.status, StatusCode::OK, "request {index}: {}", reply.body);
        assert!(reply.elapsed < RESPONSE_BOUND, "request {index} took {:?}", reply.elapsed);
        assert!(reply.body.len() < MAX_EXPANSION * input, "request {index} rendered to {} bytes", reply.body.len());
        assert!(!reply.body.contains("<b id=\"1\">"), "raw attributes are still sanitised");
    }
}
