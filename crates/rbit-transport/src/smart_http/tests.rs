// The fakes answer at once; their methods stay `async fn` like the real adapters.
#![allow(unknown_lints, clippy::unused_async_trait_impl)]

use std::sync::Mutex;

use super::{
    access::{AccessRefusal, RepositoryGate, authorize},
    credentials::{basic_credentials, bearer_task_token},
    *,
};
use crate::Service;

#[test]
fn gzip_bodies_are_recognised_in_any_case() {
    assert!(is_gzip(b"gzip") && is_gzip(b"GZip"));
    assert!(!is_gzip(b"deflate") && !is_gzip(b"x-gzip") && !is_gzip(b""));
}

#[test]
fn basic_credentials_keep_colons_in_the_password() {
    assert_eq!(basic_credentials("Basic YWxpY2U6czNjcjN0OndpdGg6Y29sb25z"), Some(("alice".into(), "s3cr3t:with:colons".into())));
    assert_eq!(basic_credentials("basic YWxpY2U6eA=="), Some(("alice".into(), "x".into())));
    for refused in ["Bearer YWxpY2U6eA==", "Basic !!!", "Basic bm9jb2xvbg==", "BASIC YWxpY2U6eA=="] {
        assert_eq!(basic_credentials(refused), None, "{refused}");
    }
}

#[test]
fn only_task_tokens_are_read_as_bearer_tokens() {
    let token = format!("rbat_{}", "A".repeat(43));
    assert_eq!(bearer_task_token(&format!("Bearer {token}")), Some(token.as_str()));
    assert_eq!(bearer_task_token(&format!("  token   {token} ")), Some(token.as_str()));
    assert_eq!(bearer_task_token("Bearer rbit_personal"), None);
    assert_eq!(bearer_task_token(&format!("Basic {token}")), None);
}

/// A gate answering from fixed outcomes and recording the order of its calls.
#[derive(Debug, Default)]
struct Gate {
    found: bool,
    failed: bool,
    writable: bool,
    mirror: Option<bool>,
    calls: Mutex<Vec<&'static str>>,
}

impl RepositoryGate for Gate {
    type Repository = ();
    type Error = &'static str;

    async fn open(&self, _owner: &str, _name: &str) -> Result<Option<()>, &'static str> {
        self.calls.lock().unwrap().push("open");
        if self.failed { Err("database down") } else { Ok(self.found.then_some(())) }
    }

    fn can_write(&self, (): &()) -> bool {
        self.calls.lock().unwrap().push("can_write");
        self.writable
    }

    async fn is_mirror(&self, (): &()) -> Result<bool, &'static str> {
        self.calls.lock().unwrap().push("is_mirror");
        self.mirror.ok_or("database down")
    }
}

async fn decide(gate: &Gate, anonymous: bool, service: Service) -> Result<(), AccessRefusal> {
    authorize(gate, anonymous, "alice", "project", service).await
}

#[tokio::test]
async fn missing_repositories_challenge_only_anonymous_callers() {
    let gate = Gate::default();
    assert_eq!(decide(&gate, true, Service::UploadPack).await, Err(AccessRefusal::Challenge));
    assert_eq!(decide(&gate, false, Service::UploadPack).await, Err(AccessRefusal::NotFound));
    let failed = Gate { failed: true, ..Gate::default() };
    assert_eq!(decide(&failed, true, Service::UploadPack).await, Err(AccessRefusal::Internal));
}

#[tokio::test]
async fn fetches_need_no_write_or_mirror_check() {
    let gate = Gate { found: true, ..Gate::default() };
    assert_eq!(decide(&gate, true, Service::UploadPack).await, Ok(()));
    assert_eq!(*gate.calls.lock().unwrap(), ["open"]);
}

#[tokio::test]
async fn pushes_check_write_access_before_revealing_a_mirror() {
    let reader = Gate { found: true, mirror: Some(true), ..Gate::default() };
    assert_eq!(decide(&reader, true, Service::ReceivePack).await, Err(AccessRefusal::Challenge));
    assert_eq!(decide(&reader, false, Service::ReceivePack).await, Err(AccessRefusal::Forbidden));
    assert!(!reader.calls.lock().unwrap().contains(&"is_mirror"));

    let mirror = Gate { found: true, writable: true, mirror: Some(true), ..Gate::default() };
    assert_eq!(decide(&mirror, false, Service::ReceivePack).await, Err(AccessRefusal::Mirror));
    assert_eq!(*mirror.calls.lock().unwrap(), ["open", "can_write", "is_mirror"]);
    let unknown = Gate { found: true, writable: true, ..Gate::default() };
    assert_eq!(decide(&unknown, false, Service::ReceivePack).await, Err(AccessRefusal::Internal));
    let writer = Gate { found: true, writable: true, mirror: Some(false), ..Gate::default() };
    assert_eq!(decide(&writer, false, Service::ReceivePack).await, Ok(()));
}

#[tokio::test]
async fn capped_readers_fail_past_their_limit() {
    use tokio::io::AsyncReadExt as _;
    let mut within = Vec::new();
    Capped::new(&[7u8; 100][..], 100).read_to_end(&mut within).await.unwrap();
    assert_eq!(within.len(), 100, "exactly the limit is accepted");
    let mut over = Vec::new();
    let error = Capped::new(&[7u8; 101][..], 100).read_to_end(&mut over).await.unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(over.len() <= 100, "{}", over.len());
}
