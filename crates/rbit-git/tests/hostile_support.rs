//! Shared fixtures of the hostile-input tests: real repositories built with the `git`
//! CLI, command markers that prove whether a configured command ran, and directory links
//! (symlinks on Unix, junctions on Windows) that point out of a root.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use rbit_git::merge::MergeEnvironment;

/// Runs `git` in `dir` with isolated configuration and a fixed identity; its trimmed
/// standard output.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = run_git(dir, args, &[]);
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Runs `git` like [`git`] with extra environment, whatever its exit status.
pub fn run_git(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new("git");
    command
        .args(["-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .env("GIT_AUTHOR_NAME", "Ada")
        .env("GIT_AUTHOR_EMAIL", "ada@example.com")
        .env("GIT_COMMITTER_NAME", "Ada")
        .env("GIT_COMMITTER_EMAIL", "ada@example.com");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("git is installed")
}

/// A bare repository `repo.git` whose `main` holds `files`, built in a fresh directory.
pub struct Repo {
    pub dir: tempfile::TempDir,
    pub bare: PathBuf,
}

impl Repo {
    pub fn new(files: &[(&str, &[u8])]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q"]);
        for (path, content) in files {
            let path = work.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        git(&work, &["add", "--all"]);
        git(&work, &["commit", "-q", "-m", "initial"]);
        let bare = dir.path().join("repo.git");
        git(dir.path(), &["clone", "-q", "--bare", work.to_str().unwrap(), bare.to_str().unwrap()]);
        Self { dir, bare }
    }

    pub fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    /// `git --git-dir <bare> <args>`.
    pub fn bare(&self, args: &[&str]) -> String {
        let mut full = vec!["--git-dir", self.bare.to_str().unwrap()];
        full.extend_from_slice(args);
        git(self.dir.path(), &full)
    }

    /// `git --git-dir <bare> <args>` reading `input` on standard input.
    pub fn bare_with_input(&self, args: &[&str], input: &[u8]) -> String {
        use std::io::Write;
        let mut child = Command::new("git")
            .arg("--git-dir")
            .arg(&self.bare)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "git {args:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Sets `key` in the bare repository's own configuration (`--add` keeps earlier values).
    pub fn config(&self, key: &str, value: &str) {
        self.bare(&["config", "--add", key, value]);
    }

    pub fn tip(&self, branch: &str) -> String {
        self.bare(&["rev-parse", &format!("refs/heads/{branch}")])
    }

    /// Where merges and edits put their scratch repositories.
    pub fn env(&self) -> MergeEnvironment {
        MergeEnvironment { git_binary: "git".into(), tmp_dir: self.dir.path().join("tmp"), timeout: Duration::from_secs(60) }
    }
}

/// A file that a configured command creates when it runs, and the shell code creating it.
pub struct Marker {
    pub path: PathBuf,
}

impl Marker {
    pub fn new(dir: &Path, name: &str) -> Self {
        Self { path: dir.join(name) }
    }

    /// The marker's path as `sh` on every platform understands it.
    pub fn shell_path(&self) -> String {
        self.path.to_string_lossy().replace('\\', "/")
    }

    /// Shell code appending a line to the marker.
    pub fn command(&self) -> String {
        format!("echo ran >> '{}'", self.shell_path())
    }

    /// A hook script that appends to the marker.
    pub fn hook(&self, hooks: &Path, name: &str) {
        std::fs::create_dir_all(hooks).unwrap();
        let path = hooks.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{}\n", self.command())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    pub fn ran(&self) -> bool {
        self.path.exists()
    }

    pub fn reset(&self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Makes `link` a directory link to `target`: a symlink on Unix, a junction on Windows
/// (which needs no privilege).
pub fn link_directory(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    {
        // `cmd` reads a forward slash as a switch.
        let native = |path: &Path| path.to_string_lossy().replace('/', "\\");
        let status = Command::new("cmd").args(["/C", "mklink", "/J"]).arg(native(link)).arg(native(target)).output().unwrap();
        assert!(status.status.success(), "mklink /J failed: {}", String::from_utf8_lossy(&status.stderr));
    }
}

/// A directory outside every root under test holding one file that must survive.
pub fn precious_outside() -> (tempfile::TempDir, PathBuf) {
    let outside = tempfile::tempdir().unwrap();
    let file = outside.path().join("precious.txt");
    std::fs::write(&file, "keep me").unwrap();
    (outside, file)
}
