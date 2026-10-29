//! Finding and running programs.
//!
//! Programs are found on `PATH` the way a shell would (with `PATHEXT` on Windows), so
//! `check` reports exactly what a step would run. [`output`] runs a short command with a
//! deadline and kills it when the deadline passes.

use std::{
    ffi::{OsStr, OsString},
    future::Future,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use tokio::process::Command;

/// Longest line [`run_streaming`] hands on before cutting it, in bytes. The task log masks
/// a whole line before cutting it to the protocol's `MAX_LOG_LINE_BYTES`, so this is kept
/// well above that: a secret across a cut here would be masked in neither piece.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// The first `name` on `PATH`, or `name` itself when it is a path to a file.
pub fn find_program(name: &str) -> Option<PathBuf> {
    find_in(name, std::env::var_os("PATH").as_deref(), std::env::var_os("PATHEXT").as_deref())
}

/// The first `name` on the search path `path` (a step's `PATH`, with its `GITHUB_PATH`
/// additions).
pub fn find_on(name: &str, path: &str) -> Option<PathBuf> {
    find_in(name, Some(OsStr::new(path)), std::env::var_os("PATHEXT").as_deref())
}

/// The shell `name` of a host step on `path`: [`find_on`], except that on Windows `bash`
/// and `sh` are Git for Windows' own. The `bash.exe` in `System32` (or the Store's
/// `WindowsApps`) starts WSL, where the step's `C:/…` script does not exist, and Git's
/// installer puts only `Git\cmd` on `PATH` by default, so Git's shells are also looked for
/// next to `git`.
pub fn find_shell(name: &str, path: &str) -> Option<PathBuf> {
    if !cfg!(windows) || !matches!(name, "bash" | "sh") {
        return find_on(name, path);
    }
    let path_ext = std::env::var_os("PATHEXT");
    let starts_wsl = |found: &Path| {
        found
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|dir| ["system32", "syswow64", "sysnative", "windowsapps"].iter().any(|system| dir.eq_ignore_ascii_case(system)))
    };
    std::env::split_paths(OsStr::new(path))
        .filter_map(|dir| find_in(name, Some(dir.as_os_str()), path_ext.as_deref()))
        .find(|found| !starts_wsl(found))
        .or_else(|| {
            // `Git\cmd\git.exe`, `Git\bin\git.exe` or `Git\mingw64\bin\git.exe`.
            let git = find_in("git", Some(OsStr::new(path)), path_ext.as_deref())?;
            git.ancestors().skip(2).take(2).map(|root| root.join("bin").join(format!("{name}.exe"))).find(|shell| shell.is_file())
        })
}

// Actions: third-party actions.
/// `path` with the directory of Git's `bash` in front, when the first `bash` on it would
/// start WSL (or there is none): on GitHub's Windows machines `bash` in a script is Git's,
/// and actions call it from PowerShell (`taiki-e/install-action`). `None` when `path`
/// already has it first, or there is no Git `bash`.
pub fn with_git_bash_first(path: &str) -> Option<String> {
    let git_bash = find_shell("bash", path)?;
    if find_on("bash", path).as_ref() == Some(&git_bash) {
        return None;
    }
    let dir = git_bash.parent()?.to_path_buf();
    let joined = std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(OsStr::new(path)))).ok()?;
    Some(joined.to_string_lossy().into_owned())
}

fn find_in(name: &str, path: Option<&OsStr>, path_ext: Option<&OsStr>) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        // Windows cannot start an extensionless file (Git's shell scripts next to bash.exe).
        if !cfg!(windows) || base.extension().is_some() {
            return vec![base];
        }
        let mut candidates = Vec::new();
        {
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
    let mut command = Command::new(program);
    command.args(args);
    capture(command, None, timeout).await
}

/// Runs `command` with `input` on its standard input (or none), for at most `timeout`,
/// and returns what it printed.
pub async fn capture(mut command: Command, input: Option<&[u8]>, timeout: Duration) -> std::io::Result<Output> {
    use tokio::io::AsyncWriteExt as _;
    command
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(input).await?;
        drop(stdin);
    }
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

/// How a step's process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// It exited with this code (`None`: killed by a signal).
    Code(Option<i32>),
    /// It was stopped because `stop` fired.
    Stopped,
}

/// How long a stopped step has between the polite and the forceful signal.
pub const STOP_GRACE: Duration = Duration::from_secs(10);

/// How a running step is stopped (§12.5.4 "Timeouts and cancellation").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopper {
    /// A process on the host. On Unix it runs in its own process group, which gets
    /// `kill -TERM`, then `kill -KILL` after [`STOP_GRACE`], through the `kill` program so
    /// no `unsafe` is needed. On Windows the tree gets `taskkill /T`, then `taskkill /T /F`.
    Host,
    /// A step `exec`-ed in a container: every process in it but its init gets `kill
    /// -TERM -1`, then `kill -KILL -1`, so the container stays for later `always()` steps.
    /// If that fails, the container itself is killed.
    Container { program: PathBuf, env: Vec<(String, PathBuf)>, container: String },
}

/// Puts a step's process in its own process group (Unix), so it can be stopped with
/// everything it started, without the runner's own variables.
pub fn isolate(command: &mut Command) {
    #[cfg(unix)]
    command.process_group(0);
    drop_runner_env(command);
}

/// A command for `program` without the runner's own variables.
pub fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    drop_runner_env(&mut command);
    command
}

/// Keeps the runner's `RBIT_RUNNER_*` variables (its registration token among them) from a
/// child: no step, `git` or engine call needs them.
fn drop_runner_env(command: &mut Command) {
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().to_ascii_uppercase().starts_with("RBIT_RUNNER_") {
            command.env_remove(name);
        }
    }
}

impl Stopper {
    /// Sends the polite (`force == false`) or forceful signal; `true` when it was sent.
    async fn signal(&self, pid: Option<u32>, force: bool) -> bool {
        let (program, args, env): (PathBuf, Vec<String>, &[(String, PathBuf)]) = match self {
            Self::Host if cfg!(windows) => {
                let Some(pid) = pid else { return false };
                let mut args = vec!["/T".to_owned()];
                if force {
                    args.push("/F".to_owned());
                }
                args.extend(["/PID".to_owned(), pid.to_string()]);
                (find_program("taskkill").unwrap_or_else(|| PathBuf::from("taskkill")), args, &[])
            }
            Self::Host => {
                let Some(pid) = pid else { return false };
                let signal = if force { "KILL" } else { "TERM" };
                let args = ["-s", signal, "--", &format!("-{pid}")].map(str::to_owned).to_vec();
                (find_program("kill").unwrap_or_else(|| PathBuf::from("kill")), args, &[])
            }
            Self::Container { program, env, container } => {
                let signal = if force { "-KILL" } else { "-TERM" };
                (program.clone(), ["exec", container, "kill", signal, "-1"].map(str::to_owned).to_vec(), env.as_slice())
            }
        };
        let mut command = self::command(&program);
        command.args(&args).envs(env.iter().map(|(name, value)| (name, value)));
        let sent = capture(command, None, Duration::from_secs(30)).await.is_ok_and(|output| output.success);
        if let (Self::Container { program, env, container }, false, true) = (self, sent, force) {
            let mut command = self::command(program);
            command.args(["kill", container.as_str()]).envs(env.iter().map(|(name, value)| (name, value)));
            return capture(command, None, Duration::from_secs(30)).await.is_ok_and(|output| output.success);
        }
        sent
    }

    /// The polite signal, [`STOP_GRACE`], then the forceful one. On Windows a refused
    /// `taskkill` without `/F` leaves nothing to wait for: it cannot end a console process
    /// that has no window, which every host step is.
    async fn escalate(self, pid: Option<u32>) {
        let sent = self.signal(pid, false).await;
        if sent || !(cfg!(windows) && matches!(self, Self::Host)) {
            tokio::time::sleep(STOP_GRACE).await;
        }
        self.signal(pid, true).await;
    }
}

/// Runs `command` to its end, handing each line of its standard output and error to
/// `on_line` as it comes (lines longer than `max_line` bytes are cut into pieces). When
/// `stop` fires first, the process is stopped as `stopper` says and [`Exit::Stopped`]
/// returned once it has ended.
pub async fn run_streaming(
    mut command: Command,
    max_line: usize,
    stop: impl Future<Output = ()>,
    stopper: &Stopper,
    mut on_line: impl FnMut(String),
) -> std::io::Result<Exit> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn()?;
    let pid = child.id();
    // The escalation runs beside the wait; it says when its forceful signal is out, and
    // is aborted as soon as the process has ended, so no signal reaches a reused id.
    let (forced, mut forced_signal) = tokio::sync::mpsc::channel::<()>(1);
    let mut escalation: Option<tokio::task::JoinHandle<()>> = None;
    let (sender, mut lines) = tokio::sync::mpsc::channel::<String>(256);
    let pipes: [Option<Pipe>; 2] =
        [child.stdout.take().map(|out| Box::new(out) as Pipe), child.stderr.take().map(|err| Box::new(err) as Pipe)];
    let readers: Vec<_> = pipes.into_iter().flatten().map(|pipe| tokio::spawn(read_lines(pipe, max_line, sender.clone()))).collect();
    drop(sender);
    tokio::pin!(stop);
    let mut stop_sent = false;
    let mut status = None;
    // Once the process has exited, its output is read for a little longer: a background
    // process it left behind may hold the pipes open indefinitely.
    let mut drain_until = None;
    let mut closed = false;
    while !(closed && status.is_some()) {
        tokio::select! {
            line = lines.recv(), if !closed => match line {
                Some(line) => on_line(line),
                None => closed = true,
            },
            exited = child.wait(), if status.is_none() => {
                status = Some(exited?);
                drain_until = Some(tokio::time::Instant::now() + OUTPUT_DRAIN);
            }
            () = tokio::time::sleep_until(drain_until.unwrap_or_else(tokio::time::Instant::now)), if drain_until.is_some() => break,
            () = &mut stop, if !stop_sent => {
                stop_sent = true;
                let (stopper, forced) = (stopper.clone(), forced.clone());
                escalation = Some(tokio::spawn(async move {
                    stopper.escalate(pid).await;
                    let _ = forced.send(()).await;
                }));
            }
            Some(()) = forced_signal.recv(), if status.is_none() => {
                // The grace period is over and the forceful signal is out; make sure.
                let _ = child.start_kill();
            }
        }
    }
    if let Some(escalation) = escalation {
        escalation.abort();
    }
    for reader in readers {
        reader.abort();
    }
    let code = status.and_then(|status| status.code());
    Ok(if stop_sent { Exit::Stopped } else { Exit::Code(code) })
}

/// How long output is still read after a step's process has exited.
const OUTPUT_DRAIN: Duration = Duration::from_secs(2);

/// A process's standard output or error.
type Pipe = Box<dyn tokio::io::AsyncRead + Unpin + Send>;

/// Sends the lines of `reader`, decoded lossily, until it closes.
async fn read_lines(mut reader: Pipe, max_line: usize, sender: tokio::sync::mpsc::Sender<String>) {
    use tokio::io::AsyncReadExt as _;
    let mut buffer = vec![0u8; 16 * 1024];
    let mut line = Vec::new();
    // Whether `line` continues a line already cut: its end then sends nothing when empty.
    let mut continued = false;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        for &byte in &buffer[..read] {
            if byte == b'\n' {
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                if std::mem::take(&mut continued) && line.is_empty() {
                    continue;
                }
                if sender.send(String::from_utf8_lossy(&std::mem::take(&mut line)).into_owned()).await.is_err() {
                    return;
                }
            } else {
                line.push(byte);
                if line.len() >= max_line {
                    // Cut before a character the piece ends inside of, so it is not mangled.
                    let cut = match std::str::from_utf8(&line) {
                        Err(error) if error.error_len().is_none() && error.valid_up_to() > 0 => error.valid_up_to(),
                        _ => line.len(),
                    };
                    let rest = line.split_off(cut);
                    continued = true;
                    if sender.send(String::from_utf8_lossy(&std::mem::replace(&mut line, rest)).into_owned()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
    if !line.is_empty() {
        let _ = sender.send(String::from_utf8_lossy(&line).into_owned()).await;
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

    #[cfg(windows)]
    #[test]
    fn windows_shells_are_git_for_windows_not_wsl() {
        let root = tempfile::tempdir().unwrap();
        let dir = |parts: &[&str]| {
            let dir = parts.iter().fold(root.path().to_path_buf(), |dir, part| dir.join(part));
            std::fs::create_dir_all(&dir).unwrap();
            dir
        };
        let (system, git_cmd, git_bin, usr_bin) =
            (dir(&["Windows", "System32"]), dir(&["Git", "cmd"]), dir(&["Git", "bin"]), dir(&["Git", "usr", "bin"]));
        for file in
            [system.join("bash.exe"), git_cmd.join("git.exe"), git_bin.join("bash.exe"), git_bin.join("sh.exe"), usr_bin.join("bash.exe")]
        {
            std::fs::write(file, "").unwrap();
        }
        let path = |dirs: &[&PathBuf]| std::env::join_paths(dirs).unwrap().to_string_lossy().into_owned();
        // Git's default installation: only `Git\cmd` on `PATH`, after `System32`.
        assert_eq!(find_shell("bash", &path(&[&system, &git_cmd])), Some(git_bin.join("bash.exe")));
        assert_eq!(find_shell("sh", &path(&[&system, &git_cmd])), Some(git_bin.join("sh.exe")));
        // A bash put on `PATH` by hand still wins, as long as it is not WSL's.
        assert_eq!(find_shell("bash", &path(&[&system, &usr_bin, &git_cmd])), Some(usr_bin.join("bash.exe")));
        assert_eq!(find_shell("bash", &path(&[&system])), None);
        assert_eq!(find_shell("pwsh", &path(&[&system, &git_cmd])), None, "other shells are looked up as before");

        // A script's own `bash` is Git's too: its directory goes first.
        let default = path(&[&system, &git_cmd]);
        assert_eq!(with_git_bash_first(&default), Some(path(&[&git_bin, &system, &git_cmd])));
        assert_eq!(with_git_bash_first(&path(&[&git_bin, &system])), None, "already first");
        assert_eq!(with_git_bash_first(&path(&[&system])), None, "no Git");
    }

    #[tokio::test]
    async fn long_lines_are_cut_between_characters() {
        let lines = |text: &'static str, max_line| async move {
            let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
            read_lines(Box::new(text.as_bytes()), max_line, sender).await;
            let mut lines = Vec::new();
            while let Some(line) = receiver.recv().await {
                lines.push(line);
            }
            lines
        };
        assert_eq!(lines("abcdéfgh\r\nnext\nlast", 5).await, ["abcd", "éfgh", "next", "last"], "é is two bytes");
        assert_eq!(lines("ab€cd\n", 4).await, ["ab", "€c", "d"]);
        assert_eq!(lines("short\n", MAX_LINE_BYTES).await, ["short"]);
        const { assert!(MAX_LINE_BYTES > rbit_workflow::protocol::MAX_LOG_LINE_BYTES * 8, "the log cuts lines after masking them") };
    }

    #[tokio::test]
    async fn output_is_captured_and_bounded() {
        let Some(git) = find_program("git") else { return };
        let output = output(&git, &["--version"], Duration::from_secs(30)).await.unwrap();
        assert!(output.success && output.stdout.starts_with("git version"), "{output:?}");
    }
}
