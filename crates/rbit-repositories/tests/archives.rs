//! Archive admission, naming and ordering over a scripted port.
use rbit_repositories::archives::{ArchiveError, ArchivePort, ArchiveRefusal, MAX_CONCURRENT_ARCHIVES, RepositoryArchives};
use std::{
    future::{Future, ready},
    net::{IpAddr, Ipv4Addr},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

#[derive(Default)]
struct Port {
    refusal: Option<ArchiveRefusal>,
    missing: bool,
    calls: Mutex<Vec<String>>,
}
impl Port {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
}
impl ArchivePort for Port {
    type Format = bool;
    type Slot = &'static str;
    type Stream = String;
    type Error = &'static str;
    fn admit(&self, client: IpAddr) -> Result<&'static str, ArchiveRefusal> {
        self.log(format!("admit {client}"));
        self.refusal.map_or(Ok("slot"), Err)
    }
    fn commit_id(&self, revision: &str) -> impl Future<Output = Result<String, &'static str>> + Send {
        self.log(format!("commit {revision}"));
        ready(if self.missing { Err("no such revision") } else { Ok("c0ffee".to_owned()) })
    }
    fn start(&self, commit: &str, base: &str, zip: bool) -> impl Future<Output = Result<String, &'static str>> + Send {
        self.log(format!("start {commit} {base} {zip}"));
        ready(Ok(format!("{base}/")))
    }
    fn extension(zip: bool) -> &'static str {
        if zip { ".zip" } else { ".tar.gz" }
    }
    fn started(&self, zip: bool) {
        self.log(format!("started {zip}"));
    }
}

const CLIENT: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[test]
fn admitted_archives_are_named_after_the_sanitized_revision() {
    let port = Port::default();
    let archive = run(RepositoryArchives::start(&port, "demo", "feature/login", false, CLIENT)).unwrap();
    assert_eq!((archive.commit_id.as_str(), archive.file_name.as_str()), ("c0ffee", "demo-feature-login.tar.gz"));
    assert_eq!((archive.stream.as_str(), archive.slot), ("demo-feature-login/", "slot"));
    assert_eq!(
        *port.calls.lock().unwrap(),
        ["admit 127.0.0.1", "commit feature/login", "start c0ffee demo-feature-login false", "started false"]
    );
    assert_eq!(MAX_CONCURRENT_ARCHIVES, 4);
}

#[test]
fn refusals_do_no_git_work_and_failures_keep_their_identity() {
    for (refusal, text) in [
        (ArchiveRefusal::Busy, "too many archives are being prepared; try again shortly"),
        (ArchiveRefusal::TooManyFromClient, "too many archives are being downloaded from this address; try again shortly"),
    ] {
        let port = Port { refusal: Some(refusal), ..Port::default() };
        let error = run(RepositoryArchives::start(&port, "demo", "main", true, CLIENT)).unwrap_err();
        assert_eq!(error.to_string(), text);
        assert_eq!(port.calls.lock().unwrap().len(), 1, "only admission ran");
    }
    let port = Port { missing: true, ..Port::default() };
    let error = run(RepositoryArchives::start(&port, "demo", "nope", true, CLIENT)).unwrap_err();
    assert!(matches!(error, ArchiveError::Service("no such revision")));
    assert!(!port.calls.lock().unwrap().iter().any(|call| call.starts_with("start")), "nothing started, nothing counted");
}
