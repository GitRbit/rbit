//! The executable emits one valid JSON measurement and rejects mismatched inputs.

use std::process::Command;

use rbit_render_probe::{CASE_COUNT, COMPILED_MODES};
use serde_json::Value;

#[test]
fn cli_reports_compiled_inputs_and_json_escapes_labels_and_optional_html() {
    let label = "revision=example \"quoted\"\nflags=recorded externally";
    let result = Command::new(env!("CARGO_BIN_EXE_rbit-render-probe"))
        .args([
            "--mode",
            COMPILED_MODES[0].name(),
            "--count",
            &CASE_COUNT.to_string(),
            "--iterations",
            "1",
            "--warmup",
            "0",
            "--label",
            label,
            "--emit",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["label"], label);
    assert_eq!(report["mode"], COMPILED_MODES[0].name());
    assert_eq!(report["count"].as_u64(), Some(u64::try_from(CASE_COUNT).unwrap()));
    assert_eq!(report["rendered_documents"].as_u64(), Some(u64::try_from(CASE_COUNT).unwrap()));
    assert!(report["rendered_bytes"].as_u64().unwrap() > 0);
    assert!(report["elapsed_ns"].as_u64().is_some());
    assert!(report["compiler_peak_memory_bytes"].is_null() && report["compile_wall_ns"].is_null());
    assert!(report["sample_html"].as_str().unwrap().contains("data-case=\"0\""));
    assert_eq!(report["build"]["askama_version"], "0.16.1");
}

#[test]
fn cli_rejects_uncompiled_counts_unknown_modes_and_invalid_iterations() {
    for arguments in [
        vec!["--count".to_owned(), (CASE_COUNT + 1).to_string()],
        vec!["--mode".to_owned(), "unknown".to_owned()],
        vec!["--iterations".to_owned(), "0".to_owned()],
        vec!["--iterations".to_owned()],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_rbit-render-probe")).args(arguments).output().unwrap();
        assert!(!result.status.success());
        assert_eq!(result.stdout, [] as [u8; 0]);
        let error: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert!(error["error"].is_string());
    }
}
