//! Finding and running programs.
//!
//! Programs are found on `PATH` the way a shell would (with `PATHEXT` on Windows), so
//! `check` reports exactly what a step would run. [`output`] runs a short command with a
//! deadline and kills it when the deadline passes.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::process::Command;

/// The first `name` on `PATH`, or `name` itself when it is a path to a file.
pub fn find_program(name: &str) -> Option<PathBuf> {
    find_in(name, std::env::var_os("PATH").as_deref(), std::env::var_os("PATHEXT").as_deref())
}

fn find_in(name: &str, path: Option<&OsStr>, path_ext: Option<&OsStr>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        let mut candidates = vec![base.clone()];
        if cfg!(windows) && base.extension().is_none() {
            let extensions = path_ext.map_or_else(|| OsString::from(".COM;.EXE;.BAT;.CMD"), OsStr::to_os_string);
            for extension in extensions.to_string_lossy().split(';').filter(|extension| !extension.is_empty()) {
                let mut with = base.clone().into_os_string();
                with.push(extension.to_ascii_lowercase());
                candidates.push(PathBuf::from(with));
            }
        }
        candidates
    };
    if Path::new(name).components().count() > 1 || Path::new(name).is_absolute() {
        return candidates(PathBuf::from(name)).into_iter().find(|candidate| candidate.is_file());
    }
    std::env::split_paths(path?).flat_map(|dir| candidates(dir.join(name))).find(|candidate| candidate.is_file())
}

/// What a short command printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program` with `args` and no input, for at most `timeout`.
pub async fn output(program: &Path, args: &[&str], timeout: Duration) -> std::io::Result<Output> {
    let child =
        Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true).spawn()?;
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(output) => {
            let output = output?;
            Ok(Output {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
        Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("did not finish within {}s", timeout.as_secs()))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_are_found_on_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let tool = if cfg!(windows) { "tool.exe" } else { "tool" };
        std::fs::write(dir.path().join(tool), "").unwrap();
        let path = std::env::join_paths([dir.path().join("missing"), dir.path().to_path_buf()]).unwrap();
        let found = find_in("tool", Some(&path), Some(OsStr::new(".EXE;.CMD"))).unwrap();
        assert_eq!(found.file_name().unwrap().to_string_lossy().to_lowercase(), tool);
        assert!(find_in("other", Some(&path), None).is_none());
        assert!(find_in("", Some(&path), None).is_none());
        let direct = dir.path().join(tool);
        assert_eq!(find_in(direct.to_str().unwrap(), None, None), Some(direct));
    }

    #[tokio::test]
    async fn output_is_captured_and_bounded() {
        let Some(git) = find_program("git") else { return };
        let output = output(&git, &["--version"], Duration::from_secs(30)).await.unwrap();
        assert!(output.success && output.stdout.starts_with("git version"), "{output:?}");
    }
}
