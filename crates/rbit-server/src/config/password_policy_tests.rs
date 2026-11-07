//! Startup precedence is tested in isolated processes; no unsafe environment mutation.

use std::{path::PathBuf, process::Command};

#[test]
fn startup_rejects_invalid_password_policy_and_environment_overrides_file_and_legacy_values() {
    let cases = [
        (4, None, None, "4"),
        (0, None, None, "invalid"),
        (65, None, None, "invalid"),
        (4, Some("3"), Some("5"), "5"),
        (4, None, Some("0"), "invalid"),
        (4, None, Some("65"), "invalid"),
        (0, None, Some("64"), "64"),
    ];
    for (file_capacity, legacy, current, expected) in cases {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("rbit.toml");
        std::fs::write(&file, format!("[security]\nmax_password_jobs = {file_capacity}\n")).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "config::password_policy_tests::configuration_load_child", "--ignored", "--nocapture"]);
        // Keep ordinary OS variables while preventing inherited deployment settings from
        // making this test inspect any real instance or change the selected policy.
        for (name, _) in std::env::vars_os() {
            let upper = name.to_string_lossy().to_ascii_uppercase();
            if upper.starts_with("RBIT_") || upper.starts_with("RUSTIT_") {
                child.env_remove(name);
            }
        }
        child
            .env("PASSWORD_POLICY_FILE", &file)
            .env("PASSWORD_POLICY_EXPECT", expected)
            .env("RBIT_STORAGE__DATA_DIR", directory.path().join("isolated-data"));
        if let Some(value) = legacy {
            child.env("RUSTIT_SECURITY__MAX_PASSWORD_JOBS", value);
        }
        if let Some(value) = current {
            child.env("RBIT_SECURITY__MAX_PASSWORD_JOBS", value);
        }
        let output = child.output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(output.status.success(), "child failed: {stdout}\n{}", String::from_utf8_lossy(&output.stderr));
        assert!(stdout.contains("running 1 test") && stdout.contains("1 passed"), "the isolated loader assertion must actually run");
    }
}

#[tokio::test]
#[ignore = "invoked only by the isolated startup policy test"]
async fn configuration_load_child() {
    let file = PathBuf::from(std::env::var_os("PASSWORD_POLICY_FILE").expect("parent supplies isolated file"));
    let expected = std::env::var("PASSWORD_POLICY_EXPECT").unwrap();
    let loaded = super::load(Some(&file)).await;
    if expected == "invalid" {
        assert!(loaded.unwrap_err().to_string().contains("security.max_password_jobs must be between 1 and 64"));
    } else {
        assert_eq!(loaded.unwrap().security.password_job_capacity().unwrap().get(), expected.parse::<usize>().unwrap());
    }
}
