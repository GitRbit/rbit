//! Real Git compatibility and process lifetime regressions.
use std::{
    io,
    pin::Pin,
    process::ExitStatus,
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::oneshot,
};

use super::*;

#[test]
fn protocol_header_filter() {
    assert!(is_safe_protocol_header("version=2"));
    assert!(!is_safe_protocol_header("version=2\nGIT_DIR=/etc"));
    assert!(!is_safe_protocol_header("$(rm -rf /)"));
}

fn args(transport: &ProcessTransport, service: Service, options: ReceiveOptions) -> Vec<String> {
    transport.service_command(service, None, options).as_std().get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
}

#[test]
fn hooks_never_run_and_pushes_are_capped() {
    let hooks = format!("core.hooksPath={}", null_device().to_string_lossy());
    let unlimited = ProcessTransport::new("git");
    let fetch = args(&unlimited, Service::UploadPack, ReceiveOptions::default());
    assert_eq!(fetch, ["-c", hooks.as_str(), "upload-pack"]);
    let push = args(&unlimited, Service::ReceivePack, ReceiveOptions { deny_non_fast_forwards: true });
    assert_eq!(push, ["-c", hooks.as_str(), "-c", "receive.denyNonFastForwards=true", "receive-pack"]);

    let capped = ProcessTransport::new("git").with_max_push_bytes(Some(1024));
    let push = args(&capped, Service::ReceivePack, ReceiveOptions::default());
    assert_eq!(push, ["-c", hooks.as_str(), "-c", "receive.maxInputSize=1024", "receive-pack"]);
    let fetch = args(&capped, Service::UploadPack, ReceiveOptions::default());
    assert!(!fetch.iter().any(|arg| arg.contains("maxInputSize")), "{fetch:?}");
}

fn bare_repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let status = std::process::Command::new("git").args(["init", "--quiet", "--bare"]).arg(dir.path()).status().unwrap();
    assert!(status.success());
    dir
}

async fn interactive(service: Service, repo: &Path, protocol: Option<&str>, input: &'static [u8]) -> (ExitStatus, Vec<u8>) {
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    let status = ProcessTransport::new("git").serve_interactive(service, repo, protocol, input, &mut output, &mut errors).await.unwrap();
    (status, output)
}

#[tokio::test]
async fn interactive_upload_pack_advertises_refs_then_exits() {
    let repo = bare_repository();
    let (status, output) = interactive(Service::UploadPack, repo.path(), None, b"0000").await;
    assert!(status.success());
    let output = String::from_utf8_lossy(&output);
    assert!(output.contains("capabilities^{}"), "v0 advertisement: {output}");
    assert!(output.ends_with("0000"), "{output}");
}

#[tokio::test]
async fn interactive_mode_honours_only_safe_protocol_requests() {
    let repo = bare_repository();
    let (status, output) = interactive(Service::UploadPack, repo.path(), Some("version=2"), b"0000").await;
    assert!(status.success());
    assert!(output.starts_with(b"000eversion 2\n"), "{}", String::from_utf8_lossy(&output));

    let (_, output) = interactive(Service::UploadPack, repo.path(), Some("version=2\nx"), b"0000").await;
    assert!(!output.starts_with(b"000eversion 2\n"), "unsafe values are ignored");
}

#[tokio::test]
async fn interactive_receive_pack_finishes_when_the_client_sends_nothing() {
    let repo = bare_repository();
    let (status, output) = interactive(Service::ReceivePack, repo.path(), None, b"0000").await;
    assert!(status.success());
    assert!(String::from_utf8_lossy(&output).contains("report-status"));
}

#[tokio::test]
async fn interactive_mode_returns_when_git_exits_even_if_the_client_stays_open() {
    let repo = bare_repository();
    let (client, mut server_side) = tokio::io::duplex(64);
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    tokio::io::AsyncWriteExt::write_all(&mut server_side, b"0000").await.unwrap();
    let transport = ProcessTransport::new("git");
    let exchange = transport.serve_interactive(Service::UploadPack, repo.path(), None, client, &mut output, &mut errors);
    let status = tokio::time::timeout(std::time::Duration::from_secs(30), exchange).await.expect("did not hang").unwrap();
    assert!(status.success());
    drop(server_side);
}

#[tokio::test]
async fn the_raw_advertisement_has_no_http_service_header() {
    let repo = bare_repository();
    let transport = ProcessTransport::new("git");
    let raw = transport.advertisement(Service::ReceivePack, repo.path(), None).await.unwrap();
    assert!(String::from_utf8_lossy(&raw).contains("report-status"));
    assert!(!raw.starts_with(b"001f# service="));
    let http = transport.advertise_refs(Service::ReceivePack, repo.path(), None).await.unwrap();
    assert!(http.starts_with(b"001f# service=git-receive-pack\n0000"));
    assert!(http.ends_with(&raw));
}

#[tokio::test]
async fn stateless_receive_pack_with_an_empty_command_list_exits_cleanly() {
    let repo = bare_repository();
    let (client, mut server_side) = tokio::io::duplex(64);
    tokio::io::AsyncWriteExt::write_all(&mut server_side, b"0000").await.unwrap();
    let (mut output, mut errors) = (Vec::new(), Vec::new());
    let transport = ProcessTransport::new("git");
    // The client keeps its stream open, as an SSH client waiting for the report does.
    let exchange = transport.receive_pack(repo.path(), None, ReceiveOptions::default(), client, &mut output, &mut errors);
    let status = tokio::time::timeout(std::time::Duration::from_secs(30), exchange).await.expect("did not hang").unwrap();
    assert!(status.success(), "{}", String::from_utf8_lossy(&errors));
    assert!(output.is_empty(), "nothing to report: {}", String::from_utf8_lossy(&output));
    drop(server_side);
}

#[tokio::test]
async fn dropping_a_stateless_response_stops_git_and_releases_the_request() {
    struct PendingRequest(Option<oneshot::Sender<()>>);
    impl AsyncRead for PendingRequest {
        fn poll_read(self: Pin<&mut Self>, _: &mut Context<'_>, _: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            Poll::Pending
        }
    }
    impl Drop for PendingRequest {
        fn drop(&mut self) {
            let _ = self.0.take().unwrap().send(());
        }
    }

    let repo = bare_repository();
    let (dropped, request_dropped) = oneshot::channel();
    let session = ProcessTransport::new("git")
        .serve(Service::UploadPack, repo.path(), None, ReceiveOptions::default(), PendingRequest(Some(dropped)))
        .unwrap();
    drop(session.output);
    let stopped = async {
        let status = session.completion.await.unwrap();
        assert!(!status.success(), "an unfinished exchange must be killed");
        request_dropped.await.unwrap();
    };
    tokio::time::timeout(std::time::Duration::from_secs(10), stopped).await.expect("abandoned exchange stayed alive");
}

#[tokio::test]
async fn a_failed_output_does_not_wait_for_the_other_pipe() {
    struct Disconnected;
    impl AsyncWrite for Disconnected {
        fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
            Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()))
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    let repo = bare_repository();
    let (input, _client) = tokio::io::duplex(64);
    let transport = ProcessTransport::new("git");
    let exchange = transport.serve_interactive(Service::UploadPack, repo.path(), None, input, Disconnected, tokio::io::sink());
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), exchange).await.expect("failed output hung").unwrap();
    assert!(!status.success());
}

#[tokio::test]
async fn invalid_v2_header_keeps_the_v0_http_service_header() {
    let repo = bare_repository();
    let output = ProcessTransport::new("git").advertise_refs(Service::UploadPack, repo.path(), Some("version=2:invalid\n")).await.unwrap();
    assert!(output.starts_with(b"001e# service=git-upload-pack\n0000"));
}
