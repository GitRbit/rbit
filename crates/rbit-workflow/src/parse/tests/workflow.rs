//! Original parser workflow fixtures; assertions and source samples retained.
use super::super::parse;
use super::support::{CI, EVERY_KEY, parse_ok};
use crate::error::Location;
use crate::limits::{Deadline, Limits};
use crate::model::{
    ActivityType, Builtin, Concurrency, DispatchInput, Env, Filter, InputKind, Permission, Permissions, RegistryCredentials, RunDefaults,
    StepAction,
};
use crate::spec::BuiltinKind;
use std::collections::BTreeMap;

#[test]
fn a_complete_workflow() {
    let workflow = parse_ok(CI);
    assert_eq!(workflow.name.as_deref(), Some("CI"));
    assert_eq!(workflow.run_name.as_deref(), Some("Build ${{ github.ref }}"));
    let push = workflow.on.push.as_ref().unwrap();
    assert!(matches!(&push.branches, Filter::Include(patterns) if patterns.iter().count() == 2));
    assert!(push.tags.is_absent());
    assert!(matches!(&push.paths, Filter::Ignore(_)));
    let pull = workflow.on.pull_request.as_ref().unwrap();
    assert_eq!(pull.types.as_deref(), Some(&[ActivityType::Opened, ActivityType::Synchronize, ActivityType::ReadyForReview][..]));
    let dispatch = workflow.on.workflow_dispatch.as_ref().unwrap();
    assert_eq!(
        dispatch.inputs,
        [
            DispatchInput {
                name: "level".into(),
                description: None,
                required: true,
                default: Some("low".into()),
                kind: InputKind::Choice(vec!["low".into(), "high".into()]),
            },
            DispatchInput { name: "dry-run".into(), description: None, required: false, default: None, kind: InputKind::Boolean },
        ]
    );
    let crons: Vec<_> = workflow.on.schedule.iter().map(|entry| entry.cron.as_str()).collect();
    assert_eq!(crons, ["0 3 * * 1", "*/30 * * * *"]);
    assert_eq!(workflow.on.schedule[0].at.line, 20);
    assert_eq!(
        workflow.permissions,
        Some(Permissions { contents: Permission::Write, issues: Permission::None, pull_requests: Permission::Read })
    );
    assert_eq!(
        workflow.concurrency,
        Some(Concurrency { group: "ci-${{ github.ref }}".into(), cancel_in_progress: "true".into(), at: Location::new(26, 3) })
    );
    let ids: Vec<_> = workflow.jobs.iter().map(|job| job.id.as_str()).collect();
    assert_eq!(ids, ["build", "deploy"]);
    let build = workflow.job("build").unwrap();
    assert_eq!(build.name.as_deref(), Some("Build ${{ matrix.os }}"));
    assert_eq!(build.runs_on, ["${{ matrix.os }}"]);
    let matrix = build.strategy.matrix.as_ref().unwrap();
    assert_eq!(
        matrix.value,
        serde_json::json!({
            "os": ["ubuntu-latest", "debian-12"],
            "rust": ["stable", 1.94],
            "include": [{"os": "debian-12", "experimental": true}],
            "exclude": [{"os": "debian-12", "rust": 1.94}],
        })
    );
    assert!(!matrix.has_expressions);
    assert_eq!(matrix.at, Location::new(34, 9));
    let deploy = workflow.job("deploy").unwrap();
    assert_eq!(deploy.needs, ["build"]);
    assert_eq!(deploy.runs_on, ["self-hosted", "linux"]);
    assert_eq!(deploy.permissions, Some(Permissions::uniform(Permission::Read)));
}

#[test]
fn short_forms_of_on() {
    let workflow = parse_ok("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
    assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_none());
    let workflow = parse_ok("on: [push, pull_request, issues, workflow_dispatch]\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
    assert!(workflow.on.push.is_some() && workflow.on.pull_request.is_some() && workflow.on.workflow_dispatch.is_some());
    assert_eq!(workflow.on.other, ["issues"]);
    let workflow = parse_ok("'on':\n  push:\n  pull_request: {}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n");
    assert!(workflow.on.push.is_some() && workflow.on.pull_request.as_ref().unwrap().types.is_none());
}

#[test]
fn permissions_forms() {
    let job = |permissions: &str| {
        let source = format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x}}]\n    permissions: {permissions}\n");
        parse(source.as_bytes(), &Limits::default(), &Deadline::NONE).map(|workflow| workflow.jobs[0].permissions)
    };
    assert_eq!(job("read-all").unwrap(), Some(Permissions::uniform(Permission::Read)));
    assert_eq!(job("write-all").unwrap(), Some(Permissions::uniform(Permission::Write)));
    assert_eq!(job("{}").unwrap(), Some(Permissions::uniform(Permission::None)));
    assert_eq!(
        job("{issues: write, id-token: write, packages: read}").unwrap(),
        Some(Permissions { issues: Permission::Write, ..Permissions::uniform(Permission::None) })
    );
    assert_eq!(job("{contents: none}").unwrap(), Some(Permissions::uniform(Permission::None)));
    assert!(job("admin").is_err());
    assert!(job("{contents: admin}").is_err());
    assert!(job("[read]").is_err());
}

#[test]
fn every_key_is_typed() {
    let workflow = parse_ok(EVERY_KEY);
    assert_eq!(
        workflow.env,
        Env::from([("CARGO_TERM_COLOR".into(), "always".into()), ("LEVEL".into(), "3".into()), ("EMPTY".into(), String::new())])
    );
    assert_eq!(workflow.defaults, RunDefaults { shell: Some("bash".into()), working_directory: Some("crates".into()) });
    let test = workflow.job("test").unwrap();
    assert_eq!(test.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
    assert_eq!(test.env, Env::from([("RUST".into(), "${{ matrix.rust }}".into())]));
    assert_eq!(test.defaults, RunDefaults { shell: Some("pwsh".into()), working_directory: None });
    assert_eq!(test.outputs, BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]));
    assert_eq!(test.timeout_minutes.as_deref(), Some("30"));
    assert_eq!(test.continue_on_error.as_deref(), Some("${{ matrix.experimental }}"));
    let concurrency = test.concurrency.as_ref().unwrap();
    assert_eq!((concurrency.group.as_str(), concurrency.cancel_in_progress.as_str()), ("test-${{ github.ref }}-${{ matrix.os }}", "true"));
    assert_eq!((test.strategy.fail_fast.as_deref(), test.strategy.max_parallel.as_deref()), (Some("false"), Some("2")));
    assert_eq!(test.strategy.matrix.as_ref().map(|matrix| matrix.value.clone()), Some(serde_json::json!({"os": ["linux", "windows"]})));
    let container = test.container.as_ref().unwrap();
    assert_eq!(container.image, "docker.io/library/rust:${{ matrix.rust || '1.94' }}");
    assert_eq!(container.env, Env::from([("CARGO_HOME".into(), "/cargo".into())]));
    assert_eq!(
        container.credentials,
        Some(RegistryCredentials { username: "ci".into(), password: "${{ secrets.REGISTRY_PASSWORD }}".into() })
    );
    let services: Vec<_> = test.services.iter().map(|service| (service.id.as_str(), service.image.as_str(), service.env.len())).collect();
    assert_eq!(services, [("postgres", "postgres:17", 1), ("redis", "redis:7", 0)]);
    assert_eq!(test.steps.len(), 2);
    assert_eq!(
        test.steps[0].action,
        StepAction::Uses(Builtin {
            uses: "actions/checkout@v4".into(),
            kind: BuiltinKind::Checkout,
            version: 1,
            with: BTreeMap::from([("fetch-depth".into(), "0".into()), ("lfs".into(), "true".into())])
        })
    );
    let meta = &test.steps[1];
    assert_eq!(
        (meta.id.as_deref(), meta.name.as_deref(), meta.condition.as_deref()),
        (Some("meta"), Some("Meta on ${{ runner.os }}"), Some("${{ always() }}"))
    );
    assert_eq!(meta.action, StepAction::Run("echo \"version=1\" >> \"$GITHUB_OUTPUT\"".into()));
    assert_eq!((meta.shell.as_deref(), meta.working_directory.as_deref()), (Some("sh"), Some("${{ github.workspace }}/x")));
    assert_eq!(meta.env, Env::from([("TOKEN".into(), "${{ secrets.GITHUB_TOKEN }}".into())]));
    assert_eq!((meta.continue_on_error.as_deref(), meta.timeout_minutes.as_deref()), (Some("true"), Some("${{ matrix.minutes }}")));
    let lint = workflow.job("lint").unwrap();
    assert_eq!(lint.container.as_ref().map(|container| container.image.as_str()), Some("alpine:3"));
    assert_eq!((lint.timeout_minutes.as_ref(), lint.continue_on_error.as_ref(), lint.condition.as_ref()), (None, None, None));
    assert!(lint.services.is_empty() && lint.outputs.is_empty() && lint.strategy.matrix.is_none());
}
