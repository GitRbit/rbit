//! The `rbit-runner` binary as people start it. Having an integration test also makes
//! `cargo test --workspace` build the binary, which the end-to-end suite (`rbit-e2e`, the
//! `actions_runner` module) runs against a real server.

use std::process::Command;

fn runner() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rbit-runner"))
}

#[test]
fn it_says_its_version_and_refuses_to_run_unregistered() {
    let output = runner().arg("--version").output().expect("start rbit-runner");
    assert!(output.status.success());
    let version = String::from_utf8_lossy(&output.stdout);
    assert!(version.starts_with("rbit-runner ") && version.contains(env!("CARGO_PKG_VERSION")), "{version}");

    let dir = tempfile::tempdir().unwrap();
    let output = runner().args(["--config", &dir.path().join("runner.toml").to_string_lossy(), "run"]).output().expect("start rbit-runner");
    assert!(!output.status.success(), "no runner.toml, no credential: it cannot run");
}

#[test]
fn registering_over_plain_http_needs_the_explicit_switch() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("runner.toml");
    let output = runner()
        .args(["--config", &config.to_string_lossy(), "register", "--url", "http://git.example.com", "--labels", "self-hosted:host"])
        .env("RBIT_RUNNER_TOKEN", "rbrg_token")
        .output()
        .expect("start rbit-runner");
    assert!(!output.status.success());
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(message.contains("https") || message.contains("insecure"), "{message}");
    assert!(!config.exists(), "nothing is written for a refused registration");
}
