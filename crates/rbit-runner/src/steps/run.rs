//! `run:` steps (§12.2 "Shells", §12.5.4).
//!
//! The script is interpolated as text and written to a file in the task's `temp`
//! directory; the shell gets the file's path, never the script, so nothing in it passes
//! through an argument list. The shell is the step's `shell`, else `defaults.run.shell`,
//! else the platform's default: `bash` (or `sh` without it) on Linux and macOS, `pwsh` (or
//! `powershell` without it) on Windows. Command lines are GitHub's
//! ([`Shell::command`]); PowerShell scripts stop at the first error and exit with the last
//! native exit code, as on GitHub.
//!
//! The process's output is read line by line: workflow commands are applied
//! ([`crate::commands`]), the rest is masked and logged. After the step its env files are
//! read: outputs become the step's outputs, `GITHUB_ENV` and `GITHUB_PATH` entries apply
//! to the following steps.

use std::{collections::BTreeMap, time::Duration};

use rbit_workflow::{model::Shell, spec::StepTemplate};

use super::Outcome;
use crate::{
    backend::StepCommand,
    commands::{Commands, Line},
    envfiles::{EnvFiles, Written},
    job::{Job, join},
    process::{self, Exit},
};

/// Runs `script` as step `index`.
pub async fn run(
    job: &mut Job,
    index: usize,
    step: &StepTemplate,
    script: &str,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
) -> Outcome {
    let script = match job.render(script) {
        Ok(script) => script,
        Err(error) => return job.fail(&format!("`run`: {error}")),
    };
    let defaults = job.spec.template.defaults.clone();
    let shell = match step.shell.as_ref().or(defaults.shell.as_ref()).map(|shell| job.render(shell)).transpose() {
        Ok(shell) => shell,
        Err(error) => return job.fail(&format!("`shell`: {error}")),
    };
    let directory = match step.working_directory.as_ref().or(defaults.working_directory.as_ref()).map(|dir| job.render(dir)).transpose() {
        Ok(directory) => directory,
        Err(error) => return job.fail(&format!("`working-directory`: {error}")),
    };
    let search_path = job.search_path(step_env);
    // A custom shell (`perl {0}`): run as written, with `{0}` the script's path.
    if let Some(template) = shell.as_deref().filter(|shell| shell.contains("{0}")) {
        return custom_shell(job, index, step_env, timeout, &script, template, directory.as_deref(), &search_path).await;
    }
    let (shell, program) = match choose_shell(shell.as_deref(), job.backend.windows(), |name| job.backend.find(name, &search_path)) {
        Ok(chosen) => chosen,
        Err(error) => return job.fail(&error),
    };
    let windows = job.backend.windows();
    let view = job.backend.view().clone();
    let cwd = match directory {
        Some(directory) if is_absolute(&directory, windows) => directory,
        Some(directory) => join(&view.workspace, &directory, windows),
        None => view.workspace.clone(),
    };

    job.reporter.log(&format!("##[group]Run {}", script.lines().find(|line| !line.trim().is_empty()).unwrap_or_default().trim()));
    for line in script.lines() {
        job.reporter.log(line);
    }
    job.reporter.log(&format!("shell: {}", shell.name()));
    job.reporter.log("##[endgroup]");

    let name = format!("step-{index}-{}{}", crate::client::idempotency_key(), shell.extension());
    let script_path = job.dirs.temp.join(&name);
    let written = crate::untrusted::create(&script_path, false).and_then(|mut file| {
        use std::io::Write as _;
        file.write_all(script_file(shell, &script).as_ref())
    });
    if let Err(error) = written {
        return job.fail(&format!("cannot write the script: {error}"));
    }
    let mut script_view = join(&view.temp, &name, windows);
    if windows && matches!(shell, Shell::Bash | Shell::Sh) {
        // Git for Windows' bash reads `C:/…` paths reliably, backslashes less so.
        script_view = script_view.replace('\\', "/");
    }
    let (args, raw_args) = command_line(shell, &script_view);
    let process = StepProcess { program, args, raw_args, cwd, extra: Vec::new() };
    let (outcome, _) = execute(job, index, process, step_env, timeout).await;
    let _ = std::fs::remove_file(&script_path);
    outcome
}

// Actions: third-party actions.
/// A process a step runs: a `run:` script's shell, or an action's `node`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepProcess {
    pub program: String,
    pub args: Vec<String>,
    pub raw_args: Option<String>,
    /// As the step sees it.
    pub cwd: String,
    /// Variables on top of the job's and the step's (an action's `INPUT_*`).
    pub extra: Vec<(String, String)>,
}

/// Runs `process` as (part of) step `index`, with fresh env files: its output is read for
/// workflow commands, masked and logged; afterwards `GITHUB_ENV` and `GITHUB_PATH` apply to
/// later steps, the summary is recorded, and the outcome carries `GITHUB_OUTPUT`. Also
/// returns what it wrote to `GITHUB_STATE`.
pub async fn execute(
    job: &mut Job,
    index: usize,
    process: StepProcess,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
) -> (Outcome, BTreeMap<String, String>) {
    let view = job.backend.view().clone();
    let files = match EnvFiles::create(&job.dirs.files, index) {
        Ok(files) => files,
        Err(error) => return (job.fail(&format!("cannot create the env files: {error}")), BTreeMap::new()),
    };
    let mut extra = files.variables(&view.files);
    extra.extend(process.extra);
    let command = StepCommand {
        program: process.program,
        args: process.args,
        raw_args: process.raw_args,
        env: job.process_env(step_env, extra),
        cwd: process.cwd,
    };

    let prepared = match job.backend.command(&command) {
        Ok(prepared) => prepared,
        Err(error) => {
            let _ = files.collect();
            return (job.fail(&format!("cannot prepare the step: {error}")), BTreeMap::new());
        }
    };
    let signal = job.stop_signal(timeout);
    // Actions: third-party actions. What deprecated `::set-output` / `::save-state` lines set.
    let mut printed_outputs = BTreeMap::new();
    let mut printed_state = BTreeMap::new();
    let mut matchers = std::mem::take(&mut job.matchers);
    // A container job's matcher files are read once the step is over, with its container
    // paused, so nothing in it can swap a link in while the runner reads them.
    let mut pending_matchers = Vec::new();
    let exit = {
        let reporter = &job.reporter;
        let mut commands = Commands::new(job.step_debug);
        let stopper = job.backend.stopper();
        let mut warned = false;
        let (view, dirs, host) = (job.backend.view().clone(), job.dirs.clone(), matches!(job.backend, crate::backend::Backend::Host(_)));
        process::run_streaming(prepared.command, process::MAX_LINE_BYTES, signal.wait(), &stopper, |line| match commands.process(&line) {
            Line::Print(text) => {
                reporter.log(&text);
                if let Some(annotation) = matchers.annotate(&reporter.mask(&text)) {
                    reporter.log(&annotation);
                }
            }
            Line::AddMatcher(seen) if !host => pending_matchers.push((seen, true)),
            Line::RemoveMatcher(owner) if !host => pending_matchers.push((owner, false)),
            Line::AddMatcher(seen) => match read_matcher(&seen, &view, &dirs, host).and_then(|text| matchers.add(&text)) {
                Ok(owners) => reporter.log(&format!("Added matchers: {}", owners.join(", "))),
                Err(error) => reporter.log(&format!("##[warning]cannot add the problem matcher {seen}: {error}")),
            },
            Line::RemoveMatcher(owner) => matchers.remove(&owner),
            Line::AddMask(value) => reporter.add_mask(&value),
            Line::Nothing => {}
            Line::Output(name, value) => {
                deprecated(&mut warned, reporter);
                if crate::envfiles::valid_output_name(&name) {
                    printed_outputs.insert(name, value);
                } else {
                    reporter.log(&format!("##[warning]`{}` is not a valid output name; it was ignored", name.escape_debug()));
                }
            }
            Line::State(name, value) => {
                deprecated(&mut warned, reporter);
                if crate::envfiles::valid_variable_name(&name) {
                    printed_state.insert(name, value);
                } else {
                    reporter.log(&format!("##[warning]`{}` is not a valid state name; it was ignored", name.escape_debug()));
                }
            }
        })
        .await
    };
    read_matchers_paused(job, &mut matchers, pending_matchers).await;
    job.matchers = matchers;
    // The step's env file is deleted as soon as the step has ended.
    drop(prepared.env_file);

    let written = files.collect();
    let outcome = match exit {
        Err(error) => job.fail(&format!("cannot start `{}`: {error}", command.program)),
        Ok(Exit::Stopped) => job.stopped(signal.reason()),
        Ok(Exit::Code(Some(0))) => Outcome::success(BTreeMap::new()),
        Ok(Exit::Code(Some(code))) => job.fail(&format!("Process completed with exit code {code}.")),
        Ok(Exit::Code(None)) => job.fail("The process was killed by a signal."),
    };
    match written {
        Ok(mut written) => {
            // Actions: step summaries.
            job.reporter.set_summary(index, &written.summary);
            let mut state = printed_state;
            state.append(&mut written.state);
            let mut outputs = printed_outputs;
            outputs.append(&mut apply(job, written));
            (Outcome { outputs, ..outcome }, state)
        }
        Err(error) => (job.fail(&error), BTreeMap::new()),
    }
}

/// Reads a container job's matcher files `pending` with its container paused.
async fn read_matchers_paused(job: &Job, matchers: &mut crate::matchers::Matchers, pending: Vec<(String, bool)>) {
    if pending.is_empty() {
        return;
    }
    if let Err(error) = job.backend.pause(&job.reporter).await {
        job.reporter.log(&format!("##[warning]cannot pause the job container to read its problem matchers: {error}"));
        return;
    }
    let (view, dirs) = (job.backend.view(), &job.dirs);
    // Adds (`true`) and removes, in the order the step printed them.
    for (seen, add) in pending {
        if !add {
            matchers.remove(&seen);
            continue;
        }
        match read_matcher(&seen, view, dirs, false).and_then(|text| matchers.add(&text)) {
            Ok(owners) => job.reporter.log(&format!("Added matchers: {}", owners.join(", "))),
            // The file is read on the host: what it holds is never echoed.
            Err(_) => job.reporter.log(&format!("##[warning]cannot add the problem matcher {seen}")),
        }
    }
    if job.backend.resume().await.is_err()
        && let Err(error) = job.backend.resume().await
    {
        job.reporter.log(&format!("##[error]cannot resume the job container: {error}"));
    }
}

/// Reads the matcher file a step named `seen`, as the runner host finds it: a path in the
/// job container's workspace, `temp` or `files` maps to the task's directory, a host job's
/// is its own. Read as untrusted (no link, pipe or device), and at most 1 MiB.
fn read_matcher(seen: &str, view: &crate::backend::View, dirs: &crate::backend::Dirs, host: bool) -> Result<String, String> {
    use std::io::Read as _;
    let mapped =
        [(&view.workspace, &dirs.workspace), (&view.temp, &dirs.temp), (&view.files, &dirs.files)].into_iter().find_map(|(view, dir)| {
            let rest = seen.strip_prefix(view.as_str())?;
            let rest = std::path::Path::new(rest.trim_start_matches(['/', '\\']));
            // Plain names only: no `..`, root or drive (`C:` would replace `dir` on Windows).
            (rest.components().all(|part| matches!(part, std::path::Component::Normal(_))) && !crate::untrusted::crosses_link(dir, rest))
                .then(|| dir.join(rest))
        });
    let path = match mapped {
        Some(path) => path,
        None if host => std::path::PathBuf::from(seen),
        None => return Err("a container job's matcher must be in its workspace or temp directory".to_owned()),
    };
    let mut text = String::new();
    crate::untrusted::open(&path).and_then(|file| file.take(1 << 20).read_to_string(&mut text)).map_err(|error| error.to_string())?;
    Ok(text)
}

/// Runs `script` with a custom shell `template` (`perl {0}`), as GitHub does: the script is
/// written as it is to a file without an extension, and the command runs with `{0}` its path.
/// The program is found on the step's `PATH` on the host, and by the container's own.
#[allow(clippy::too_many_arguments)] // the parts of one step
async fn custom_shell(
    job: &mut Job,
    index: usize,
    step_env: &BTreeMap<String, String>,
    timeout: Option<Duration>,
    script: &str,
    template: &str,
    directory: Option<&str>,
    search_path: &str,
) -> Outcome {
    let words = match rbit_workflow::options::split(template) {
        Ok(words) if words.first().is_some_and(|program| !program.contains("{0}")) => words,
        Ok(_) => return job.fail(&format!("`shell: {template}` must start with the program that runs the script")),
        Err(error) => return job.fail(&format!("`shell: {template}`: {error}")),
    };
    let windows = job.backend.windows();
    let view = job.backend.view().clone();
    let cwd = match directory {
        Some(directory) if is_absolute(directory, windows) => directory.to_owned(),
        Some(directory) => join(&view.workspace, directory, windows),
        None => view.workspace.clone(),
    };
    job.reporter.log(&format!("##[group]Run {}", script.lines().find(|line| !line.trim().is_empty()).unwrap_or_default().trim()));
    for line in script.lines() {
        job.reporter.log(line);
    }
    job.reporter.log(&format!("shell: {template}"));
    job.reporter.log("##[endgroup]");
    let name = format!("step-{index}-{}", crate::client::idempotency_key());
    let script_path = job.dirs.temp.join(&name);
    let written = crate::untrusted::create(&script_path, false).and_then(|mut file| {
        use std::io::Write as _;
        file.write_all(script.as_bytes())
    });
    if let Err(error) = written {
        return job.fail(&format!("cannot write the script: {error}"));
    }
    let script_view = join(&view.temp, &name, windows);
    let program = match &job.backend {
        crate::backend::Backend::Host(_) => crate::process::find_shell(&words[0], search_path)
            .map_or_else(|| words[0].clone(), |found| found.to_string_lossy().into_owned()),
        crate::backend::Backend::Container(_) => words[0].clone(),
    };
    let args = words[1..].iter().map(|word| word.replace("{0}", &script_view)).collect();
    let process = StepProcess { program, args, raw_args: None, cwd, extra: Vec::new() };
    let (outcome, _) = execute(job, index, process, step_env, timeout).await;
    let _ = std::fs::remove_file(&script_path);
    outcome
}

// Actions: third-party actions.
/// Warns once per step that `::set-output` / `::save-state` are deprecated, as GitHub does.
fn deprecated(warned: &mut bool, reporter: &crate::task::Reporter) {
    if !std::mem::replace(warned, true) {
        reporter.log(
            "##[warning]The `set-output` and `save-state` commands are deprecated and will be disabled; write to $GITHUB_OUTPUT and $GITHUB_STATE instead",
        );
    }
}

/// Applies what a step wrote to `GITHUB_ENV` and `GITHUB_PATH`, and returns its outputs.
pub(crate) fn apply(job: &mut Job, written: Written) -> BTreeMap<String, String> {
    for name in &written.refused {
        job.reporter.log(&format!("##[warning]GITHUB_ENV cannot set `{name}`; the entry was ignored"));
    }
    for (name, value) in written.env {
        job.env.insert(name, value);
    }
    for entry in written.path {
        job.path.insert(0, entry);
    }
    job.sync_contexts();
    written.outputs
}

/// The shell to use and the program that runs it.
pub fn choose_shell(name: Option<&str>, windows: bool, find: impl Fn(&str) -> Option<String>) -> Result<(Shell, String), String> {
    let shell = match name {
        Some(name) => Shell::parse(name).ok_or_else(|| format!("`{name}` is not a shell rbit supports"))?,
        None if windows => {
            if find("pwsh").is_some() {
                Shell::Pwsh
            } else {
                Shell::Powershell
            }
        }
        None => {
            if find("bash").is_some() {
                Shell::Bash
            } else {
                Shell::Sh
            }
        }
    };
    if shell == Shell::Cmd && !windows {
        return Err("`cmd` is only available on Windows".to_owned());
    }
    let candidates: &[&str] = match shell {
        Shell::Python => &["python", "python3"],
        other => &[other.name()],
    };
    candidates
        .iter()
        .find_map(|candidate| find(candidate))
        .map(|program| (shell, program))
        .ok_or_else(|| format!("the `{}` shell is not installed where this job runs", shell.name()))
}

/// The script file's contents: PowerShell scripts stop at the first error and pass on the
/// last native exit code, and get a byte order mark so Windows PowerShell reads UTF-8;
/// `cmd` scripts get CRLF line endings.
pub fn script_file(shell: Shell, script: &str) -> Vec<u8> {
    match shell {
        Shell::Pwsh | Shell::Powershell => {
            let text = format!(
                "$ErrorActionPreference = 'stop'\n{script}\nif ((Test-Path -LiteralPath variable:\\LASTEXITCODE)) {{ exit $LASTEXITCODE }}\n"
            );
            [b"\xEF\xBB\xBF".as_slice(), text.as_bytes()].concat()
        }
        Shell::Cmd => script.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes(),
        _ => script.as_bytes().to_vec(),
    }
}

/// The arguments after the program, as in [`Shell::command`] with `{0}` the script, and
/// the part `cmd` needs written as is.
pub fn command_line(shell: Shell, script: &str) -> (Vec<String>, Option<String>) {
    match shell {
        Shell::Bash => (["--noprofile", "--norc", "-eo", "pipefail", script].map(str::to_owned).to_vec(), None),
        Shell::Sh => (vec!["-e".to_owned(), script.to_owned()], None),
        // GitHub's command line, plus what lets a script file run at all on a Windows host
        // with the default `Restricted` policy, without a profile or prompts.
        Shell::Pwsh | Shell::Powershell => (
            ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-command"]
                .map(str::to_owned)
                .into_iter()
                .chain([format!(". '{}'", script.replace('\'', "''"))])
                .collect(),
            None,
        ),
        Shell::Cmd => (Vec::new(), Some(format!("/D /E:ON /V:OFF /S /C \"CALL \"{script}\"\""))),
        Shell::Python => (vec![script.to_owned()], None),
    }
}

fn is_absolute(path: &str, windows: bool) -> bool {
    if windows {
        let bytes = path.as_bytes();
        path.starts_with("\\\\") || (bytes.len() >= 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
    } else {
        path.starts_with('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_are_githubs() {
        for shell in Shell::ALL {
            let (args, raw) = command_line(shell, "/tmp/x");
            let quoted: Vec<String> = args.iter().map(|arg| if arg.contains(' ') { format!("\"{arg}\"") } else { arg.clone() }).collect();
            let line = [shell.name().to_owned()].into_iter().chain(quoted).chain(raw).collect::<Vec<_>>().join(" ");
            let line = line.replace(" -NoProfile -NonInteractive -ExecutionPolicy Bypass", "");
            assert_eq!(line, shell.command().replace("{0}", "/tmp/x"), "{shell:?}");
        }
        assert_eq!(command_line(Shell::Pwsh, "C:\\it's.ps1").0[5], ". 'C:\\it''s.ps1'");
    }

    #[test]
    fn shells_default_per_platform_and_must_exist() {
        let all = |name: &str| Some(format!("/bin/{name}"));
        let only = |names: &'static [&'static str]| move |name: &str| names.contains(&name).then(|| format!("/bin/{name}"));
        assert_eq!(choose_shell(None, false, all).unwrap(), (Shell::Bash, "/bin/bash".into()));
        assert_eq!(choose_shell(None, false, only(&["sh"])).unwrap().0, Shell::Sh);
        assert_eq!(choose_shell(None, true, all).unwrap().0, Shell::Pwsh);
        assert_eq!(choose_shell(None, true, only(&["powershell"])).unwrap().0, Shell::Powershell);
        assert_eq!(choose_shell(Some("python"), false, only(&["python3"])).unwrap(), (Shell::Python, "/bin/python3".into()));
        assert!(choose_shell(Some("cmd"), false, all).unwrap_err().contains("only available on Windows"));
        assert!(choose_shell(Some("fish"), false, all).is_err());
        assert!(choose_shell(Some("bash"), false, only(&["sh"])).unwrap_err().contains("not installed"));
    }

    #[test]
    fn script_files_suit_their_shell() {
        let pwsh = script_file(Shell::Pwsh, "Write-Output hi");
        assert!(pwsh.starts_with(b"\xEF\xBB\xBF$ErrorActionPreference = 'stop'\n"));
        assert!(String::from_utf8_lossy(&pwsh).contains("exit $LASTEXITCODE"));
        assert_eq!(script_file(Shell::Cmd, "echo a\necho b"), b"echo a\r\necho b");
        assert_eq!(script_file(Shell::Bash, "echo a\n"), b"echo a\n");
        assert!(is_absolute("C:\\x", true) && is_absolute("/x", false) && !is_absolute("x/y", false) && !is_absolute("x", true));
    }
}
