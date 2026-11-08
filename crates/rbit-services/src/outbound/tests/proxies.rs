//! Proxy environment variables: a proxy resolves names itself, so a request sent through
//! one would bypass the address checks. Every outbound client ignores them. The variables
//! are process-wide, so the requests run in a child copy of this test binary.

use std::{path::Path, process::Stdio, time::Duration};

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::{LOOPBACK, Reply, Server, name};
use crate::{
    external_auth::client::ProviderClient,
    imports::client::SourceClient,
    outbound::{check_public_host, scripted},
    webhooks::delivery::build_client,
};

const CHILD: &str = "RBIT_OUTBOUND_PROXY_CHILD";
const CHILD_TEST: &str = "outbound::tests::proxies::child_sends_every_outbound_request_directly";
const VARIABLES: [&str; 6] = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"];

#[tokio::test]
async fn proxy_environment_variables_do_not_route_outbound_requests() {
    let proxy = Server::always(Reply::status("200 OK", "", b"proxied")).await;
    let mut child = tokio::process::Command::new(std::env::current_exe().unwrap());
    child.args([CHILD_TEST, "--exact", "--nocapture", "--test-threads=1"]).env(CHILD, "1").env_remove("NO_PROXY").env_remove("no_proxy");
    for variable in VARIABLES {
        child.env(variable, proxy.url(""));
    }
    let output = tokio::time::timeout(Duration::from_secs(120), child.stdout(Stdio::piped()).stderr(Stdio::piped()).output())
        .await
        .expect("the child finished")
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("every outbound client connected directly"), "the child ran its requests: {stdout}");
    let heads = proxy.heads();
    assert_eq!(heads.len(), 1, "only the control request used the proxy: {heads:?}");
    assert!(heads[0].contains("/control"), "{heads:?}");
}

/// Runs only as the child of the test above.
#[tokio::test]
async fn child_sends_every_outbound_request_directly() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let target = Server::always(Reply::status("200 OK", "Content-Type: application/json\r\n", b"{}")).await;
    let host = name("proxy-target");
    let _dns = scripted::serve(&host, &[&[LOOPBACK]]);
    let base = format!("http://{host}:{}", target.address.port());

    crate::webhooks::delivery::install_crypto_provider();
    // Control: an ordinary client honors the variables, so they are in effect here.
    let control = reqwest::Client::new().get(format!("{base}/control")).send().await.unwrap();
    assert_eq!(control.text().await.unwrap(), "proxied", "the environment names the proxy");

    let hooks = build_client(true, Duration::from_secs(5), true).unwrap();
    assert!(hooks.post(format!("{base}/hook")).body("{}").send().await.unwrap().status().is_success());

    let source = rbit_core::import::ImportSource::parse(rbit_core::import::SourceKind::Gitea, &base, "acme/rocket").unwrap();
    let imports = SourceClient::new(source, None, true, Duration::from_secs(5), Duration::ZERO, CancellationToken::new(), true).unwrap();
    let _: Value = imports.get(&format!("{base}/api")).await.unwrap();
    imports.download(&format!("{base}/download"), &reqwest::header::HeaderMap::new(), false).await.unwrap();

    let providers = ProviderClient::new(true, Duration::from_secs(5), true).unwrap();
    providers.get_json(&format!("{base}/.well-known/openid-configuration"), None, &[]).await.unwrap();

    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("mirror.git");
    assert!(std::process::Command::new("git").args(["init", "-q", "--bare"]).arg(&repo).status().unwrap().success());
    let pinned = check_public_host(&host, target.address.port(), true).await.unwrap();
    let url = format!("{base}/acme/rocket.git");
    let _ = rbit_git::mirror::fetch_mirror(Path::new("git"), &repo, &url, Some(&pinned), Duration::from_secs(30)).await;

    let paths: Vec<String> = target.heads().iter().filter_map(|head| head.split_whitespace().nth(1).map(str::to_owned)).collect();
    for expected in ["/hook", "/api", "/download", "/.well-known/openid-configuration", "/acme/rocket.git/info/refs"] {
        assert!(paths.iter().any(|path| path.starts_with(expected)), "{expected} went directly: {paths:?}");
    }
    println!("every outbound client connected directly");
}
