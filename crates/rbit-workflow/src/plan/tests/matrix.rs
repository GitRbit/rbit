//! Original planner matrix fixtures; behavior assertions retained.
use super::super::matrix::{Axes, combinations, go_float, go_format};
use super::super::planning::plan;
use super::support::{matrix_of, planned, run_context, workflow};
use crate::limits::{Deadline, Limits};
use serde_json::json;

#[test]
fn matrix_expansion_table() {
    let cases: &[(&str, &[&str])] = &[
        ("        os: [a, b]", &["os=a", "os=b"]),
        ("        os: [b, a]\n        v: [2, 1]", &["os=a,v=1", "os=a,v=2", "os=b,v=1", "os=b,v=2"]),
        ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a, v: 2}", &["os=a,v=1", "os=b,v=1", "os=b,v=2"]),
        ("        os: [a, b]\n        v: [1, 2]\n        exclude:\n          - {os: a}", &["os=b,v=1", "os=b,v=2"]),
        // An include that fits merges into every combination it fits.
        ("        os: [a, b]\n        include:\n          - {os: a, extra: x}", &["os=b", "extra=x,os=a"]),
        ("        os: [a, b]\n        include:\n          - {extra: x}", &["extra=x,os=a", "extra=x,os=b"]),
        // An include that contradicts every original value stands alone.
        ("        os: [a, b]\n        include:\n          - {os: c, extra: x}", &["os=a", "os=b", "extra=x,os=c"]),
        // Added values may be overwritten by a later include; original ones never.
        ("        os: [a]\n        include:\n          - {extra: x}\n          - {extra: y}", &["extra=y,os=a"]),
        // GitHub's documented example.
        (
            "        fruit: [apple, pear]\n        animal: [cat, dog]\n        include:\n          - color: green\n          - color: pink\n            animal: cat\n          - fruit: apple\n            shape: circle\n          - fruit: banana\n          - fruit: banana\n            animal: cat",
            &[
                "fruit=banana",
                "animal=cat,fruit=banana",
                "animal=cat,color=pink,fruit=apple,shape=circle",
                "animal=cat,color=pink,fruit=pear",
                "animal=dog,color=green,fruit=apple,shape=circle",
                "animal=dog,color=green,fruit=pear",
            ],
        ),
        // Only includes: each is a combination.
        ("        include:\n          - {os: a}\n          - {os: b, v: 1}", &["os=a", "os=b,v=1"]),
        // An empty dimension leaves nothing but the includes, or nothing at all (which
        // planning refuses: see `an_empty_matrix_is_an_error`).
        ("        os: []", &[]),
        ("        os: [a, b]\n        exclude:\n          - {os: a}\n          - {os: b}", &[]),
        ("        os: []\n        include:\n          - {os: z}", &["os=z"]),
        // Values compare as data: 1.10 equals 1.1, 1 does not equal '1'.
        ("        v: [1.10, 2]\n        exclude:\n          - {v: 1.1}", &["v=2"]),
        ("        v: [1, 2]\n        exclude:\n          - {v: '1'}", &["v=1", "v=2"]),
        ("        cfg: [{a: 1}, [x, y]]", &["cfg=[x y]", "cfg=map[a:1]"]),
        // Expressions over the run's contexts, anywhere in the matrix.
        ("        os: ${{ fromJSON(vars.OSES) }}", &["os=alpine", "os=debian"]),
        ("        os: ['${{ inputs.target }}', '${{ vars.RUNNER }}-1']", &["os=arm64", "os=big-1"]),
        ("        os: [a]\n        include: '${{ fromJSON(''[{\"os\": \"b\"}]'') }}'", &["os=a", "os=b"]),
    ];
    for (matrix, expected) in cases {
        assert_eq!(matrix_of(matrix), *expected, "{matrix}");
    }
    let whole = "on: push\njobs:\n  j:\n    runs-on: x\n    strategy: {matrix: '${{ fromJSON(inputs.matrix) }}'}\n    steps: [{run: x}]\n";
    let names: Vec<_> = planned(whole).into_iter().map(|job| job.name).collect();
    assert_eq!(names, ["j (x)", "j (y)"]);
}

#[test]
fn evaluated_matrices_are_checked() {
    let cases = [
        ("'${{ inputs.target }}'", "must be a mapping of lists"),
        ("{os: '${{ inputs.target }}'}", "`os` must be a list"),
        ("{os: [a], include: '${{ fromJSON(''[1]'') }}'}", "`include` must be a list of mappings"),
        ("{os: [a], exclude: '${{ fromJSON(''[{\"v\": 1}]'') }}'}", "`v` is not a matrix dimension"),
        ("{os: '${{ fromJSON(''[]'') }}', include: [{os: z}]}", ""),
        ("{os: '${{ fromJSON(''[]'') }}', include: []}", "`os` has no values"),
        ("'${{ fromJSON(inputs.target) }}'", "fromJSON"),
    ];
    for (matrix, message) in cases {
        let source = format!("on: push\njobs:\n  j:\n    runs-on: x\n    strategy: {{matrix: {matrix}}}\n    steps: [{{run: x}}]\n");
        let result = plan(&workflow(&source), &run_context(), &Limits::default(), &Deadline::NONE);
        if message.is_empty() {
            assert_eq!(result.unwrap().len(), 1, "{matrix}");
            continue;
        }
        let error = result.unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Plan, "{matrix}");
        assert!(error.message.contains(message) && error.message.starts_with("job `j`: `strategy.matrix`"), "{matrix}: {error}");
    }
}

#[test]
fn matrix_limits() {
    let limits = Limits { max_combinations: 4, ..Limits::default() };
    let axes =
        |source: &str| Axes::resolve(workflow(source).jobs[0].strategy.matrix.as_ref().unwrap(), &run_context().contexts(), "j").unwrap();
    let source = "on: push\njobs:\n  j:\n    steps: [{run: x}]\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3]\n        b: [1, 2]\n";
    let error = combinations(&axes(source), &limits, &Deadline::NONE).unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Limit);
    let source = "on: push\njobs:\n  j:\n    steps: [{run: x}]\n    runs-on: x\n    strategy:\n      matrix:\n        a: [1, 2, 3, 4]\n        include: [{a: 5}]\n";
    assert!(combinations(&axes(source), &limits, &Deadline::NONE).is_err(), "includes count too");
    let run_limits = Limits { max_run_jobs: 5, ..Limits::default() };
    let source = "on: push\njobs:\n  a:\n    steps: [{run: x}]\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n  b:\n    steps: [{run: x}]\n    runs-on: x\n    strategy: {matrix: {v: [1, 2, 3]}}\n";
    let error = plan(&workflow(source), &run_context(), &run_limits, &Deadline::NONE).unwrap_err();
    assert_eq!(error.kind, crate::ErrorKind::Limit);
    let expired = Deadline::at(std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1)).unwrap());
    assert_eq!(plan(&workflow(source), &run_context(), &Limits::default(), &expired).unwrap_err().kind, crate::ErrorKind::Timeout);
}

#[test]
fn an_empty_matrix_is_an_error() {
    // GitHub fails such a job; it never runs once with an empty `matrix`.
    let job = |strategy: &str| format!("on: push\njobs:\n  j:\n    runs-on: x\n    strategy: {strategy}\n    steps: [{{run: x}}]\n");
    let cases = [
        ("{matrix: {os: []}}", "`os` has no values"),
        ("{matrix: {os: [a, b], exclude: [{os: a}, {os: b}]}}", "`exclude` removes every combination"),
        ("{matrix: {os: [a], v: [], include: []}}", "`v` has no values"),
    ];
    for (strategy, message) in cases {
        let error = plan(&workflow(&job(strategy)), &run_context(), &Limits::default(), &Deadline::NONE).unwrap_err();
        assert_eq!(error.kind, crate::ErrorKind::Plan, "{strategy}");
        assert!(error.message.starts_with("job `j`: `strategy.matrix`: ") && error.message.contains(message), "{strategy}: {error}");
    }
    // A job without a matrix still runs once.
    assert_eq!(planned("on: push\njobs:\n  j:\n    runs-on: x\n    steps: [{run: x}]\n").len(), 1);

    // A matrix from a need's output that turns out empty: the job's plan fails, so the job
    // fails rather than running without a matrix.
    let source = "on: push\njobs:\n  gen:\n    runs-on: x\n    steps: [{run: x}]\n  use:\n    needs: gen\n    runs-on: x\n    strategy: {matrix: '${{ fromJSON(needs.gen.outputs.m) }}'}\n    steps: [{run: x}]\n";
    let needs = |m: &str| json!({ "gen": { "result": "success", "outputs": { "m": m } } });
    let deferred = |m: &str| {
        let contexts = run_context().contexts().with("needs", needs(m));
        super::super::planning::plan_deferred(&workflow(source), "use", &contexts, 2, &Limits::default(), &Deadline::NONE)
    };
    let error = deferred(r#"{"item": []}"#).unwrap_err();
    assert!(error.message.contains("`item` has no values"), "{error}");
    let error = deferred(r#"{"item": [1], "exclude": [{"item": 1}]}"#).unwrap_err();
    assert!(error.message.contains("`exclude` removes every combination"), "{error}");
    assert_eq!(deferred(r#"{"item": [1, 2]}"#).unwrap().len(), 2);
}

#[test]
fn go_formatting() {
    let cases = [(1.17, "1.17"), (3.0, "3"), (1e21, "1e+21"), (1.5e-5, "1.5e-05"), (0.0001, "0.0001"), (-2.5, "-2.5"), (0.0, "0")];
    for (value, expected) in cases {
        assert_eq!(go_float(value), expected, "{value}");
    }
    assert_eq!(go_float(f64::NAN), "NaN");
    assert_eq!(go_format(&json!({"b": null, "a": [1, 2.5, true]})), "map[a:[1 2.5 true] b:<nil>]");
}
