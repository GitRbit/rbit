//! Original planner golden fixtures; behavior assertions retained.
use super::support::planned;
use crate::spec::JobTemplate;

/// Each case plans a workflow and compares its job templates, as pretty JSON, with the
/// expected file. The inputs are Gitea's job parser test data (MIT, see
/// `THIRD-PARTY.md`), the outputs rbit's.
const GOLDEN: [(&str, &str, &str); 7] = [
    (
        "multiple_jobs",
        include_str!("../../../tests/golden/multiple_jobs.in.yaml"),
        include_str!("../../../tests/golden/multiple_jobs.out.json"),
    ),
    (
        "multiple_matrix",
        include_str!("../../../tests/golden/multiple_matrix.in.yaml"),
        include_str!("../../../tests/golden/multiple_matrix.out.json"),
    ),
    (
        "job_name_with_matrix",
        include_str!("../../../tests/golden/job_name_with_matrix.in.yaml"),
        include_str!("../../../tests/golden/job_name_with_matrix.out.json"),
    ),
    ("has_needs", include_str!("../../../tests/golden/has_needs.in.yaml"), include_str!("../../../tests/golden/has_needs.out.json")),
    (
        "prefixed_newline",
        include_str!("../../../tests/golden/prefixed_newline.in.yaml"),
        include_str!("../../../tests/golden/prefixed_newline.out.json"),
    ),
    (
        "continue_on_error_expr",
        include_str!("../../../tests/golden/continue_on_error_expr.in.yaml"),
        include_str!("../../../tests/golden/continue_on_error_expr.out.json"),
    ),
    (
        "include_exclude",
        include_str!("../../../tests/golden/include_exclude.in.yaml"),
        include_str!("../../../tests/golden/include_exclude.out.json"),
    ),
];

#[test]
fn golden_templates() {
    // RBIT_BLESS=1 rewrites the expected files after a deliberate change.
    let bless = std::env::var_os("RBIT_BLESS").is_some();
    for (name, input, expected) in GOLDEN {
        let jobs = planned(input);
        let actual = serde_json::to_string_pretty(&jobs).unwrap() + "\n";
        if bless {
            std::fs::write(format!("{}/tests/golden/{name}.out.json", env!("CARGO_MANIFEST_DIR")), &actual).unwrap();
            continue;
        }
        assert_eq!(actual, expected.replace("\r\n", "\n"), "{name}");
        assert_eq!(serde_json::from_str::<Vec<JobTemplate>>(expected).unwrap(), jobs, "{name}: the templates read back");
    }
}
