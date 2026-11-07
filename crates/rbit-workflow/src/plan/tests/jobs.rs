//! Original planner jobs fixtures; behavior assertions retained.
use super::super::planning::plan;
use super::support::{EVERY_FIELD, planned, run_context, workflow};
use crate::limits::{Deadline, Limits};
use crate::spec::{
    BuiltinKind, ConcurrencyGroup, DEFAULT_TIMEOUT_MINUTES, JOB_TEMPLATE_VERSION, RunDefaults, StepAction, StepTemplate, Strategy,
};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn names_and_labels() {
    let source = "on: push
jobs:
  plain:
    runs-on: linux
    steps: [{run: x}]
  named:
    name: Build ${{ matrix.os }} for ${{ inputs.target }} (${{ strategy.job-index }}/${{ strategy.job-total }})
    runs-on: [self-hosted, '${{ matrix.os }}-${{ matrix.arch || ''x64'' }}']
    steps: [{run: x}]
    strategy:
      matrix:
        os: [debian]
  listed:
    runs-on: ${{ matrix.labels }}
    steps: [{run: x}]
    strategy:
      matrix:
        labels: [[self-hosted, arm64]]
        n: [1.10]
  github:
    name: ${{ github.repository }}
    runs-on: ['${{ vars.RUNNER }}', '${{ github.ref == ''refs/heads/main'' && ''release'' || ''dev'' }}']
    steps: [{run: x}]
  empty-name:
    name: ${{ inputs.missing }}
    runs-on: x
    steps: [{run: x}]
";
    let jobs = planned(source);
    let summary: Vec<(&str, &str, Vec<&str>)> =
        jobs.iter().map(|job| (job.job_id.as_str(), job.name.as_str(), job.runs_on.iter().map(String::as_str).collect())).collect();
    assert_eq!(
        summary,
        [
            ("plain", "plain", vec!["linux"]),
            ("named", "Build debian for arm64 (0/1)", vec!["self-hosted", "debian-x64"]),
            ("listed", "listed ([self-hosted arm64], 1.1)", vec!["self-hosted", "arm64"]),
            ("github", "alice/app", vec!["big", "release"]),
            ("empty-name", "empty-name", vec!["x"]),
        ]
    );
    assert!(jobs[0].matrix.is_empty());
    assert_eq!(serde_json::to_string(&jobs[2].matrix).unwrap(), r#"{"labels":["self-hosted","arm64"],"n":1.1}"#);
}

#[test]
fn runs_on_refusals() {
    for runs_on in ["${{ matrix.missing }}", "${{ matrix.empty }}", "${{ matrix.nested }}", "${{ fromJSON('{}') }}", "two words"] {
        let source = format!(
            "on: push\njobs:\n  j:\n    steps: [{{run: x}}]\n    runs-on: \"{runs_on}\"\n    strategy: {{matrix: {{empty: [''], nested: [[{{a: 1}}]]}}}}\n"
        );
        let error = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Plan, "{runs_on}");
        assert!(error.message.starts_with("job `j`"), "{error}");
    }
}

// Actions: environments.
#[test]
fn environments_are_named_when_planned_and_their_url_on_the_runner() {
    let jobs = planned(
        "on: push
jobs:
  deploy:
    runs-on: x
    strategy: {matrix: {stage: [staging, production]}}
    environment:
      name: ${{ matrix.stage }}
      url: ${{ steps.deploy.outputs.url }}
    steps: [{run: x}]
  plain:
    runs-on: x
    environment: Production
    steps: [{run: x}]
",
    );
    let environments: Vec<_> = jobs.iter().map(|job| (job.environment.as_deref(), job.environment_url.as_deref())).collect();
    assert_eq!(
        environments,
        [
            (Some("production"), Some("${{ steps.deploy.outputs.url }}")),
            (Some("staging"), Some("${{ steps.deploy.outputs.url }}")),
            (Some("Production"), None)
        ]
    );
    for name in ["${{ inputs.missing }}", "a/b"] {
        let source = format!(
            "on: push
jobs:
  j:
    runs-on: x
    environment: '{name}'
    steps: [{{run: x}}]
"
        );
        let error = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_err();
        assert!(error.message.contains("environment"), "{name}: {error}");
    }
}

#[test]
fn every_field_is_filled() {
    let jobs = planned(EVERY_FIELD);
    assert_eq!(jobs.len(), 3);
    let build = &jobs[0];
    assert_eq!((build.timeout_minutes, build.continue_on_error, build.strategy), (DEFAULT_TIMEOUT_MINUTES, false, Strategy::SINGLE));
    assert_eq!(build.permissions, Some(crate::model::Permissions::uniform(crate::model::Permission::Read).into()));
    assert_eq!(build.defaults, RunDefaults { shell: Some("bash".into()), working_directory: Some("crates".into()) });
    let (linux, windows) = (&jobs[1], &jobs[2]);
    assert_eq!((linux.name.as_str(), windows.name.as_str()), ("Test linux", "Test windows"));
    assert_eq!(linux.version, JOB_TEMPLATE_VERSION);
    assert_eq!(linux.needs, ["build"]);
    assert_eq!(linux.condition.as_deref(), Some("github.ref == 'refs/heads/main'"));
    assert_eq!((linux.runs_on.as_slice(), windows.runs_on.as_slice()), (&["linux".to_owned()][..], &["windows".to_owned()][..]));
    assert_eq!(serde_json::Value::Object(linux.matrix.clone()), json!({"os": "linux"}));
    assert_eq!(linux.strategy, Strategy { fail_fast: false, max_parallel: Some(1), job_index: 0, job_total: 2 });
    assert_eq!(windows.strategy.job_index, 1);
    assert_eq!(serde_json::to_value(linux.permissions).unwrap(), json!({"contents": "write", "issues": "none", "pull_requests": "none"}));
    assert_eq!(linux.concurrency, Some(ConcurrencyGroup { group: "test-refs/heads/main-linux".into(), cancel_in_progress: true }));
    assert_eq!(linux.timeout_minutes, 45);
    assert_eq!((linux.continue_on_error, windows.continue_on_error), (false, true));
    assert_eq!(linux.workflow_env, crate::spec::Env::from([("CARGO_TERM_COLOR".into(), "always".into())]));
    assert_eq!(linux.env, crate::spec::Env::from([("RUST".into(), "${{ matrix.rust }}".into())]));
    assert_eq!(linux.defaults, RunDefaults { shell: Some("sh".into()), working_directory: Some("crates".into()) });
    assert_eq!(linux.outputs, BTreeMap::from([("version".into(), "${{ steps.meta.outputs.version }}".into())]));
    let container = linux.container.as_ref().unwrap();
    assert_eq!(container.image, "rust:linux");
    assert_eq!(container.credentials.as_ref().map(|credentials| credentials.password.as_str()), Some("${{ secrets.REGISTRY }}"));
    assert_eq!(linux.services.len(), 1);
    assert_eq!((linux.services[0].id.as_str(), linux.services[0].image.as_str()), ("db", "postgres:2"));
    assert_eq!(linux.services[0].env, crate::spec::Env::from([("POSTGRES_PASSWORD".into(), "${{ secrets.DB }}".into())]));
    assert!(matches!(&linux.steps[0].action, StepAction::Uses(builtin) if builtin.kind == BuiltinKind::Checkout));
    let meta = &linux.steps[1];
    assert_eq!(
        meta,
        &StepTemplate {
            id: Some("meta".into()),
            name: Some("Meta".into()),
            condition: Some("always()".into()),
            action: StepAction::Run("echo version=1 >> \"$GITHUB_OUTPUT\"".into()),
            shell: Some("bash".into()),
            working_directory: Some("x".into()),
            env: crate::spec::Env::from([("A".into(), "b".into())]),
            continue_on_error: Some("true".into()),
            timeout_minutes: Some("5".into()),
        }
    );
    for job in &jobs {
        job.validate().unwrap();
    }
}

#[test]
fn timeouts_flags_and_numbers() {
    let job = |settings: &str| {
        let source = format!("on: push\njobs:\n  j:\n    runs-on: x\n{settings}    steps: [{{run: x}}]\n");
        plan(&workflow(&source), &run_context(), &Limits { max_timeout_minutes: 120, ..Limits::default() }, &Deadline::NONE)
    };
    let timeout = |settings: &str| job(settings).map(|jobs| jobs[0].timeout_minutes);
    assert_eq!(timeout(""), Ok(120), "the default, capped");
    assert_eq!(timeout("    timeout-minutes: 30\n"), Ok(30));
    assert_eq!(timeout("    timeout-minutes: 2.5\n"), Ok(3), "rounded up");
    assert_eq!(timeout("    timeout-minutes: 600\n"), Ok(120), "capped");
    assert_eq!(timeout("    timeout-minutes: ${{ inputs.minutes }}\n"), Ok(45));
    assert_eq!(timeout("    timeout-minutes: \"${{ format('{0}', 7) }}\"\n"), Ok(7));
    assert_eq!(job("    timeout-minutes: ${{ inputs.target }}\n").unwrap_err().kind, crate::ErrorKind::Plan);
    assert_eq!(job("    timeout-minutes: ${{ inputs.missing }}\n").unwrap_err().kind, crate::ErrorKind::Plan);
    let default = plan(
        &workflow("on: push\njobs:\n  j:\n    runs-on: x\n    steps: [{run: x}]\n"),
        &run_context(),
        &Limits::default(),
        &Deadline::NONE,
    );
    assert_eq!(default.unwrap()[0].timeout_minutes, DEFAULT_TIMEOUT_MINUTES);
    let flag = |settings: &str| job(settings).map(|jobs| (jobs[0].continue_on_error, jobs[0].strategy.fail_fast));
    assert_eq!(flag(""), Ok((false, true)));
    assert_eq!(flag("    continue-on-error: true\n    strategy: {fail-fast: false}\n"), Ok((true, false)));
    assert_eq!(flag("    continue-on-error: ${{ inputs.target == 'arm64' }}\n"), Ok((true, true)));
    assert_eq!(flag("    continue-on-error: \"${{ 'TRUE' }}\"\n"), Ok((true, true)));
    assert!(job("    continue-on-error: ${{ inputs.target }}\n").unwrap_err().message.contains("must be true or false, not `arm64`"));
    let parallel = |settings: &str| job(settings).map(|jobs| jobs[0].strategy.max_parallel);
    assert_eq!(parallel(""), Ok(None));
    assert_eq!(parallel("    strategy: {max-parallel: 3}\n"), Ok(Some(3)));
    assert!(job("    strategy: {max-parallel: '${{ fromJSON(''2.5'') }}'}\n").unwrap_err().message.contains("whole number"));
    assert!(job("    concurrency: '${{ inputs.missing }}'\n").unwrap_err().message.contains("empty group"));
    assert!(job("    container: '${{ inputs.missing }}'\n").unwrap_err().message.contains("is not an image name"));
}
