//! `rbit-runner check`: whether this host can run what the configuration promises.
//!
//! It reports whether docker or podman answers (the configured program, or both without a
//! configuration), the `git` version (checkout needs 2.31 or later) and which shells of
//! §12.2 exist. A configured backend that cannot work is an error, so `check` can gate a
//! deployment script.

use std::{path::Path, time::Duration};

use rbit_workflow::protocol::Backend;

use crate::{config, process};

/// The oldest `git` the built-in checkout supports.
pub const MIN_GIT: (u32, u32) = (2, 31);
/// The shells a `run:` step may name, in the order `check` lists them.
pub const SHELLS: [&str; 6] = ["bash", "sh", "pwsh", "powershell", "cmd", "python"];

const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Whether a container CLI answers, and with which server version.
pub async fn container_engine(program: &str) -> Result<String, String> {
    let path = process::find_program(program).ok_or_else(|| format!("`{program}` is not on PATH"))?;
    // `version` talks to the daemon (or, for rootless Podman, the user's service), so it
    // fails when the CLI is installed but cannot run containers.
    let output = process::output(&path, &["version", "--format", "{{.Server.Version}}"], PROBE_TIMEOUT)
        .await
        .map_err(|error| format!("`{program} version` failed: {error}"))?;
    if output.success && !output.stdout.is_empty() {
        Ok(output.stdout)
    } else {
        let reason = output.stderr.lines().find(|line| !line.trim().is_empty()).unwrap_or("no answer").trim().to_owned();
        Err(format!("`{program}` cannot reach its engine: {reason}"))
    }
}

/// The `git` version, as `(major, minor)` and the full text.
pub async fn git_version() -> Result<((u32, u32), String), String> {
    let path = process::find_program("git").ok_or_else(|| "`git` is not on PATH".to_owned())?;
    let output = process::output(&path, &["--version"], PROBE_TIMEOUT).await.map_err(|error| format!("`git --version` failed: {error}"))?;
    let text = output.stdout;
    parse_git_version(&text).map(|version| (version, text.clone())).ok_or_else(|| format!("cannot read the git version from `{text}`"))
}

/// `git version 2.45.1.windows.1` → `(2, 45)`.
pub fn parse_git_version(text: &str) -> Option<(u32, u32)> {
    let version = text.trim().strip_prefix("git version ")?;
    let mut parts = version.split(['.', ' ']);
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// The shells of [`SHELLS`] a host step finds on `PATH`.
pub fn shells() -> Vec<&'static str> {
    let path = std::env::var("PATH").unwrap_or_default();
    SHELLS.into_iter().filter(|shell| process::find_shell(shell, &path).is_some()).collect()
}

/// Runs the checks and prints a report; fails when a configured backend cannot work.
pub async fn run(config_path: &Path) -> anyhow::Result<()> {
    let loaded = match config::load(config_path) {
        Ok(loaded) => Some(loaded),
        Err(error) if config_path.exists() => return Err(error),
        Err(_) => {
            println!("no configuration at {}; checking both container programs", config_path.display());
            None
        }
    };
    let mut problems = Vec::new();

    let programs: Vec<String> = match &loaded {
        Some(loaded) if loaded.config.backends().contains(&Backend::Container) => vec![loaded.config.container.program.clone()],
        Some(_) => Vec::new(),
        None => vec!["docker".to_owned(), "podman".to_owned()],
    };
    for program in &programs {
        match container_engine(program).await {
            Ok(version) => println!("container: {program} is reachable (engine {version})"),
            Err(error) => {
                println!("container: {error}");
                if loaded.is_some() {
                    problems.push(error);
                }
            }
        }
    }

    match git_version().await {
        Ok((version, text)) if version >= MIN_GIT => println!("git: {text}"),
        Ok((_, text)) => {
            let (major, minor) = MIN_GIT;
            let problem = format!("{text} is too old: checkout needs git {major}.{minor} or later");
            println!("git: {problem}");
            problems.push(problem);
        }
        Err(error) => {
            println!("git: {error}");
            problems.push(error);
        }
    }

    let found = shells();
    println!("shells: {}", if found.is_empty() { "none found".to_owned() } else { found.join(", ") });
    if let Some(loaded) = &loaded {
        if loaded.config.backends().contains(&Backend::Host) && found.is_empty() {
            problems.push("the host backend has no shell to run steps with".to_owned());
        }
        if let Some(warning) = config::host_warning(&loaded.config) {
            println!("{warning}");
        }
    }

    if problems.is_empty() {
        println!("ok");
        Ok(())
    } else {
        anyhow::bail!("{} problem(s): {}", problems.len(), problems.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_versions_parse() {
        assert_eq!(parse_git_version("git version 2.45.1.windows.1"), Some((2, 45)));
        assert_eq!(parse_git_version("git version 2.30.0"), Some((2, 30)));
        assert_eq!(parse_git_version("git version 2.39.5 (Apple Git-154)"), Some((2, 39)));
        assert_eq!(parse_git_version("hg 6.0"), None);
        assert!((2, 30) < MIN_GIT && (2, 31) == MIN_GIT && (3, 0) > MIN_GIT);
    }

    #[tokio::test]
    async fn missing_programs_are_reported_not_fatal() {
        let error = container_engine("rbit-no-such-container-program").await.unwrap_err();
        assert!(error.contains("not on PATH"), "{error}");
        // The probe itself must work wherever the tests run.
        let _ = shells();
    }
}
