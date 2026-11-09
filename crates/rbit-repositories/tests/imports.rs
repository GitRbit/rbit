use rbit_repositories::imports::{AdmissionError, DISABLED, ImportAdmission, ImportRun, UNAVAILABLE, admit, run};
use std::{
    future::{Future, ready},
    sync::Mutex,
};

/// Both ports; `trace` records calls in order.
#[derive(Default)]
struct World {
    trace: Mutex<Vec<String>>,
    disabled: bool,
    /// The first step that fails.
    fail: &'static str,
    description: Option<&'static str>,
    unavailable: bool,
}
impl World {
    fn push(&self, entry: impl Into<String>) {
        self.trace.lock().unwrap().push(entry.into());
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
    fn step(&self, name: &str) -> Result<(), String> {
        self.push(name);
        if self.fail == name { Err(format!("{name} failed")) } else { Ok(()) }
    }
}

impl ImportAdmission for World {
    type Error = &'static str;
    type Source = ();
    type Client = ();
    type Metadata = String;
    type Created = String;
    fn enabled(&self) -> bool {
        !self.disabled
    }
    fn source(&self) -> Result<(), String> {
        self.step("source")
    }
    fn source_name((): &()) -> String {
        "octo/tools".into()
    }
    fn source_repository_name((): &()) -> String {
        "tools".into()
    }
    fn check_host(&self, (): &()) -> impl Future<Output = Result<(), String>> + Send {
        ready(self.step("host"))
    }
    fn client(&self, (): &(), token: Option<&str>) -> Result<(), String> {
        self.step(&format!("client:{token:?}"))
    }
    fn metadata(&self, (): &()) -> impl Future<Output = Result<String, String>> + Send {
        ready(self.step("metadata").map(|()| self.description.unwrap_or_default().to_owned()))
    }
    fn description(metadata: &String) -> Option<&str> {
        Some(metadata.as_str()).filter(|description| !description.is_empty())
    }
    fn create(&self, name: String, description: String) -> impl Future<Output = Result<String, &'static str>> + Send {
        self.push(format!("create:{name}:{description}"));
        ready(if self.fail == "create" { Err("taken") } else { Ok(name) })
    }
    fn record(&self, created: &String, (): &()) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.push(format!("record:{created}"));
        ready(if self.fail == "record" { Err("locked") } else { Ok(()) })
    }
    fn spawn(&self, created: &String, (): (), _: String) {
        self.push(format!("spawn:{created}"));
    }
}

#[tokio::test]
async fn admission_checks_everything_before_creating_and_spawns_last() {
    let world = World { description: Some("  A tool  "), ..World::default() };
    assert_eq!(admit(&world, " t0k ", "").await, Ok("tools".to_owned()));
    assert_eq!(world.trace(), ["source", "host", "client:Some(\"t0k\")", "metadata", "create:tools:A tool", "record:tools", "spawn:tools"]);
}

#[tokio::test]
async fn source_problems_are_shown_and_nothing_is_created() {
    let world = World { disabled: true, ..World::default() };
    assert_eq!(admit(&world, "", "").await, Err(AdmissionError::Invalid(DISABLED.into())));
    assert_eq!(world.trace(), Vec::<String>::new());
    let world = World::default();
    assert_eq!(admit(&world, "bad token", "").await, Err(AdmissionError::Invalid("the token is not valid".into())));
    assert_eq!(world.trace(), ["source"], "the token is checked before any request");
    for (fail, message) in [("host", "host failed"), ("metadata", "octo/tools: metadata failed")] {
        let world = World { fail, ..World::default() };
        assert_eq!(admit(&world, "", "mine").await, Err(AdmissionError::Invalid(message.into())));
        assert!(!world.trace().iter().any(|entry| entry.starts_with("create")));
    }
    let world = World { fail: "record", ..World::default() };
    assert_eq!(admit(&world, "", "mine").await, Err(AdmissionError::Failure("locked")));
    assert!(!world.trace().iter().any(|entry| entry.starts_with("spawn")));
}

impl ImportRun for World {
    type Context = ();
    fn mark_started(&self) -> impl Future<Output = ()> + Send {
        self.push("started");
        ready(())
    }
    fn open_writable(&self) -> impl Future<Output = Option<()>> + Send {
        self.push("open");
        ready((!self.unavailable).then_some(()))
    }
    fn execute(&self, (): ()) -> impl Future<Output = Result<(), String>> + Send {
        ready(self.step("execute").map_err(|_| " the git copy\u{7} failed ".to_owned()))
    }
    fn observe(&self, outcome: &Result<(), String>) {
        self.push(format!("observe:{}", outcome.is_ok()));
    }
    fn clean(message: &str) -> String {
        message.trim().chars().filter(|c| !c.is_control()).collect()
    }
}

#[tokio::test]
async fn a_run_is_marked_reopened_executed_observed_and_cleaned() {
    let world = World::default();
    assert_eq!(run(&world).await, None);
    assert_eq!(world.trace(), ["started", "open", "execute", "observe:true"]);
    let world = World { fail: "execute", ..World::default() };
    assert_eq!(run(&world).await.as_deref(), Some("the git copy failed"));
    let world = World { unavailable: true, ..World::default() };
    assert_eq!(run(&world).await.as_deref(), Some(UNAVAILABLE));
    assert_eq!(world.trace(), ["started", "open"], "nothing runs or is counted for a lost repository");
}
