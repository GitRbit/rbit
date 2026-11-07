//! Original parser steps fixtures; assertions and source samples retained.
use super::support::{assert_errors, job_err, parse_ok};
use crate::error::ErrorKind;
use crate::model::{Builtin, Shell, StepAction};
use crate::spec::{BuiltinKind, MAX_SERVICES, MAX_STEPS};

// Actions: third-party actions.
#[test]
fn other_actions_are_steps_the_runner_fetches() {
    let workflow = parse_ok(
        "on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n      - uses: actions/setup-node@v4\n        with:\n          node-version: 22\n          cache: ${{ matrix.cache }}\n      - uses: ./.github/actions/build\n      - uses: docker://alpine:3.20\n      - uses: actions/checkout@0123456789abcdef0123456789abcdef01234567\n",
    );
    let steps = &workflow.jobs[0].steps;
    let StepAction::Action(node) = &steps[0].action else { panic!("{:?}", steps[0].action) };
    assert_eq!(node.uses, "actions/setup-node@v4");
    assert_eq!(
        node.source,
        crate::spec::ActionSource::Repository {
            owner: "actions".into(),
            repo: "setup-node".into(),
            path: String::new(),
            reference: "v4".into()
        }
    );
    assert_eq!((node.with["node-version"].as_str(), node.with["cache"].as_str()), ("22", "${{ matrix.cache }}"));
    assert!(
        matches!(&steps[1].action, StepAction::Action(local) if local.source == crate::spec::ActionSource::Local { path: ".github/actions/build".into() })
    );
    assert!(
        matches!(&steps[2].action, StepAction::Action(docker) if docker.source == crate::spec::ActionSource::Docker { image: "alpine:3.20".into() })
    );
    assert!(matches!(&steps[3].action, StepAction::Uses(builtin) if builtin.kind == BuiltinKind::Checkout), "a pinned built-in");
}

#[test]
fn steps_do_exactly_one_thing() {
    let cases: &[(&str, &str, (u32, u32))] = &[
        ("    steps:\n      - run: x\n        uses: y\n", "has both `run` and `uses`", (7, 9)),
        ("    steps:\n      - name: nothing\n", "needs `run` or `uses`", (6, 9)),
        ("    steps:\n      - run: x\n        with: {a: b}\n", "steps[0].with` only applies to `uses` steps", (7, 9)),
        ("    steps:\n      - uses: y\n        shell: bash\n", "steps[0].shell` only applies to `run` steps", (7, 9)),
        ("    steps:\n      - uses: y\n        working-directory: d\n", "steps[0].working-directory` only applies", (7, 9)),
        ("    steps:\n      - run: x\n      -\n", "`jobs.a.steps[1]` must be a step", (7, 8)),
        ("    steps:\n      - run: x\n      - x\n", "`jobs.a.steps[1]` must be a step", (7, 9)),
        ("    steps: []\n", "at least one step", (5, 12)),
        ("    steps: x\n", "must be a list of steps", (5, 12)),
        ("    steps:\n      - {id: a, run: x}\n      - {id: a, run: y}\n", "step id `a` is used twice", (7, 9)),
        ("    steps:\n      - {id: 1a, run: x}\n", "`1a` must start with a letter", (6, 14)),
        ("    steps:\n      - {run: x, with: {}}\n", "steps[0].with` only applies", (6, 18)),
        ("    steps:\n      - {uses: rbit/checkout@v1, with: {ref: [1]}}\n", "`jobs.a.steps[0].with.ref` must be a string", (6, 46)),
        (
            "    steps:\n      - {uses: 'octo/ci/.github/workflows/x.yml@v1'}\n",
            "`jobs.a.steps[0].uses`: `octo/ci/.github/workflows/x.yml@v1` is a workflow: call it from a job",
            (6, 16),
        ),
        ("    steps:\n      - {uses: 'octo/repo@--upload-pack=x'}\n", "is not a branch, tag or commit", (6, 16)),
        ("    steps:\n      - {uses: actions/setup-node@v4, with: {'a b': x}}\n", "`a b` is not an input name", (6, 46)),
        (
            "    steps:\n      - {uses: actions/checkout@v4, with: {ssh-key: x}}\n",
            "`jobs.a.steps[0].with.ssh-key`: `actions/checkout@v4` has no input `ssh-key`",
            (6, 44),
        ),
        ("    steps:\n      - {uses: actions/checkout@v4, with: {lfs: maybe}}\n", "`lfs` must be true or false", (6, 44)),
        (
            "    steps:\n      - {uses: actions/cache@v4, with: {path: x}}\n",
            "`jobs.a.steps[0]`: `actions/cache@v4` needs `with.key`",
            (6, 16),
        ),
        ("    steps:\n      - {run: x, env: {A: {b: c}}}\n", "`jobs.a.steps[0].env.A` must be a string", (6, 27)),
        ("    steps:\n      - {run: x, continue-on-error: maybe}\n", "true, false or an expression", (6, 37)),
        ("    steps:\n      - {run: x, continue-on-error: 'x ${{ true }}'}\n", "true, false or an expression", (6, 37)),
        ("    steps:\n      - {run: x, timeout-minutes: 0}\n", "a positive number or an expression", (6, 35)),
        ("    steps:\n      - {run: x, timeout-minutes: soon}\n", "a positive number or an expression", (6, 35)),
        ("    steps:\n      - {run: x, shell: fish}\n", "`fish` is not a shell rbit supports", (6, 25)),
        ("    steps:\n      - {run: x, shell: '{0} perl'}\n", "a custom shell starts with the program", (6, 25)),
    ];
    assert_errors(cases.iter().map(|(steps, message, at)| (*steps, job_err(&format!("    runs-on: x\n{steps}")), *message, *at)));
    let steps = (0..=MAX_STEPS).map(|index| format!("      - run: echo {index}\n")).collect::<Vec<_>>().concat();
    let error = job_err(&format!("    runs-on: x\n    steps:\n{steps}"));
    assert_eq!(error.kind, ErrorKind::Limit, "{error}");
    let steps = (0..MAX_STEPS).map(|index| format!("      - run: echo {index}\n")).collect::<Vec<_>>().concat();
    parse_ok(&format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps:\n{steps}"));
    assert!(job_err("    runs-on: x\n").message.contains("`jobs.a` needs `steps`"));
    let services = (0..=MAX_SERVICES).map(|index| format!("      s{index}: {{image: x}}\n")).collect::<Vec<_>>().concat();
    assert_eq!(job_err(&format!("    runs-on: x\n    services:\n{services}    steps: [{{run: x}}]\n")).kind, ErrorKind::Limit);
}

#[test]
fn shells_and_amounts() {
    for shell in Shell::ALL {
        let source = format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{{run: x, shell: {}}}]\n", shell.name());
        assert_eq!(parse_ok(&source).jobs[0].steps[0].shell.as_deref(), Some(shell.name()));
        assert!(shell.command().contains("{0}") && shell.extension().starts_with('.'));
    }
    assert_eq!(Shell::Bash.command(), "bash --noprofile --norc -eo pipefail {0}");
    assert_eq!(Shell::Sh.command(), "sh -e {0}");
    let step = |step: &str| parse_ok(&format!("on: push\njobs:\n  a:\n    runs-on: x\n    steps: [{step}]\n")).jobs[0].steps[0].clone();
    assert_eq!(step("{run: x, shell: '${{ matrix.shell }}'}").shell.as_deref(), Some("${{ matrix.shell }}"));
    assert_eq!(step("{run: x, timeout-minutes: 1.5}").timeout_minutes.as_deref(), Some("1.5"));
    assert_eq!(step("{run: x, continue-on-error: false}").continue_on_error.as_deref(), Some("false"));
    assert_eq!(step("{run: x, if: false}").condition.as_deref(), Some("false"));
    assert_eq!(step("{run: x, if: \"contains(github.ref, 'x')\"}").condition.as_deref(), Some("contains(github.ref, 'x')"));
    assert!(matches!(
        step("{uses: rbit/download-artifact@v1}").action,
        StepAction::Uses(Builtin { kind: BuiltinKind::DownloadArtifact, .. })
    ));
    let job = |job: &str| job_err(&format!("    runs-on: x\n{job}    steps: [{{run: x}}]\n"));
    assert!(job("    timeout-minutes: -1\n").message.contains("a positive number"));
    assert!(job("    strategy: {max-parallel: 1.5}\n").message.contains("a positive whole number"));
    assert!(job("    strategy: {max-parallel: 0}\n").message.contains("a positive whole number"));
    assert!(job("    continue-on-error: yes\n").message.contains("true, false or an expression"));
    let source = "on: push\njobs:\n  a:\n    runs-on: x\n    timeout-minutes: ${{ inputs.minutes }}\n    strategy: {max-parallel: '${{ vars.N }}', fail-fast: '${{ inputs.fast }}'}\n    steps: [{run: x}]\n";
    let job = parse_ok(source).jobs.remove(0);
    assert_eq!(job.timeout_minutes.as_deref(), Some("${{ inputs.minutes }}"));
    assert_eq!(
        (job.strategy.max_parallel.as_deref(), job.strategy.fail_fast.as_deref()),
        (Some("${{ vars.N }}"), Some("${{ inputs.fast }}"))
    );
}
