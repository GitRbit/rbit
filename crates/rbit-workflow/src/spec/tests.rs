//! Exact spec tests acceptance fixtures.

use serde_json::json;

use super::{
    Access, ActionsPolicy, Builtin, BuiltinKind, CacheScopes, CheckoutSource, ConcurrencyGroup, Container, Contexts,
    DEFAULT_TIMEOUT_MINUTES, Env, JOB_SPEC_VERSION, JOB_TEMPLATE_VERSION, JobResult, JobSpec, JobTemplate, MAX_SECRET_BYTES, MAX_SERVICES,
    MAX_STEPS, NeedSource, Needed, RegistryCredentials, RunDefaults, Service, StepAction, StepTemplate, Strategy, TokenPermissions,
    valid_job_key,
};
use crate::model::{Permission, Permissions};
use crate::protocol::{PollResponse, TaskId, tests::Refusal};
use crate::secret::Secret;
use serde::Serialize;
use serde_json::Map;
use std::collections::BTreeMap;

const TOKEN: &str = "rbat_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

#[test]
fn called_jobs_have_keys_of_their_calls() {
    for key in ["build", "call/build", "call#1/build", "outer/inner#0/leaf"] {
        assert!(valid_job_key(key), "{key}");
    }
    for key in ["", "call/", "/build", "call#/build", "call#x/build", "a b", &"x/".repeat(60)] {
        assert!(!valid_job_key(key), "{key}");
    }
}
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn template() -> JobTemplate {
    JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: "test".into(),
        name: "test (linux, stable)".into(),
        needs: vec!["build".into()],
        condition: Some("${{ success() && github.ref == 'refs/heads/main' }}".into()),
        runs_on: vec!["ubuntu-latest".into()],
        matrix: json!({"os": "linux", "rust": "stable"}).as_object().cloned().unwrap(),
        strategy: Strategy { fail_fast: true, max_parallel: Some(2), job_index: 0, job_total: 4 },
        permissions: Some(TokenPermissions { contents: Access::Read, issues: Access::None, pull_requests: Access::Write }),
        concurrency: Some(ConcurrencyGroup { group: "ci-refs/heads/main".into(), cancel_in_progress: true }),
        timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        continue_on_error: false,
        workflow_env: Env::from([("CARGO_TERM_COLOR".into(), "always".into())]),
        env: Env::from([("RUST".into(), "${{ matrix.rust }}".into())]),
        defaults: RunDefaults { shell: Some("bash".into()), working_directory: None },
        outputs: BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]),
        container: Some(Container {
            image: "docker.io/library/rust:1.94".into(),
            env: Env::new(),
            credentials: Some(RegistryCredentials { username: "ci".into(), password: "${{ secrets.REGISTRY_PASSWORD }}".into() }),
            options: Vec::new(),
            volumes: Vec::new(),
        }),
        services: vec![Service {
            id: "postgres".into(),
            image: "postgres:17".into(),
            env: Env::from([("POSTGRES_PASSWORD".into(), "pw".into())]),
            credentials: None,
            options: Vec::new(),
            volumes: Vec::new(),
        }],
        steps: vec![
            StepTemplate {
                id: None,
                name: None,
                condition: None,
                action: StepAction::Uses(Builtin {
                    uses: "actions/checkout@v4".into(),
                    kind: BuiltinKind::Checkout,
                    version: 1,
                    with: BTreeMap::from([("fetch-depth".into(), "0".into())]),
                }),
                shell: None,
                working_directory: None,
                env: Env::new(),
                continue_on_error: None,
                timeout_minutes: None,
            },
            StepTemplate {
                id: Some("meta".into()),
                name: Some("Test on ${{ matrix.os }}".into()),
                condition: Some("always()".into()),
                action: StepAction::Run("cargo test --locked\necho \"version=1\" >> \"$GITHUB_OUTPUT\"\n".into()),
                shell: Some("bash".into()),
                working_directory: Some("crates".into()),
                env: Env::from([("TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())]),
                continue_on_error: Some("${{ matrix.rust == 'nightly' }}".into()),
                timeout_minutes: Some("30".into()),
            },
        ],
        environment: None,
        environment_url: None,
        calls: Vec::new(),
        need_names: std::collections::BTreeMap::new(),
        deferred: false,
        default_ceiling: false,
    }
}

fn spec() -> JobSpec {
    JobSpec {
        version: JOB_SPEC_VERSION,
        task_id: TaskId(42),
        template: template(),
        contexts: Contexts {
            github: json!({
                "event_name": "push",
                "ref": "refs/heads/main",
                "sha": SHA,
                "repository": "alice/app",
                "run_id": "9",
                "server_url": "https://git.example.com",
                "event": {"ref": "refs/heads/main"}
            })
            .as_object()
            .cloned()
            .unwrap(),
            inputs: Map::new(),
            vars: BTreeMap::from([("DEPLOY_HOST".into(), "deploy.example.com".into())]),
            needs: BTreeMap::from([(
                "build".into(),
                Needed { result: JobResult::Success, outputs: BTreeMap::from([("artifact".into(), "app.tar".into())]) },
            )]),
        },
        secrets: BTreeMap::from([("REGISTRY_PASSWORD".into(), Secret::new("s3cr3t-registry"))]),
        token: Secret::new(TOKEN),
        checkout: CheckoutSource {
            url: "https://git.example.com/alice/app".into(),
            repository: "alice/app".into(),
            sha: SHA.into(),
            reference: "refs/heads/main".into(),
        },
        cache: CacheScopes { repository_id: 5, read: vec!["refs/heads/main".into()], write: "refs/heads/main".into() },
        actions_policy: ActionsPolicy::default(),
    }
}

/// Pretty JSON equals the golden file and parses back to the value. `RBIT_BLESS=1`
/// rewrites the file after a deliberate change.
#[track_caller]
fn golden_file<T: Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(value: &T, name: &str, expected: &str) {
    let actual = serde_json::to_string_pretty(value).unwrap() + "\n";
    if std::env::var_os("RBIT_BLESS").is_some() {
        std::fs::write(format!("{}/tests/golden/{name}", env!("CARGO_MANIFEST_DIR")), &actual).unwrap();
        return;
    }
    assert_eq!(actual, expected.replace("\r\n", "\n"), "{name}");
    assert_eq!(&serde_json::from_str::<T>(expected).unwrap(), value, "{name}");
}

#[test]
fn golden_template_and_spec() {
    template().validate().unwrap();
    spec().validate().unwrap();
    golden_file(&template(), "job_template.json", include_str!("../../tests/golden/job_template.json"));
    golden_file(&PollResponse { task: spec() }, "poll_response.json", include_str!("../../tests/golden/poll_response.json"));
}

#[test]
fn specs_are_strict() {
    let mut value = serde_json::to_value(spec()).unwrap();
    value["template"]["steps"][1]["privileged"] = json!(true);
    assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown step field");
    let mut value = serde_json::to_value(spec()).unwrap();
    value["template"]["steps"][0]["action"] = json!({"run": "a", "uses": {}});
    assert!(serde_json::from_value::<JobSpec>(value).is_err(), "both run and uses");
    let mut value = serde_json::to_value(spec()).unwrap();
    value["template"]["steps"][0]["action"]["uses"]["kind"] = json!("setup_node");
    assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown built-in");
    let mut value = serde_json::to_value(spec()).unwrap();
    value["sandbox"] = json!(false);
    assert!(serde_json::from_value::<JobSpec>(value).is_err(), "an unknown top-level field");
    let mut value = serde_json::to_value(spec()).unwrap();
    value.as_object_mut().unwrap().remove("cache");
    assert!(serde_json::from_value::<JobSpec>(value).is_err(), "a missing field");
}

#[test]
fn template_validation() {
    let refused: [Refusal<JobTemplate>; 16] = [
        (|t| t.version = 2, "version 2"),
        (|t| t.job_id = "1st".into(), "job id"),
        (|t| t.needs.push("test".into()), "`needs` entry `test`"),
        (|t| t.needs.push("build".into()), "must not repeat"),
        (|t| t.runs_on.clear(), "labels"),
        (|t| t.runs_on.push("ubuntu latest".into()), "label `ubuntu latest`"),
        (|t| t.strategy.job_index = 4, "job-index"),
        (|t| t.strategy.max_parallel = Some(0), "max-parallel"),
        (|t| t.timeout_minutes = 0, "timeout-minutes"),
        (|t| _ = t.env.insert("A=B".into(), String::new()), "`A=B`"),
        (|t| _ = t.outputs.insert("1x".into(), String::new()), "output name"),
        (|t| t.services.push(t.services[0].clone()), "service ids"),
        (|t| t.services[0].image.clear(), "no image"),
        (|t| t.steps.clear(), "1 to 256 steps"),
        (|t| t.steps[0].id = Some("meta".into()), "step ids"),
        (|t| t.steps[0].id = Some("a b".into()), "step id `a b`"),
    ];
    for (change, message) in refused {
        let mut template = template();
        change(&mut template);
        let error = template.validate().unwrap_err().message;
        assert!(error.contains(message), "{error:?} should mention {message:?}");
    }
    let mut long = template();
    long.steps = vec![long.steps[0].clone(); MAX_STEPS + 1];
    assert!(long.validate().is_err());
    long.steps.truncate(MAX_STEPS);
    long.validate().unwrap();
    let mut many = template();
    many.services = (0..=MAX_SERVICES).map(|i| Service { id: format!("s{i}"), ..many.services[0].clone() }).collect();
    assert!(many.validate().is_err());
}

#[test]
fn spec_validation() {
    let refused: [Refusal<JobSpec>; 13] = [
        (|s| s.version = 0, "version 0"),
        (|s| s.template.version = 0, "job template version"),
        (|s| _ = s.secrets.insert("GITHUB_TOKEN".into(), Secret::new("x")), "`GITHUB_TOKEN` is reserved"),
        (|s| _ = s.secrets.insert("lower".into(), Secret::new("x")), "`lower` is malformed"),
        (|s| _ = s.secrets.insert("BIG".into(), Secret::new("x".repeat(MAX_SECRET_BYTES + 1))), "`BIG` is larger"),
        (|s| _ = s.contexts.vars.insert("RBIT_X".into(), String::new()), "variable name `RBIT_X` is reserved"),
        (|s| s.token = Secret::new("rbit_notatasktoken"), "task token"),
        (|s| _ = s.contexts.github.insert("Token".into(), json!("x")), "must not carry the token"),
        (|s| s.contexts.needs.clear(), "every needed job"),
        (|s| s.checkout.url = "https://user:pw@git.example.com/alice/app".into(), "credentials"),
        (|s| s.checkout.sha = SHA.to_uppercase(), "SHA"),
        (|s| s.checkout.reference = "main".into(), "full ref"),
        (|s| s.cache.write = "../other".into(), "cache scope"),
    ];
    for (change, message) in refused {
        let mut spec = spec();
        change(&mut spec);
        let error = spec.validate().unwrap_err().message;
        assert!(error.contains(message), "{error:?} should mention {message:?}");
    }
    let mut sha256 = spec();
    sha256.checkout.sha = "ab".repeat(32);
    sha256.validate().unwrap();

    // Actions: reusable workflows. A job after a call needs its jobs (`call/build`) and
    // reads them as `needs.call`.
    let mut after_call = spec();
    let needed = after_call.contexts.needs.remove("build").unwrap();
    after_call.contexts.needs.insert("call".into(), needed);
    after_call.template.needs = vec!["call/build".into()];
    after_call.template.need_names = BTreeMap::from([(
        "call".into(),
        NeedSource::Call { keys: vec!["call/build".into()], outputs: BTreeMap::new(), jobs: BTreeMap::new() },
    )]);
    after_call.validate().unwrap();
    after_call.contexts.needs.clear();
    assert!(after_call.validate().unwrap_err().message.contains("every needed job"));
}

#[test]
fn secrets_never_appear_in_debug_output() {
    let debug = format!("{:?} {:?}", spec(), PollResponse { task: spec() });
    assert!(!debug.contains("s3cr3t-registry") && !debug.contains(TOKEN), "{debug}");
    let credentials = RegistryCredentials { username: "ci".into(), password: "literal-password".into() };
    assert!(!format!("{credentials:?}").contains("literal-password"));
}

#[test]
fn permissions_convert_from_the_model() {
    let model = Permissions { contents: Permission::Write, issues: Permission::Read, pull_requests: Permission::None };
    let converted = TokenPermissions::from(model);
    assert_eq!(converted, TokenPermissions { contents: Access::Write, issues: Access::Read, pull_requests: Access::None });
    assert_eq!(serde_json::to_string(&converted).unwrap(), r#"{"contents":"write","issues":"read","pull_requests":"none"}"#);
    assert_eq!(Strategy::SINGLE.validate(), Ok(()));
}
