use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Json, Router, extract::Request, http::StatusCode, response::IntoResponse, routing::get};
use reqwest::{Client, header};

use super::{
    github::{MAX_RESPONSE_BYTES, REQUEST_TIMEOUT, USER_AGENT},
    *,
};

fn notices(check_for_updates: bool) -> UpdateNotices {
    UpdateNotices::new(&UpdateConfig { check_for_updates, report_version: true })
}

async fn serve(app: Router) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    (address, tokio::spawn(async move { axum::serve(listener, app).await.unwrap() }))
}

#[tokio::test]
async fn worker_caches_checks_sends_no_running_version_and_stops_promptly() {
    rbit_services::webhooks::delivery::install_crypto_provider();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let app = Router::new().route(
        "/latest",
        get(move |request: Request| {
            observed.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(request.headers()[header::USER_AGENT], USER_AGENT);
                for key in [header::AUTHORIZATION, header::COOKIE, header::REFERER] {
                    assert!(request.headers().get(key).is_none());
                }
                assert!(request.uri().query().is_none());
                Json(serde_json::json!({ "tag_name":"v99.0.0", "draft":false, "prerelease":false, "html_url":"https://evil.example" }))
            }
        }),
    );
    let (address, server) = serve(app).await;
    let client = Client::builder().user_agent(USER_AGENT).no_proxy().timeout(REQUEST_TIMEOUT).build().unwrap();
    let notices = notices(true);
    let source = GithubReleases::with_client(client, format!("http://{address}/latest"));
    let checker = UpdateChecker { source, notices: notices.clone() };
    let initial_revision = notices.status().revision;
    let owner = TaskLifecycle::new();
    assert!(owner.start(checker.run(owner.stop_signal())));
    assert!(!owner.start(async {}), "one checker per owner");
    tokio::time::timeout(Duration::from_secs(5), async {
        while notices.status().state != UpdateState::Available {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(notices.status().revision > initial_revision);
    let observed_revision = notices.status().revision;
    assert_eq!(notices.status().revision, observed_revision, "reading a snapshot does not advance its revision");
    assert_eq!(notices.status().release.unwrap().url, "https://github.com/GitRbit/rbit/releases/tag/v99.0.0");
    tokio::time::sleep(Duration::from_millis(25)).await;
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    tokio::time::timeout(Duration::from_secs(1), shutdown(&owner)).await.unwrap();
    assert_eq!(owner.state(), rbit_kernel::TaskState::Stopped);
    server.abort();
}

#[tokio::test]
async fn disabled_checks_start_no_task() {
    let notices = notices(false);
    let owner = start(&UpdateConfig { check_for_updates: false, report_version: true }, notices.clone());
    assert_eq!(owner.state(), rbit_kernel::TaskState::Idle);
    assert_eq!(notices.status().state, UpdateState::Disabled);
    shutdown(&owner).await;
}

#[tokio::test]
async fn upstream_redirects_oversized_json_and_timeouts_do_not_produce_notices() {
    rbit_services::webhooks::delivery::install_crypto_provider();
    let app = Router::new()
        .route("/redirect", get(|| async { (StatusCode::FOUND, [(header::LOCATION, "https://evil.example")]) }))
        .route("/large", get(|| async { "x".repeat(MAX_RESPONSE_BYTES + 1) }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(2)).await;
                StatusCode::OK.into_response()
            }),
        );
    let (address, server) = serve(app).await;
    for path in ["redirect", "large", "slow"] {
        let client = Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(500))
            .build()
            .unwrap();
        let notices = notices(true);
        let checker =
            UpdateChecker { source: GithubReleases::with_client(client, format!("http://{address}/{path}")), notices: notices.clone() };
        assert!(checker.check().await.is_err());
        assert!(notices.status().release.is_none());
    }
    server.abort();
}

/// Scripted results, oldest first.
struct Scripted(Mutex<Vec<Result<Release, Option<Duration>>>>);

impl ReleaseSource for Scripted {
    fn latest(&self) -> impl Future<Output = Result<Release, Option<Duration>>> + Send {
        let next = self.0.lock().unwrap().remove(0);
        async move { next }
    }
}

fn tag(tag: &str) -> Release {
    Release { tag_name: tag.into(), draft: false, prerelease: false }
}

#[tokio::test]
async fn outages_keep_the_last_notice_and_back_off_until_a_success_resets_them() {
    let script = vec![Ok(tag("v99.0.0")), Err(None), Err(Some(Duration::from_hours(2))), Ok(tag("v0.0.1"))];
    let notices = notices(true);
    let checker = UpdateChecker { source: Scripted(Mutex::new(script)), notices: notices.clone() };
    let mut failures = 0;
    assert_eq!(checker.step(&mut failures).await, release::CACHE_TIME);
    assert_eq!(notices.status().state, UpdateState::Available);
    let available = notices.status().revision;
    assert_eq!(checker.step(&mut failures).await, Duration::from_mins(15));
    let outage = notices.status();
    assert_eq!((outage.state, outage.release.unwrap().version.as_str()), (UpdateState::Unavailable, "99.0.0"));
    assert!(outage.revision > available);
    assert_eq!(checker.step(&mut failures).await, Duration::from_hours(2), "a longer upstream hint wins");
    assert_eq!(failures, 2);
    assert_eq!(checker.step(&mut failures).await, release::CACHE_TIME);
    assert_eq!(failures, 0);
    assert_eq!((notices.status().state, notices.status().release), (UpdateState::UpToDate, None));
}
