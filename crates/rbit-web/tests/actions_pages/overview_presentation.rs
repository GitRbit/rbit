//! Actual overview/router contracts: native controls stay outside the refreshed list.
use super::*;
use axum::Extension;
use rbit_localization::{Locale, LocaleResolver};
use rbit_storage::{Work, count_work};

fn results(html: &str) -> &str {
    let start = html.find("<div id=\"runs\"").unwrap();
    let mut remaining = &html[start..];
    let mut consumed = 0;
    let mut depth = 0;
    loop {
        let open = remaining.find("<div");
        let close = remaining.find("</div>").unwrap();
        if open.is_some_and(|position| position < close) {
            let end = open.unwrap() + 4;
            depth += 1;
            consumed += end;
            remaining = &remaining[end..];
        } else {
            depth -= 1;
            consumed += close + 6;
            if depth == 0 {
                return &html[start..start + consumed];
            }
            remaining = &remaining[close + 6..];
        }
    }
}

async fn enabled() -> Fixture {
    let fixture = Fixture::new().await;
    fixture.database.actions_settings().set_enabled(fixture.repo_id().await, true, Timestamp::now()).await.unwrap();
    fixture
}

#[tokio::test]
async fn native_filters_pagination_and_refresh_keep_the_same_machine_query_contract() {
    let fixture = Box::pin(enabled()).await;
    for index in 0..26 {
        fixture.run(".rbit/workflows/ci.yml", &format!("Change {index}"), RunEvent::Push).await;
    }
    let path = "/alice/rocket/actions?branch=main&actor=alice&event=push";
    let body = fixture.page(path, &fixture.reader).await;
    let region = results(&body);
    assert!(region.contains("25 runs on this page"));
    assert_eq!(region.matches("href=\"/alice/rocket/actions/runs/").count(), 25);
    assert!(region.contains("hx-trigger=\"every 3s\""));
    assert!(!region.contains("<form") && !region.contains("id=\"runs-branch\""));
    assert!(body.contains("method=\"get\" action=\"/alice/rocket/actions\""));
    for needle in ["name=\"branch\"", "name=\"actor\"", "name=\"event\"", "name=\"status\"", "value=\"main\"", "value=\"alice\""] {
        assert!(body.contains(needle), "missing native field {needle}");
    }
    let before_next = region.split("rel=\"next\"").next().unwrap();
    let next = before_next.rsplit("href=\"").next().unwrap().split('"').next().unwrap().replace("&#38;", "&").replace("&amp;", "&");
    assert!(next.starts_with("/alice/rocket/actions?branch=main&actor=alice&event=push&before="));
    let older = fixture.page(&next, &fixture.reader).await;
    assert!(results(&older).contains("1 run on this page"));
    assert_eq!(results(&older).matches("href=\"/alice/rocket/actions/runs/").count(), 1);
    assert!(results(&older).contains("hx-trigger=\"every 3s\""), "unfinished older runs retain existing polling");
    let empty = fixture.page("/alice/rocket/actions?branch=missing&actor=alice&event=push&status=running", &fixture.reader).await;
    assert!(results(&empty).contains("No matching runs"));
    assert!(results(&empty).contains("0 runs on this page"));
}

#[tokio::test]
async fn workflow_dispatch_toggle_and_private_visibility_remain_server_authorized() {
    let fixture = Box::pin(enabled()).await;
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    push(&repo, &[(".rbit/workflows/release.yml", RELEASE)]);
    let path = "/alice/rocket/actions/workflows/release.yml";
    let writer = fixture.page(path, &fixture.owner).await;
    assert!(writer.contains(&format!("method=\"post\" action=\"{path}/switch\"")));
    assert!(writer.contains(&format!("method=\"post\" action=\"{path}/dispatch\"")));
    assert!(writer.contains(&format!("method=\"get\" action=\"{path}\"")));
    for field in ["name=\"ref\"", "name=\"inputs[version]\"", "name=\"inputs[dry-run]\"", "name=\"disabled\""] {
        assert!(writer.contains(field));
    }
    assert!(!results(&writer).contains("<form"));
    assert!(results(&writer).contains("No runs yet"));
    let reader = fixture.page(path, &fixture.reader).await;
    assert!(!reader.contains(&format!("action=\"{path}/switch\"")));
    assert!(!reader.contains(&format!("action=\"{path}/dispatch\"")));
    let (status, ..) = fixture.send(Method::POST, &format!("{path}/dispatch"), &fixture.reader, "ref=main&inputs[version]=1.0").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, ..) = fixture.send(Method::POST, &format!("{path}/switch"), &fixture.owner, "disabled=true").await;
    assert!(status.is_redirection());
    let disabled = fixture.page(path, &fixture.owner).await;
    assert!(disabled.contains("Enable workflow") && disabled.contains("This workflow is disabled"));
    assert!(!disabled.contains(&format!("action=\"{path}/dispatch\"")));

    let hidden = fixture
        .services
        .repositories
        .create(
            &fixture.alice,
            CreateRepository { name: "hidden".into(), description: String::new(), visibility: rbit_core::Visibility::Private },
        )
        .await
        .unwrap();
    fixture.database.actions_settings().set_enabled(hidden.repo.id, true, Timestamp::now()).await.unwrap();
    for cookie in [fixture.reader.as_str(), ""] {
        let (status, ..) = fixture.send(Method::GET, "/alice/hidden/actions", cookie, "").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

async fn localized(router: &Router, path: &str, cookie: &str, language: &str, htmx: bool) -> (String, axum::http::HeaderMap, Work) {
    let mut request = Request::builder()
        .uri(path)
        .header(header::HOST, "localhost:3000")
        .header(header::COOKIE, cookie)
        .header(header::ACCEPT_LANGUAGE, language);
    if htmx {
        request = request.header("HX-Request", "true").header("HX-Target", "runs");
    }
    let ((body, headers), work) = count_work(async {
        let response = router.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let headers = response.headers().clone();
        let body = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        (body, headers)
    })
    .await;
    (body, headers, work)
}

#[tokio::test]
async fn locale_and_htmx_selection_preserve_privacy_filter_values_and_storage_work() {
    let fixture = Box::pin(enabled()).await;
    fixture.run(".rbit/workflows/ci.yml", "Change <unsafe>", RunEvent::Push).await;
    let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
    let path = "/alice/rocket/actions?branch=main&event=push";
    localized(&router, path, &fixture.reader, "en", false).await;
    let (_, _, baseline) = localized(&router, path, &fixture.reader, "en", false).await;
    for language in ["en", "ar-XB", "en-XA"] {
        for htmx in [false, true] {
            let (body, headers, work) = localized(&router, path, &fixture.reader, language, htmx).await;
            assert_eq!(headers[header::CONTENT_LANGUAGE], language);
            assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
            assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
            let vary = headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(",");
            assert!(vary.contains("Accept-Language") && vary.contains("Cookie"));
            assert_eq!(work.statements, baseline.statements);
            assert_eq!(work.acquisitions, baseline.acquisitions);
            let region = results(&body);
            let root = region.split('>').next().unwrap();
            assert!(root.contains(&format!("lang=\"{language}\"")));
            let direction = if language == "ar-XB" { "rtl" } else { "ltr" };
            assert!(root.contains(&format!("dir=\"{direction}\"")));
            assert!(region.contains("Change &#60;unsafe&#62;") && !region.contains("Change <unsafe>"));
            assert!(!region.contains("<form"));
            assert!(body.contains("name=\"branch\"") && body.contains("value=\"main\""));
        }
    }
}

#[tokio::test]
async fn translated_options_and_native_invalid_dispatch_keep_forms_locale_and_sql_work() {
    use rbit_localization::{Catalog, OverviewControlMessage as Control};
    let fixture = Box::pin(enabled()).await;
    let repo = fixture.services.repositories.open(Some(&fixture.alice), "alice", "rocket").await.unwrap();
    push(&repo, &[(".rbit/workflows/release.yml", RELEASE)]);
    let router = fixture.router.clone().layer(Extension(LocaleResolver::new(Locale::English, true).unwrap()));
    let path = "/alice/rocket/actions/workflows/release.yml";
    localized(&router, path, &fixture.owner, "en", false).await;
    let (_, _, baseline) = localized(&router, path, &fixture.owner, "en", false).await;
    let mut invalid_work = None;
    for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
        let language = locale.tag();
        let (body, _, work) = localized(&router, path, &fixture.owner, language, false).await;
        assert_eq!(work, baseline);
        let catalog = Catalog::new(locale);
        assert!(body.contains(&format!("value=\"workflow_dispatch\" >{}</option>", catalog.overview_control(Control::Manual))));
        assert!(body.contains(&format!("value=\"running\" >{}</option>", catalog.overview_control(Control::Running))));
        assert!(body.contains(&format!("for=\"dispatch-ref\">{}</label>", catalog.overview_control(Control::WorkflowFrom))));
        assert!(!body.contains("<div lang=\"en\" dir=\"ltr\"><details"));
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("{path}/dispatch"))
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::COOKIE, &fixture.owner)
            .header(header::ACCEPT_LANGUAGE, language)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("ref=main&inputs%5Bversion%5D=&inputs%5Bchannel%5D=beta&inputs%5Bdry-run%5D=false"))
            .unwrap();
        let (response, work) = count_work(router.clone().oneshot(request)).await;
        if let Some(expected) = invalid_work {
            assert_eq!(work, expected);
        } else {
            invalid_work = Some(work);
        }
        let response = response.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(response.headers()[header::CONTENT_LANGUAGE], language);
        assert!(!response.headers().contains_key(header::LOCATION));
        let html = String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
        assert!(html.contains(&format!("for=\"dispatch-ref\">{}</label>", catalog.overview_control(Control::WorkflowFrom))));
        assert!(html.contains("name=\"inputs[version]\""));
        assert!(html.contains("value=\"beta\" selected"));
        assert!(html.contains("role=\"alert\""));
    }
    let spanish = localized(&router, path, &fixture.owner, "es", false).await.0;
    assert!(spanish.contains("Usar el flujo de") && spanish.contains("En curso") && spanish.contains("Ejecutar flujo"));
}
