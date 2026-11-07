//! Existing expression conditions acceptance cases.
use super::super::{Condition, ContextMap, Contexts, Expr, MAX_JSON_INPUT_BYTES, MAX_RESULT_BYTES, MAX_STEPS, Status, Template};
use super::support::contexts;
use crate::spec::JobResult;
use serde_json::{Value, json};

#[test]
fn status_functions_follow_the_status() {
    let holds = |condition: &str, status: Status| Condition::parse(condition).unwrap().evaluate(&contexts().with_status(status)).unwrap();
    let table = [
        ("success()", [true, false, false, false]),
        ("failure()", [false, true, false, false]),
        ("cancelled()", [false, false, true, false]),
        ("always()", [true, true, true, true]),
        ("", [true, false, false, false]),
        ("true", [true, false, false, false]),
        ("!cancelled()", [true, true, false, true]),
        ("failure() || cancelled()", [false, true, true, false]),
    ];
    for (condition, expected) in table {
        let got = [Status::Success, Status::Failure, Status::Cancelled, Status::Skipped].map(|status| holds(condition, status));
        assert_eq!(got, expected, "{condition}");
    }
    assert!(Expr::parse("success()").unwrap().evaluate(&contexts()).unwrap_err().0.contains("only be used in `if`"));
    let of_needs = |results: &[JobResult]| Status::of_needs(results.iter().copied());
    assert_eq!(of_needs(&[]), Status::Success);
    assert_eq!(of_needs(&[JobResult::Success, JobResult::Success]), Status::Success);
    assert_eq!(of_needs(&[JobResult::Skipped, JobResult::Success]), Status::Skipped);
    assert_eq!(of_needs(&[JobResult::Skipped, JobResult::Cancelled]), Status::Cancelled);
    assert_eq!(of_needs(&[JobResult::Failure, JobResult::Cancelled, JobResult::Skipped]), Status::Failure);
    assert_eq!(of_needs(&[JobResult::Cancelled, JobResult::Failure]), Status::Failure);
}

#[test]
fn conditions() {
    let cases = [
        ("", "success()"),
        ("  ", "success()"),
        ("true", "success() && true"),
        ("github.ref == 'refs/heads/main'", "success() && github.ref == 'refs/heads/main'"),
        ("${{ github.ref == 'refs/heads/main' }}", "success() && github.ref == 'refs/heads/main'"),
        (" ${{ github.ref }} ", "success() && github.ref"),
        ("env.a || env.b", "success() && (env.a || env.b)"),
        ("always() && env.x", "always() && env.x"),
        ("${{ !cancelled() }}", "!cancelled()"),
        ("contains(needs.*.result, 'failure') || failure()", "contains(needs.*.result, 'failure') || failure()"),
    ];
    for (text, expected) in cases {
        assert_eq!(Condition::parse(text).unwrap().to_string(), expected, "{text}");
    }
    assert_eq!(Condition::parse("").unwrap(), Condition::success());
    for refused in ["${{ github.ref }} == 'x'", "a ${{ github.ref }}", "${{ github.ref", "github.ref ==", "${{ }}"] {
        assert!(Condition::parse(refused).is_err(), "{refused}");
    }
    let skipped = contexts().with_status(Status::Skipped);
    assert!(!Condition::parse("true").unwrap().evaluate(&skipped).unwrap());
    assert!(Condition::parse("always()").unwrap().evaluate(&skipped).unwrap());
    assert!(Condition::parse("needs.lint.result == 'skipped'").unwrap().expr().uses_status());
}

#[test]
fn hash_files_goes_through_the_hook() {
    struct Runner(ContextMap);
    impl Contexts for Runner {
        fn context(&self, name: &str) -> Option<&Value> {
            self.0.context(name)
        }
        fn hash_files(&self, patterns: &[String]) -> Option<Result<String, String>> {
            Some(if patterns.is_empty() { Err("no pattern".into()) } else { Ok(format!("hash of {}", patterns.join(" "))) })
        }
    }
    let runner = Runner(contexts());
    let key = Template::parse("cargo-${{ hashFiles('**/Cargo.lock', matrix.os) }}").unwrap().render(&runner).unwrap();
    assert_eq!(key, "cargo-hash of **/Cargo.lock linux");
}

#[test]
fn limits_are_errors() {
    let big = "x".repeat(MAX_JSON_INPUT_BYTES + 1);
    let half = "y".repeat(MAX_RESULT_BYTES / 2 + 1);
    let numbers = json!((0..MAX_STEPS).collect::<Vec<_>>());
    let grid = json!(vec![vec![1; 200]; 60]);
    let contexts = ContextMap::new().with("env", json!({"big": big, "half": half, "numbers": numbers, "grid": grid}));
    let evaluate = |text: &str| Template::parse(text).and_then(|template| template.render(&contexts));
    let cases = [
        ("${{ fromJSON(env.big) }}", "fromJSON: the input is larger"),
        ("${{ format('{0}{0}', env.half) }}", "format: the result is larger"),
        ("${{ join(fromJSON('[1, 2, 3]'), env.half) }}", "join: the result is larger"),
        ("${{ env.half }}${{ env.half }}", "the result is larger"),
        ("${{ contains(env.numbers, -1) }}", "evaluation steps"),
        ("${{ join(env.numbers) }}", "evaluation steps"),
        ("${{ env.grid.*.* }}", "evaluation steps"),
        ("${{ contains(env.grid.*.*, 2) }}", "evaluation steps"),
    ];
    for (template, message) in cases {
        let error = evaluate(template).unwrap_err();
        assert!(error.0.contains(message), "{template}: {error}");
    }
    // A few large calls run out of steps, not only many small ones.
    let costly = format!("${{{{ {} }}}}", vec!["toJSON(env.grid)"; 200].join(" && "));
    assert!(evaluate(&costly).unwrap_err().0.contains("evaluation steps"));
    assert!(evaluate("${{ contains(env.grid.*[0], 1) }}").is_ok());
}
