//! Original planner names fixtures; behavior assertions retained.
use super::super::group_context::GroupContext;
use super::super::names::{concurrency_group, run_name};
use super::support::{run_context, workflow};

#[test]
fn run_names() {
    let named = |text: &str| {
        run_name(&workflow(&format!("on: push\nrun-name: {text}\njobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n")), &run_context())
    };
    assert_eq!(
        named("'Deploy ${{ inputs.target }} from ${{ github.ref }}'").unwrap().as_deref(),
        Some("Deploy arm64 from refs/heads/main")
    );
    assert_eq!(named("'${{ inputs.missing }}'").unwrap(), None);
    assert_eq!(run_name(&workflow("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n"), &run_context()).unwrap(), None);
}

#[test]
fn concurrency_groups() {
    let context = GroupContext {
        workflow: "CI".into(),
        workflow_path: ".rbit/workflows/ci.yml".into(),
        reference: "refs/pull/7/head".into(),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        event_name: "pull_request".into(),
        repository: "alice/rocket".into(),
        run_id: 31,
        pull_request_number: Some(7),
        sha: "0123456789abcdef0123456789abcdef01234567".into(),
        actor: "bob".into(),
        triggering_actor: "bob".into(),
        repository_owner: "alice".into(),
        repository_id: 4,
        server_url: "https://git.example.com".into(),
        ..GroupContext::default()
    };
    let group = |concurrency: &str| {
        let source = format!("on: push\nconcurrency: {concurrency}\njobs: {{a: {{runs-on: x, steps: [{{run: x}}]}}}}\n");
        concurrency_group(&workflow(&source), &context.run_context())
    };
    assert_eq!(group("ci").unwrap(), Some(("ci".into(), false)));
    assert_eq!(group("'${{ github.workflow }}-${{ github.ref }}'").unwrap(), Some(("CI-refs/pull/7/head".into(), false)));
    assert_eq!(
        group("{group: '${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}', cancel-in-progress: true}").unwrap(),
        Some(("CI-7".into(), true))
    );
    assert_eq!(group("'${{ github.head_ref || github.run_id }}'").unwrap(), Some(("feature".into(), false)));
    assert_eq!(
        group("'${{ github.repository }}/${{ github.event_name }}/${{ github.base_ref }}'").unwrap().unwrap().0,
        "alice/rocket/pull_request/main"
    );
    assert_eq!(group("'${{ github.actor }}-${{ github.sha }}'").unwrap().unwrap().0, "bob-0123456789abcdef0123456789abcdef01234567");
    assert_eq!(group("'${{ github.ref_name }}@${{ github.server_url }}'").unwrap().unwrap().0, "7/head@https://git.example.com");
    assert!(group("'${{ github.run_number }}'").is_err(), "unknown before the run exists, so the group is empty");
    assert!(group("'${{ github.base_ref && null }}'").is_err(), "an empty group is refused");
    let push = GroupContext { pull_request_number: None, head_ref: String::new(), ..context };
    let source =
        "on: push\nconcurrency: '${{ github.event.pull_request.number || github.run_id }}'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n";
    assert_eq!(concurrency_group(&workflow(source), &push.run_context()).unwrap(), Some(("31".into(), false)));
    assert_eq!(concurrency_group(&workflow("on: push\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n"), &push.run_context()).unwrap(), None);
    let source = "on: push\nconcurrency: '${{ vars.RUNNER }}-${{ inputs.target }}'\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n";
    assert_eq!(concurrency_group(&workflow(source), &run_context()).unwrap(), Some(("big-arm64".into(), false)));
    // rbit's own CI: only pull requests cancel their runs in progress.
    let source = "on: [push, pull_request]\nconcurrency:\n  group: ci\n  cancel-in-progress: ${{ github.event_name == 'pull_request' }}\njobs: {a: {runs-on: x, steps: [{run: x}]}}\n";
    let pull = GroupContext { event_name: "pull_request".into(), ..GroupContext::default() };
    assert_eq!(concurrency_group(&workflow(source), &pull.run_context()).unwrap(), Some(("ci".into(), true)));
    let pushed = GroupContext { event_name: "push".into(), ..push };
    assert_eq!(concurrency_group(&workflow(source), &pushed.run_context()).unwrap(), Some(("ci".into(), false)));
}
