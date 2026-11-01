//! Real HTTP contract shared by the pre-extraction and extracted summary views.
use super::*;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

struct Document {
    headers: HeaderMap,
    html: String,
    queries: Vec<String>,
}
async fn measured_summary(fixture: &Fixture, cookie: &str, path: &str) -> Document {
    let counter = crate::query_count::QueryCounter::new();
    let (status, headers, html) = counter.measure(request(fixture, cookie, Method::GET, path, String::new())).await;
    assert_eq!(status, StatusCode::OK, "{path}");
    let queries: Vec<_> = counter.queries().into_iter().map(|(sql, _)| sql).collect();
    assert!(queries.len() > 1, "query instrumentation must observe actual application work: {queries:?}");
    // The combined header lookup includes both open issue and pull-request counts.
    // It must happen once, even though native document authorization also opens
    // the repository. Generic shell re-composition must not load it a second time.
    let headers_read = queries.iter().filter(|sql| sql.contains("AS open_issues") && sql.contains("AS open_pulls")).count();
    assert_eq!(headers_read, 1, "one authorized repository-header expansion: {queries:#?}");
    Document { headers, html, queries }
}
fn assert_full_host_contract(document: &Document, title: &str, heading: &str) {
    let html = &document.html;
    assert_document_appearance(html, "base24-berlin", "dark");
    assert_eq!(html.matches("<!doctype html>").count(), 1);
    assert_eq!(html.matches("<title>").count(), 1);
    assert!(html.contains(&format!("<title>{}</title>", Escaped { value: title }.render().unwrap())));
    assert_eq!(html.matches("<h1").count(), 1, "generic fragment title must not replace or duplicate summary heading");
    assert!(html.contains(&Escaped { value: heading }.render().unwrap()));
    let body = html.split("<body ").nth(1).unwrap().split('>').next().unwrap();
    assert!(body.contains("hx-history=\"false\"") && body.contains("hx-boost=\"false\""));
    assert!(body.contains("data-signed-in"));
    assert!(html.contains("data-actions-summary"));
    let active: Vec<_> = html
        .split("<a ")
        .filter_map(|element| {
            let tag = element.split('>').next()?;
            if !tag.contains("aria-current=\"page\"") {
                return None;
            }
            tag.split("data-repo-tab=\"").nth(1)?.split('"').next()
        })
        .collect();
    assert_eq!(active, ["rbit.actions:actions-repository-tab"]);
    assert!(html.contains("href=\"/alice/rocket\""), "host repository navigation survives");
    assert_eq!(document.headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(document.headers[header::CONTENT_LANGUAGE], "en");
    let policy = document.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.contains("script-src 'self'") && policy.contains("frame-ancestors 'none'"));
    assert!(!policy.contains("'unsafe-inline'") && !policy.contains("'unsafe-eval'"));
    let styles: Vec<_> =
        html.split("<style ").skip(1).map(|style| style.split_once('>').unwrap().1.split_once("</style>").unwrap().0).collect();
    assert_eq!(styles.len(), 2, "the host includes its app and selected-theme sheets");
    for style in styles {
        assert!(policy.contains(&format!("'sha256-{}'", STANDARD.encode(Sha256::digest(style.as_bytes())))));
    }
}
fn amplified_source() -> String {
    let source = format!(
        "## Full summary\n\n{}|\n{}|\n{}\n<script>untrusted_summary()</script>\nCOMPLETE_SOURCE_TAIL",
        "|x".repeat(100),
        "|-".repeat(100),
        format!("{}\n", "|".repeat(101)).repeat(620)
    );
    assert!(source.len() <= 64 * 1024);
    assert!(rbit_web::render::render_markdown_bounded(&source, 512 * 1024).is_none());
    source
}
#[tokio::test]
async fn summary_representation_preserves_single_host_expansion_document_security_and_query_work() {
    use crate::actions_runner_tasks::{payload, poll, seed, spec, state, template};
    let fixture = Fixture::new().await;
    unread_notification(&fixture).await;
    let credential = fixture.runner(&["linux"]).await;
    let run = seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = fixture.database.actions_runs().jobs(run.id).await.unwrap().remove(0);
    let task = spec(&poll(&fixture, &credential, Some("summary-host-contract")).await.1).task_id.0;
    let source = amplified_source();
    let body = serde_json::json!({"steps":[],"outputs":{},"result":"success","summaries":{"0":"## HTML summary\n\n**SANITIZED_CONTENT_MARKER**<script>untrusted_summary()</script>","1":source}});
    assert_eq!(state(&fixture, &credential, task, &body).await.0, StatusCode::OK);
    let cookie = format!("{}; rbit-theme=base24-berlin", cookie(&fixture).await);
    let base = format!("/alice/rocket/-/plugins/rbit.actions/summary?run={}&job={}", run.number, job.id.0);
    // Resolve cold connection/cache/session setup before comparing request work.
    for step in [0, 1] {
        assert_eq!(request(&fixture, &cookie, Method::GET, &format!("{base}&step={step}"), String::new()).await.0, StatusCode::OK);
    }
    let rendered = measured_summary(&fixture, &cookie, &format!("{base}&step=0")).await;
    let fallback = measured_summary(&fixture, &cookie, &format!("{base}&step=1")).await;
    let title = format!("{} summary · {}", job.name, fixture.services.instance.current().site_name);
    for (step, document) in [(0, &rendered), (1, &fallback)] {
        assert_full_host_contract(document, &title, &format!("{} summary · {step}", job.name));
    }
    assert!(rendered.html.contains("<strong>SANITIZED_CONTENT_MARKER</strong>"));
    assert!(!rendered.html.contains("COMPLETE_SOURCE_TAIL"));
    assert!(rendered.html.contains(&format!("href=\"{}\"", Escaped { value: &format!("{base}&step=1") }.render().unwrap())));
    assert!(fallback.html.contains("COMPLETE_SOURCE_TAIL") && fallback.html.contains("showing its Markdown source"));
    assert!(fallback.html.contains("&#60;script&#62;untrusted_summary()&#60;/script&#62;"));
    assert!(fallback.html.contains(&format!("href=\"{}\"", Escaped { value: &format!("{base}&step=0") }.render().unwrap())));
    for document in [&rendered, &fallback] {
        assert!(!document.html.contains("<script>untrusted_summary()"));
    }
    assert_eq!(rendered.headers[header::CONTENT_SECURITY_POLICY], fallback.headers[header::CONTENT_SECURITY_POLICY]);
    // Preserve every actual SQL string and multiplicity; only ignore completion
    // order from legitimate parallel reads, not any representation-specific work.
    let mut html_sql = rendered.queries;
    let mut source_sql = fallback.queries;
    html_sql.sort();
    source_sql.sort();
    eprintln!(
        "summary host SQL multiset: html_count={} html_sha256={} source_count={} source_sha256={}",
        html_sql.len(),
        sql_multiset_digest(&html_sql),
        source_sql.len(),
        sql_multiset_digest(&source_sql)
    );
    assert_eq!(html_sql, source_sql);
    fixture.services.shutdown().await;
}

// Versioned domain + fixed-width big-endian count and per-statement UTF-8 byte
// lengths make the fingerprint unambiguous, retaining duplicate statements.
// The caller sorts only completion order; SQL bytes are never normalized.
fn sql_multiset_digest(statements: &[String]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"rbit-summary-host-sql-multiset-v1\0");
    digest.update(u64::try_from(statements.len()).unwrap().to_be_bytes());
    for statement in statements {
        digest.update(u64::try_from(statement.len()).unwrap().to_be_bytes());
        digest.update(statement.as_bytes());
    }
    let mut encoded = String::with_capacity(64);
    for byte in &digest.finalize() {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}
