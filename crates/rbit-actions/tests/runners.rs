//! Runner registration, declaration and management decisions over scripted ports.
#![allow(clippy::unused_async_trait_impl)]
use rbit_actions::runners::{
    Declared, ManagedTarget, Management, RecentKeys, Registration, RegistrationOptions, RunnerDeclaration, RunnerLabels, RunnerRefusal,
    RunnerRegistration, RunnerRegistry, intervals, labels_changed, listed_for_repository, management, seen_is_due, shown_online,
};
use rbit_workflow::{
    protocol::{
        Backend, Capabilities, DeclareRequest, DeclareResponse, DeclaredLabel, Intervals, RegisterRequest, RegisterResponse, RunnerId,
    },
    secret::Secret,
};
use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
    time::Duration,
};

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("scripted ports are always ready"),
    }
}

const LINUX: &[(&str, Backend)] = &[("linux", Backend::Container)];
const INTERVALS: Intervals = Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 };

fn capabilities() -> Capabilities {
    Capabilities {
        backends: vec![Backend::Container, Backend::Host],
        os: "linux".into(),
        arch: "x86_64".into(),
        capacity: 1,
        version: "0.2.0".into(),
        ephemeral: false,
        exclusive_labels: Vec::new(),
    }
}

fn labels(names: &[(&str, Backend)]) -> Vec<DeclaredLabel> {
    names.iter().map(|(name, backend)| DeclaredLabel { name: (*name).into(), backend: *backend }).collect()
}

#[derive(Debug, Default)]
struct Port {
    calls: Mutex<Vec<String>>,
    outcome: Option<&'static str>,
    pinned: Option<Vec<String>>,
    current: (Vec<String>, Vec<String>),
    gone: bool,
}

impl Port {
    fn call(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl RunnerLabels for Port {
    type Error = &'static str;
    type Label = String;
    type Capabilities = Vec<String>;
    fn parse_label(&self, name: &str) -> Result<String, String> {
        if name.contains(' ') { Err(format!("label `{name}` is malformed")) } else { Ok(name.to_owned()) }
    }
    fn capabilities(&self, _: &Capabilities, host_labels: &[String]) -> Vec<String> {
        host_labels.to_vec()
    }
}

impl RunnerRegistration for Port {
    type Secret = String;
    type Credential = &'static str;
    type Runner = Vec<String>;
    fn parse_secret(&self, token: &str) -> Option<String> {
        token.starts_with("rbrg_").then(|| token.to_owned())
    }
    fn new_credential(&self) -> &'static str {
        "rbrn_x"
    }
    fn register(
        &self,
        _: &String,
        _: &&'static str,
        _: &RegisterRequest,
        declared: &Declared<String, Vec<String>>,
    ) -> impl Future<Output = Result<Registration<Vec<String>>, &'static str>> + Send {
        self.call("register");
        ready(Ok(match self.outcome {
            Some("invalid") => Registration::InvalidToken,
            Some("pinned") => Registration::LabelsNotAllowed,
            Some("bare") => Registration::Registered(Vec::new()),
            _ => Registration::Registered(declared.labels.clone()),
        }))
    }
    async fn admit(&self, _: &Vec<String>, _: &&'static str) -> Result<(), &'static str> {
        self.call("admit");
        Ok(())
    }
    fn has_labels(&self, runner: &Vec<String>) -> bool {
        !runner.is_empty()
    }
    fn bump_tasks_version(&self) {
        self.call("bump");
    }
    fn response(&self, _: &Vec<String>, credential: &&'static str) -> RegisterResponse {
        RegisterResponse { runner_id: RunnerId(1), credential: (*credential).to_owned().into(), intervals: INTERVALS }
    }
}

impl RunnerDeclaration for Port {
    type Runner = ();
    fn pinned(&self) -> Option<&[String]> {
        self.pinned.as_deref()
    }
    fn current(&self) -> (&[String], &[String]) {
        (&self.current.0, &self.current.1)
    }
    fn now(&self) -> i64 {
        7
    }
    async fn declare(&self, _: &DeclareRequest, _: &Declared<String, Vec<String>>, now: i64) -> Result<Option<()>, &'static str> {
        self.call(format!("declare {now}"));
        Ok((!self.gone).then_some(()))
    }
    async fn forget(&self) -> Result<(), &'static str> {
        self.call("forget");
        Ok(())
    }
    fn bump_tasks_version(&self) {
        self.call("bump");
    }
    async fn refresh(&self, (): &(), now: i64) -> Result<(), &'static str> {
        self.call(format!("refresh {now}"));
        Ok(())
    }
    fn response(&self, (): &()) -> DeclareResponse {
        DeclareResponse { runner_id: RunnerId(1), intervals: INTERVALS, shared_cache: true }
    }
}

fn register(port: &Port, token: &str, names: &[(&str, Backend)]) -> Result<RegisterResponse, RunnerRefusal<&'static str>> {
    let request = RegisterRequest { token: Secret::new(token), name: "r".into(), labels: labels(names), capabilities: capabilities() };
    run(RunnerRegistry::register(port, &request))
}

fn declare(port: &Port, names: &[(&str, Backend)]) -> Result<DeclareResponse, RunnerRefusal<&'static str>> {
    run(RunnerRegistry::declare(port, &DeclareRequest { name: "r".into(), labels: labels(names), capabilities: capabilities() }))
}

#[test]
fn registration_refusals_keep_their_protocol_answers() {
    let unknown = "the registration token is unknown, revoked, expired or used up";
    let port = Port::default();
    assert!(matches!(register(&port, "nope", LINUX), Err(RunnerRefusal::Unauthenticated(text)) if text == unknown));
    assert!(matches!(register(&port, "rbrg_x", &[("a b", Backend::Container)]), Err(RunnerRefusal::BadRequest(_))));
    assert_eq!(port.calls(), Vec::<String>::new(), "nothing is stored before the token and labels parse");
    let invalid = Port { outcome: Some("invalid"), ..Port::default() };
    assert!(matches!(register(&invalid, "rbrg_x", LINUX), Err(RunnerRefusal::Unauthenticated(text)) if text == unknown));
    let pinned = Port { outcome: Some("pinned"), ..Port::default() };
    let forbidden = "the registration token allows other labels than these";
    assert!(matches!(register(&pinned, "rbrg_x", LINUX), Err(RunnerRefusal::Forbidden(text)) if text == forbidden));
}

#[test]
fn only_runners_registered_with_labels_move_the_scheduler() {
    let labelled = Port::default();
    assert_eq!(register(&labelled, "rbrg_x", &[("linux", Backend::Container)]).unwrap().credential.expose(), "rbrn_x");
    assert_eq!(labelled.calls(), ["register", "admit", "bump"]);
    let bare = Port { outcome: Some("bare"), ..Port::default() };
    register(&bare, "rbrg_x", LINUX).unwrap();
    assert_eq!(bare.calls(), ["register", "admit"]);
}

#[test]
fn declarations_respect_pins_and_bump_only_on_label_changes() {
    let pinned = Port { pinned: Some(vec!["linux".into()]), ..Port::default() };
    let refused = declare(&pinned, &[("linux", Backend::Container), ("gpu", Backend::Container)]);
    assert!(
        matches!(refused, Err(RunnerRefusal::Forbidden(text)) if text == "this runner's registration token does not allow label `gpu`")
    );
    assert_eq!(pinned.calls(), Vec::<String>::new());
    let same = Port { current: (vec!["b".into(), "a".into()], vec!["a".into()]), ..Port::default() };
    declare(&same, &[("a", Backend::Host), ("b", Backend::Container)]).unwrap();
    assert_eq!(same.calls(), ["declare 7", "refresh 7"], "order does not matter");
    let moved = Port { current: (vec!["a".into(), "b".into()], vec![]), ..Port::default() };
    declare(&moved, &[("a", Backend::Host), ("b", Backend::Container)]).unwrap();
    assert_eq!(moved.calls(), ["declare 7", "bump", "refresh 7"], "a label moved to the host");
    let gone = Port { gone: true, ..Port::default() };
    let unknown = "the runner credential is missing, malformed or unknown";
    assert!(matches!(declare(&gone, LINUX), Err(RunnerRefusal::Unauthenticated(text)) if text == unknown));
    assert_eq!(gone.calls(), ["declare 7", "forget"]);
}

#[test]
fn registration_tokens_last_a_minute_to_thirty_days_with_some_use() {
    let options = RegistrationOptions::<String>::default;
    assert_eq!(options().expires_at(1_000, 64), Ok(4_600));
    let short = RegistrationOptions { lifetime: Duration::from_secs(1), ..options() };
    assert_eq!(short.expires_at(1_000, 64), Ok(1_060), "a minute at least");
    let lifetime = Err("a registration token lasts from a minute up to 30 days".to_owned());
    assert_eq!(RegistrationOptions { lifetime: Duration::ZERO, ..options() }.expires_at(0, 64), lifetime);
    assert_eq!(RegistrationOptions { lifetime: Duration::from_hours(31 * 24), ..options() }.expires_at(0, 64), lifetime);
    let uses = Err("a registration token must allow at least one registration".to_owned());
    assert_eq!(RegistrationOptions { uses: Some(0), ..options() }.expires_at(0, 64), uses);
    let pins = Err("pin 1 to 2 labels, or none to allow any".to_owned());
    assert_eq!(RegistrationOptions { labels: Some(vec![]), ..options() }.expires_at(0, 2), pins);
    assert_eq!(RegistrationOptions { labels: Some(vec!["a".into(); 3]), ..options() }.expires_at(0, 2), pins);
    assert_eq!(options().audit_detail(3, 99), "#3 until 99, 1 use(s)");
    assert_eq!(RegistrationOptions::<String> { uses: None, ..options() }.audit_detail(3, 99), "#3 until 99, unlimited uses");
}

#[test]
fn managers_are_admins_owners_and_repository_admins() {
    let decide = |admin, target, owner: bool| {
        let asked = Mutex::new(false);
        let decision = run(management::<(), _>(admin, target, || {
            *asked.lock().unwrap() = true;
            ready(Ok(owner))
        }))
        .unwrap();
        (decision, asked.into_inner().unwrap())
    };
    let org = ManagedTarget::Owner { own_account: false, organization: true };
    assert_eq!(decide(true, ManagedTarget::Instance, false), (Management::Allowed, false));
    assert_eq!(decide(false, ManagedTarget::Instance, true), (Management::Hidden, false));
    assert_eq!(decide(false, ManagedTarget::Owner { own_account: true, organization: false }, false), (Management::Allowed, false));
    assert_eq!(decide(true, org, false), (Management::Allowed, false), "administrators are not looked up");
    assert_eq!(decide(false, org, true), (Management::Allowed, true));
    assert_eq!(decide(false, org, false), (Management::Hidden, true));
    assert_eq!(decide(false, ManagedTarget::Owner { own_account: false, organization: false }, true), (Management::Hidden, false));
    assert_eq!(decide(false, ManagedTarget::Repository { admin: true, read: true }, false), (Management::Allowed, false));
    assert_eq!(decide(true, ManagedTarget::Repository { admin: false, read: true }, false), (Management::Forbidden, false));
    assert_eq!(decide(true, ManagedTarget::Repository { admin: false, read: false }, false), (Management::Hidden, false));
}

#[test]
fn bookkeeping_rules() {
    let none: &[&str] = &[];
    assert!(labels_changed((&["a"], none), (&["b"], none)));
    assert!(!labels_changed((&["a", "b"], &["b"]), (&["b", "a"], &["b"])));
    assert!(seen_is_due(160, 100) && !seen_is_due(159, 100) && !seen_is_due(100, 160));
    assert_eq!(shown_online(Some(100), Some(170)), Some(170));
    assert_eq!(shown_online(Some(100), Some(50)), Some(100));
    assert_eq!(shown_online(None, Some(0)), None);
    assert_eq!(shown_online(Some(100), None), Some(100));
    assert_eq!(intervals(0), Intervals { long_poll_seconds: 1, heartbeat_seconds: 10 });
    assert_eq!(intervals(600).long_poll_seconds, 25);
    assert!(listed_for_repository(false, false) && listed_for_repository(true, true) && !listed_for_repository(true, false));
    let mut keys = RecentKeys::default();
    for task in 0..40 {
        keys.remember(&format!("k{task}"), task);
    }
    assert_eq!((keys.remembered("k39"), keys.remembered("k7"), keys.remembered("k8")), (Some(39), None, Some(8)));
}
