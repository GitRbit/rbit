//! The hostile load regression group for workflow files: YAML alias expansion (billion
//! laughs), nesting depth, expression nesting and matrix expansion pushed on several refs
//! at once are each refused as a workflow error within the evaluation budget, on the
//! bounded evaluation pool, while an ordinary repository read is served meanwhile.

use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

use rbit_core::{PushSource, actions::Status};

use crate::actions_fixture::{Fixture, workflow};

/// Pushes evaluated at once; each carries every hostile file below.
const PUSHES: usize = 6;
/// Generous for debug builds; the evaluation budget itself is ten seconds.
const BOUND: Duration = Duration::from_secs(60);

/// Eleven levels of ten aliases each: 10^11 nodes if expanded.
fn billion_laughs() -> String {
    let mut text = String::from("on: push\na: &a [x, x, x, x, x, x, x, x, x, x]\n");
    for level in 1..12u8 {
        let (previous, name) = (char::from(b'a' + level - 1), char::from(b'a' + level));
        writeln!(text, "{name}: &{name} [{}]", vec![format!("*{previous}"); 10].join(", ")).unwrap();
    }
    text.push_str("jobs:\n  build:\n    runs-on: x\n    steps: [{run: x}]\n");
    text
}

/// Aliases replayed through a merge key at every level of a job.
fn merge_expansion() -> String {
    let mut text = String::from("on: push\nx0: &x0 {a: 1, b: 2, c: 3, d: 4, e: 5, f: 6, g: 7, h: 8}\n");
    for level in 1..10 {
        let previous = level - 1;
        writeln!(text, "x{level}: &x{level} {{a: *x{previous}, b: *x{previous}, c: *x{previous}, d: *x{previous}, e: *x{previous}}}")
            .unwrap();
    }
    text.push_str("jobs:\n  build:\n    runs-on: x\n    env: *x9\n    steps: [{run: x}]\n");
    text
}

fn deep_nesting() -> String {
    let depth = 10_000;
    format!("on: push\njobs: {}1{}\n", "[".repeat(depth), "]".repeat(depth))
}

fn expression_nesting() -> String {
    let depth = 2_000;
    format!(
        "on: push\njobs:\n  build:\n    runs-on: x\n    if: ${{{{ {}true{} }}}}\n    steps: [{{run: x}}]\n",
        "(".repeat(depth),
        ")".repeat(depth)
    )
}

/// Ten axes of ten values: 10^10 combinations if enumerated.
fn matrix_expansion() -> String {
    let axes = (0..10).fold(String::new(), |mut axes, axis| {
        writeln!(axes, "        k{axis}: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]").unwrap();
        axes
    });
    format!("on: push\njobs:\n  build:\n    runs-on: x\n    strategy:\n      matrix:\n{axes}    steps: [{{run: x}}]\n")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hostile_workflows_pushed_at_once_are_refused_within_budget_while_reads_continue() {
    let fixture = Fixture::with(|config| config.actions.evaluation_concurrency = 2).await;
    let hostile = [
        ("a-laughs", billion_laughs()),
        ("b-merge", merge_expansion()),
        ("c-deep", deep_nesting()),
        ("d-expression", expression_nesting()),
        ("e-matrix", matrix_expansion()),
    ];
    let mut files: Vec<(String, String)> =
        hostile.iter().map(|(name, text)| (format!(".forgejo/workflows/{name}.yml"), text.clone())).collect();
    files.push((".forgejo/workflows/z-good.yml".to_owned(), workflow("Good", "push")));
    files.push(("README.md".to_owned(), "An ordinary read.\n".to_owned()));
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(path, text)| (path.as_str(), text.as_str())).collect();
    fixture.commit(&borrowed, "Hostile workflows");
    let mut updates = Vec::new();
    for push in 0..PUSHES {
        updates.push(fixture.push_ref(&format!("refs/heads/hostile-{push}")).await);
    }
    let repo = fixture.repo(&fixture.alice).await;
    let (id, source) = (repo.repo.repo.id, PushSource::User(fixture.alice.id()));

    let started = Instant::now();
    let evaluations: Vec<_> = updates
        .into_iter()
        .map(|update| {
            let actions = fixture.actions().clone();
            tokio::spawn(async move {
                let runs = actions.handle_push(id, source, &[update]).await.unwrap();
                (runs, Instant::now())
            })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let read_started = Instant::now();
    let readme = repo.reader.blob("hostile-0".to_owned(), "README.md".to_owned(), 1024).await.unwrap();
    let read_finished = Instant::now();
    assert_eq!(readme.text(), Some("An ordinary read.\n"));
    assert!(read_finished - read_started < BOUND);

    let mut last = started;
    for evaluation in evaluations {
        let (runs, finished) = tokio::time::timeout(BOUND, evaluation).await.expect("evaluation is bounded").unwrap();
        last = last.max(finished);
        let summary: Vec<_> = runs.iter().map(|run| (run.workflow_path.rsplit('/').next().unwrap(), run.status)).collect();
        assert_eq!(
            summary,
            [
                ("a-laughs.yml", Status::Failure),
                ("b-merge.yml", Status::Failure),
                ("c-deep.yml", Status::Failure),
                ("d-expression.yml", Status::Failure),
                ("e-matrix.yml", Status::Failure),
                ("z-good.yml", Status::Waiting),
            ],
            "{:?}",
            runs.iter().map(|run| run.error.clone()).collect::<Vec<_>>()
        );
        for run in &runs[..hostile.len()] {
            let error = run.error.as_deref().unwrap();
            assert!(!error.contains("took too long"), "refused by a limit, not by the deadline: {error}");
        }
    }
    assert!(last - started < BOUND, "all evaluations finished in {:?}", last - started);
    assert!(read_finished < last, "the ordinary read was served while evaluations were running");
    assert_eq!(fixture.actions().evaluations().available_permits(), 2, "every evaluation permit was returned");
}
