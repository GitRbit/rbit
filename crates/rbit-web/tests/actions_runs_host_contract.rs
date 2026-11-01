//! Same real-router contract is run before and after the runs document extraction.
use super::native_actions_summary_host_contract::{Document, assert_host_document, measured_summary, sql_multiset_digest};
use super::*;
use crate::actions_runner_tasks::{payload, seed, template};
use sha2::{Digest as _, Sha256};

const RUNS: &str = "/alice/rocket/-/plugins/rbit.actions/runs";
const WORKFLOW: &str = "/alice/rocket/-/plugins/rbit.actions/workflow?workflow=ci.yml";
const SWITCH: &str = "/alice/rocket/-/plugins/rbit.actions/switch";

// Normalize host-issued CSRF proof VALUES, complete <time> elements and the
// request-duration text, update observation revision and random catalog base.
// Keep every other byte, including field names/actions, whitespace, URLs, CSS,
// scripts, identities, form values, polling attributes and escaping.
fn normalized_document(html: &str) -> String {
    const CSRF: &str = "name=\"_csrf\" value=\"";
    const DURATION: &str = "<span data-render-time>";
    const UPDATE: &str = "data-update-notice=\"";
    const LOCALE: &str = "data-locale-revision=\"en:";
    const PLUGINS: &str = "data-plugin-revision=\"";
    let mut normalized = String::new();
    let mut remaining = html;
    while let Some((prefix, tail)) = remaining.split_once(CSRF) {
        normalized.push_str(prefix);
        normalized.push_str(CSRF);
        normalized.push_str("HOST_CSRF_PROOF");
        let (_, rest) = tail.split_once('"').expect("closed CSRF input");
        normalized.push('"');
        remaining = rest;
    }
    normalized.push_str(remaining);
    let mut result = String::new();
    remaining = &normalized;
    while let Some((prefix, tail)) = remaining.split_once("<time ") {
        result.push_str(prefix);
        let (element, rest) = tail.split_once("</time>").expect("closed time element");
        assert!(element.contains("datetime=\""), "time semantics remain explicitly checked");
        result.push_str("<time HOST_OBSERVED_TIME></time>");
        remaining = rest;
    }
    result.push_str(remaining);
    let mut output = String::new();
    remaining = &result;
    while let Some((prefix, tail)) = remaining.split_once(DURATION) {
        output.push_str(prefix);
        output.push_str(DURATION);
        let (duration, rest) = tail.split_once("</span>").expect("closed render duration");
        let (number, unit) = duration.split_once(' ').expect("render duration has a unit");
        assert!(matches!(unit, "µs" | "ms" | "s"));
        assert!(number.parse::<f64>().is_ok_and(|value| value.is_finite() && value >= 0.0));
        output.push_str("HOST_RENDER_DURATION</span>");
        remaining = rest;
    }
    output.push_str(remaining);
    let mut final_document = String::new();
    remaining = &output;
    while let Some((prefix, tail)) = remaining.split_once(UPDATE) {
        final_document.push_str(prefix);
        final_document.push_str(UPDATE);
        let (revision, rest) = tail.split_once(';').expect("update observation revision");
        assert!(!revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_digit()));
        assert!(revision.parse::<u128>().is_ok());
        final_document.push_str("HOST_UPDATE_REVISION;");
        remaining = rest;
    }
    final_document.push_str(remaining);
    // The i18n catalog starts with a random revision. Preserve locale and the
    // plugin revision delta so a changed route publication remains observable.
    if let Some((_, tail)) = final_document.split_once(LOCALE) {
        let (locale, _) = tail.split_once('"').expect("closed locale revision");
        let locale_number: u64 = locale.parse().expect("numeric English catalog revision");
        let (_, tail) = final_document.split_once(PLUGINS).expect("host plugin revision");
        let (plugin, _) = tail.split_once('"').expect("closed plugin revision");
        let plugin_number: u64 = plugin.parse().expect("numeric plugin revision");
        let delta = plugin_number.checked_sub(locale_number).expect("plugin revision includes locale revision");
        let locale_attribute = format!("{LOCALE}{locale}\"");
        let plugin_attribute = format!("{PLUGINS}{plugin}\"");
        final_document = final_document.replacen(&locale_attribute, &format!("{LOCALE}HOST_CATALOG_REVISION\""), 1);
        final_document = final_document.replacen(&plugin_attribute, &format!("{PLUGINS}HOST_CATALOG_REVISION+{delta}\""), 1);
    }
    final_document
}
fn emit_cell(route: &str, document: &Document) {
    let mut statements = document.queries.clone();
    statements.sort();
    let normalized = normalized_document(&document.html);
    let mut normalized_hash = String::with_capacity(64);
    for byte in Sha256::digest(normalized.as_bytes()) {
        use std::fmt::Write as _;
        write!(normalized_hash, "{byte:02x}").unwrap();
    }
    // Captured on production 274f598; both cells matched two fresh fixtures.
    let (count, sql_hash, html_hash) = match route {
        "runs-owner-filtered" => (
            12,
            "0d27a7db939204fe603770663b7b05dff5d961294e7144351066731e01e3f0c6",
            "4961580bb863ba810875f0fcc1d4f688d89719ea3970fcf85489fea77d2f1384",
        ),
        "workflow-owner-disabled-filtered" => (
            16,
            "6664c7a70c27a515ba87711086975f1688f157cd036fbe9da79aafdd300fd7bd",
            "4d026bfc063581bd3b51fbb789001744f245196193a686ddd7b375a9f4ee4bfe",
        ),
        _ => panic!("unregistered runs contract cell"),
    };
    assert_eq!(statements.len(), count, "{route} query work");
    assert_eq!(sql_multiset_digest(&statements), sql_hash, "{route} exact SQL multiset");
    let html_hash = if cfg!(feature = "i18n") {
        html_hash
    } else {
        match route {
            "runs-owner-filtered" => "a3e9afa5447b5c5750b7f407084ef3b33b2c4992b08fc954dcbf2f39eab9cd83",
            "workflow-owner-disabled-filtered" => "f207a88c636b442ed9a3f052fb993784dc3d341fe417e360201303e1a1b9363b",
            _ => unreachable!(),
        }
    };
    assert_eq!(normalized_hash, html_hash, "{route} normalized host document");
    eprintln!(
        "runs host contract: route={route} sql_count={} sql_sha256={} normalized_html_sha256={normalized_hash}",
        statements.len(),
        sql_multiset_digest(&statements)
    );
}
async fn stable_cell(fixture: &Fixture, cookie: &str, path: &str) -> Document {
    let first = measured_summary(fixture, cookie, path).await;
    let second = measured_summary(fixture, cookie, path).await;
    assert_eq!(
        normalized_document(&first.html),
        normalized_document(&second.html),
        "declared normalization must produce stable same-state documents"
    );
    let mut first_sql = first.queries.clone();
    let mut second_sql = second.queries;
    first_sql.sort();
    second_sql.sort();
    assert_eq!(first_sql, second_sql, "same route/state retains exact SQL bytes and multiplicity");
    first
}
fn assert_runs_fields(document: &Document, action: &str) {
    let html = &document.html;
    assert!(html.contains(&format!("action=\"{}\"", Escaped { value: action }.render().unwrap())));
    for name in ["branch", "actor", "event", "status"] {
        assert!(html.contains(&format!("name=\"{name}\"")), "filter {name}");
    }
    assert!(html.contains("value=\"alice\""), "current actor filter survives");
    assert!(html.contains("value=\"main\""));
    assert!(html.contains("Build it") && html.contains("CI #1"));
    assert!(html.contains("id=\"runs\"") && html.contains("hx-select=\"#runs\""));
    assert!(html.contains("datetime=\"") && html.contains("</time>"));
    assert!(html.contains("href=\"/alice/rocket/-/plugins/rbit.actions/run?run=1\""));
}
async fn assert_private_denials(fixture: &Fixture, path: &str, outsider: &str) {
    for viewer in ["", outsider] {
        let (status, headers, html) = request(fixture, viewer, Method::GET, path, String::new()).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "private repository existence stays hidden");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        assert!(!html.contains("Build it") && !html.contains("CI #1"));
        assert!(!html.contains(&format!("action=\"{SWITCH}\"")));
    }
}
async fn unrelated_session(fixture: &Fixture) -> String {
    let user = fixture
        .services
        .accounts
        .create_user(
            rbit_services::SignUp {
                username: "outsider".into(),
                email: "outsider@example.com".into(),
                password: "correct horse battery".into(),
            },
            Some(false),
        )
        .await
        .unwrap();
    format!("rbit_session={}", fixture.services.accounts.start_session(&user).await.unwrap().encode())
}
#[tokio::test]
async fn runs_and_workflow_documents_preserve_host_work_forms_identity_and_private_access() {
    let fixture = Fixture::new().await;
    unread_notification(&fixture).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let repo = fixture.repo().await;
    fixture.actions().set_workflow_disabled(&fixture.alice, &repo, ".rbit/workflows/ci.yml", true).await.unwrap();
    let owner = format!("{}; rbit-theme=base24-berlin", cookie(&fixture).await);
    let outsider = unrelated_session(&fixture).await;
    let filters = "branch=main&actor=alice&event=push";
    let runs = format!("{RUNS}?{filters}");
    let workflow = format!("{WORKFLOW}&{filters}");
    for path in [&runs, &workflow] {
        assert_eq!(request(&fixture, &owner, Method::GET, path, String::new()).await.0, StatusCode::OK);
    }
    let all = stable_cell(&fixture, &owner, &runs).await;
    let selected = stable_cell(&fixture, &owner, &workflow).await;
    let site = fixture.services.instance.current().site_name;
    assert_host_document(&all, &format!("Actions · alice/rocket · {site}"), "All workflow runs");
    assert_host_document(&selected, &format!("CI · Actions · alice/rocket · {site}"), "CI");
    assert_runs_fields(&all, RUNS);
    assert_runs_fields(&selected, WORKFLOW);
    assert!(!all.html.contains(&format!("action=\"{SWITCH}\"")));
    assert!(selected.html.contains(&format!("action=\"{SWITCH}\"")));
    assert!(selected.html.contains("name=\"workflow\" value=\"ci.yml\""));
    assert!(selected.html.contains("name=\"disabled\" value=\"false\""));
    assert!(selected.html.contains("name=\"_csrf\" value=\""));
    assert!(selected.html.contains("Enable workflow"));
    assert!(!selected.html.contains("name=\"inputs["), "disabled workflow cannot offer a dispatch form");
    emit_cell("runs-owner-filtered", &all);
    emit_cell("workflow-owner-disabled-filtered", &selected);
    for path in [&runs, &workflow] {
        assert_private_denials(&fixture, path, &outsider).await;
    }
    fixture.services.shutdown().await;
}
#[test]
fn normalization_preserves_nonvolatile_values_and_security_markup() {
    let input = "<input name=\"_csrf\" value=\"proof\"><input name=\"actor\" value=\"alice\"><time datetime=\"2026-01-01\">now</time><script src=\"/app.js\"></script>";
    let input = format!(
        "{input}<span data-render-time>2.7 ms</span><span data-other-time>2.7 ms</span><span data-update-notice=\"123;pending\"></span>"
    );
    let output = normalized_document(&input);
    assert!(!output.contains("proof") && !output.contains("2026-01-01"));
    assert!(output.contains("data-update-notice=\"HOST_UPDATE_REVISION;pending\""));
    assert!(output.contains("name=\"_csrf\"") && output.contains("name=\"actor\" value=\"alice\""));
    assert!(output.contains("<span data-render-time>HOST_RENDER_DURATION</span><span data-other-time>2.7 ms</span>"));
    assert!(output.contains("<script src=\"/app.js\"></script>"));
}

#[test]
fn normalization_rejects_malformed_render_duration() {
    for duration in ["unexpected", "NaN ms", "-1 ms", "2 minutes", "<b>2</b> ms"] {
        let html = format!("<span data-render-time>{duration}</span>");
        assert!(std::panic::catch_unwind(|| normalized_document(&html)).is_err(), "{duration}");
    }
}

#[test]
fn normalization_preserves_locale_and_relative_plugin_publication_revision() {
    let document = |locale, plugin| format!("<body data-locale-revision=\"en:{locale}\" data-plugin-revision=\"{plugin}\"></body>");
    assert_eq!(normalized_document(&document(100, 111)), normalized_document(&document(200, 211)));
    assert_ne!(normalized_document(&document(100, 111)), normalized_document(&document(200, 212)));
    let other = "<body data-locale-revision=\"es:100\" data-plugin-revision=\"111\"></body>";
    assert_eq!(normalized_document(other), other);
}
