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

/// The first `name` on `PATH`, or `name` itself when it is a path to a file.
pub fn find_program(name: &str) -> Option<PathBuf> {
    find_in(name, std::env::var_os("PATH").as_deref(), std::env::var_os("PATHEXT").as_deref())
}

/// The first `name` on the search path `path` (a step's `PATH`, with its `GITHUB_PATH`
/// additions).
pub fn find_on(name: &str, path: &str) -> Option<PathBuf> {
    find_in(name, Some(OsStr::new(path)), std::env::var_os("PATHEXT").as_deref())
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

/// How a step's process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// It exited with this code (`None`: killed by a signal).
    Code(Option<i32>),
    /// It was stopped because `stop` fired.
    Stopped,
}

/// Runs `command` to its end, handing each line of its standard output and error to
/// `on_line` as it comes (lines longer than `max_line` bytes are cut into pieces). When
/// `stop` fires first, the process is killed and [`Exit::Stopped`] returned.
pub async fn run_streaming(
    mut command: Command,
    max_line: usize,
    stop: impl Future<Output = ()>,
    mut on_line: impl FnMut(String),
) -> std::io::Result<Exit> {
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = command.spawn()?;
    let (sender, mut lines) = tokio::sync::mpsc::channel::<String>(256);
    let readers: Vec<_> = [child.stdout.take().map(Pipe::Out), child.stderr.take().map(Pipe::Err)]
        .into_iter()
        .flatten()
        .map(|pipe| tokio::spawn(read_lines(pipe, max_line, sender.clone())))
        .collect();
    drop(sender);
    tokio::pin!(stop);
    let mut stopped = false;
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
            () = &mut stop, if !stopped => {
                stopped = true;
                let _ = child.start_kill();
            }
        }
    }
    for reader in readers {
        reader.abort();
    }
    let code = status.and_then(|status| status.code());
    Ok(if stopped { Exit::Stopped } else { Exit::Code(code) })
}

/// How long output is still read after a step's process has exited.
const OUTPUT_DRAIN: Duration = Duration::from_secs(2);

enum Pipe {
    Out(tokio::process::ChildStdout),
    Err(tokio::process::ChildStderr),
}

/// Sends the lines of `pipe`, decoded lossily, until it closes.
async fn read_lines(pipe: Pipe, max_line: usize, sender: tokio::sync::mpsc::Sender<String>) {
    use tokio::io::AsyncReadExt as _;
    let mut reader: Box<dyn tokio::io::AsyncRead + Unpin + Send> = match pipe {
        Pipe::Out(out) => Box::new(out),
        Pipe::Err(err) => Box::new(err),
    };
    let mut buffer = vec![0u8; 16 * 1024];
    let mut line = Vec::new();
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
                if sender.send(String::from_utf8_lossy(&std::mem::take(&mut line)).into_owned()).await.is_err() {
                    return;
                }
            } else {
                line.push(byte);
                if line.len() >= max_line && sender.send(String::from_utf8_lossy(&std::mem::take(&mut line)).into_owned()).await.is_err() {
                    return;
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

    #[tokio::test]
    async fn output_is_captured_and_bounded() {
        let Some(git) = find_program("git") else { return };
        let output = output(&git, &["--version"], Duration::from_secs(30)).await.unwrap();
        assert!(output.success && output.stdout.starts_with("git version"), "{output:?}");
    }
}
