//! Workflow commands: `::name key=value,…::message` lines a step prints (§12.5.4
//! "Workflow commands").
//!
//! Supported are `::add-mask::`, `::group::` / `::endgroup::`, the annotations
//! `::error`, `::warning` and `::notice` (with `file`, `line`, `title` and the other
//! parameters, kept as written), `::debug::`, and `::stop-commands::<token>`, which turns
//! command processing off until a line `::<token>::`. As on GitHub, `::debug::` lines are
//! printed only when the `ACTIONS_STEP_DEBUG` secret or variable is `true`, and
//! `::add-matcher::` / `::remove-matcher owner=…::` (and their legacy `##[add-matcher]`
//! forms) add and remove problem matchers ([`crate::matchers`]). The deprecated `::set-output` and `::save-state`, which older actions
//! still print, work with a warning; `::set-env` and `::add-path`, which GitHub disabled
//! because they let any printed line change the environment, are refused with a log line
//! pointing at the env files. Anything else is printed as it came.
//!
//! Commands become log lines in the `##[kind]text` form the log viewer understands; an
//! `::add-mask::` line is not printed at all. Values are unescaped as GitHub escapes them
//! (`%25`, `%0D`, `%0A`, and in parameters also `%3A`, `%2C`).

/// The `owner` of `owner=…` parameters.
fn owner_param(params: &str) -> String {
    params.split(',').find_map(|param| param.trim().strip_prefix("owner=")).unwrap_or_default().trim().to_owned()
}

/// One line after command processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Print this (it is masked when it is logged).
    Print(String),
    /// Mask this value from now on; print nothing.
    AddMask(String),
    /// Print nothing (a stop-commands switch).
    Nothing,
    // Actions: third-party actions.
    /// `::set-output name=…::`: an output of the step.
    Output(String, String),
    /// `::save-state name=…::`: state for the action's `post` script.
    State(String, String),
    /// `::add-matcher::<file>`: the matcher file, as the step names it.
    AddMatcher(String),
    /// `::remove-matcher owner=…::`: the owner.
    RemoveMatcher(String),
}

/// The command state of one step.
#[derive(Debug, Default)]
pub struct Commands {
    /// While set, commands are off until `::<token>::`.
    stopped: Option<String>,
    // Actions: third-party actions.
    /// Whether `::debug::` lines are printed (`ACTIONS_STEP_DEBUG`).
    debug: bool,
}

impl Commands {
    /// Commands of a step, printing `::debug::` lines when `debug` is set.
    pub fn new(debug: bool) -> Self {
        Self { stopped: None, debug }
    }

    /// What `line` amounts to.
    pub fn process(&mut self, line: &str) -> Line {
        // Actions: third-party actions. The legacy `##[add-matcher]path` form, which
        // `actions/setup-node` still prints.
        let legacy = line.trim_start();
        if self.stopped.is_none() {
            if let Some(path) = legacy.strip_prefix("##[add-matcher]") {
                return Line::AddMatcher(path.trim().to_owned());
            }
            if let Some(rest) = legacy.strip_prefix("##[remove-matcher") {
                return Line::RemoveMatcher(owner_param(rest.trim_end_matches(']')));
            }
        }
        let Some(command) = parse(line) else { return Line::Print(line.to_owned()) };
        if let Some(token) = &self.stopped {
            if command.name == token.as_str() && command.params.is_none() {
                self.stopped = None;
                return Line::Nothing;
            }
            return Line::Print(line.to_owned());
        }
        let message = unescape(command.message, false);
        match command.name {
            "add-mask" => Line::AddMask(message),
            "group" => Line::Print(format!("##[group]{message}")),
            "endgroup" => Line::Print("##[endgroup]".to_owned()),
            "debug" if self.debug => Line::Print(format!("##[debug]{message}")),
            "debug" => Line::Nothing,
            "add-matcher" => Line::AddMatcher(message),
            "remove-matcher" => Line::RemoveMatcher(owner_param(&command.params.map(|params| unescape(params, true)).unwrap_or_default())),
            "error" | "warning" | "notice" => {
                let params = command.params.map(|params| unescape(params, true)).filter(|params| !params.is_empty());
                match params {
                    Some(params) => Line::Print(format!("##[{}]{message} ({params})", command.name)),
                    None => Line::Print(format!("##[{}]{message}", command.name)),
                }
            }
            "stop-commands" if !message.is_empty() => {
                self.stopped = Some(message);
                Line::Nothing
            }
            "set-output" | "save-state" => {
                let name = command
                    .params
                    .and_then(|params| params.split(',').find_map(|param| param.trim().strip_prefix("name=")))
                    .map(|name| unescape(name, true))
                    .filter(|name| !name.is_empty());
                match (name, command.name) {
                    (Some(name), "set-output") => Line::Output(name, message),
                    (Some(name), _) => Line::State(name, message),
                    (None, _) => Line::Print(format!("##[error]`{}` needs a `name`", command.name)),
                }
            }
            "set-env" | "add-path" => {
                let file = if command.name == "set-env" { "$GITHUB_ENV" } else { "$GITHUB_PATH" };
                Line::Print(format!("##[error]The `{}` command is disabled, as on GitHub; write to {file} instead", command.name))
            }
            _ => Line::Print(line.to_owned()),
        }
    }
}

#[derive(Debug)]
struct Command<'a> {
    name: &'a str,
    params: Option<&'a str>,
    message: &'a str,
}

/// `::name params::message`, after leading whitespace.
fn parse(line: &str) -> Option<Command<'_>> {
    let rest = line.trim_start().strip_prefix("::")?;
    let (head, message) = rest.split_once("::")?;
    let (name, params) = match head.split_once(' ') {
        Some((name, params)) => (name, Some(params)),
        None => (head, None),
    };
    (!name.is_empty() && !name.contains(char::is_whitespace)).then_some(Command { name, params, message })
}

fn unescape(text: &str, param: bool) -> String {
    let mut out = text.replace("%0D", "\r").replace("%0A", "\n");
    if param {
        out = out.replace("%3A", ":").replace("%2C", ",");
    }
    out.replace("%25", "%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn printed(commands: &mut Commands, line: &str) -> String {
        match commands.process(line) {
            Line::Print(text) => text,
            Line::Nothing => String::new(),
            other => panic!("{line:?} is {other:?}"),
        }
    }

    #[test]
    fn commands_become_log_lines() {
        let mut commands = Commands::default();
        assert_eq!(commands.process("::add-mask::hunter%252"), Line::AddMask("hunter%2".into()));
        assert_eq!(printed(&mut commands, "::group::Build %0Aall"), "##[group]Build \nall");
        assert_eq!(printed(&mut commands, "::endgroup::"), "##[endgroup]");
        assert_eq!(printed(&mut commands, "::warning file=a.rs,line=3::careful"), "##[warning]careful (file=a.rs,line=3)");
        assert_eq!(printed(&mut commands, "  ::error::broken"), "##[error]broken");
        assert_eq!(printed(&mut commands, "::notice title=a%3Ab::fyi"), "##[notice]fyi (title=a:b)");
        assert_eq!(printed(&mut commands, "plain ::text::"), "plain ::text::");
        assert_eq!(printed(&mut commands, "::unknown-thing::x"), "::unknown-thing::x");
    }

    #[test]
    fn disabled_commands_are_refused_and_deprecated_ones_work() {
        let mut commands = Commands::default();
        for (line, file) in [("::set-env name=X::1", "$GITHUB_ENV"), ("::add-path::/x", "$GITHUB_PATH")] {
            let text = printed(&mut commands, line);
            assert!(text.starts_with("##[error]") && text.contains("disabled") && text.contains(file), "{text}");
        }
        assert_eq!(commands.process("::set-output name=answer::4%0A2"), Line::Output("answer".into(), "4\n2".into()));
        assert_eq!(commands.process("::save-state name=pid::42"), Line::State("pid".into(), "42".into()));
        assert!(printed(&mut commands, "::set-output::x").contains("needs a `name`"));
    }

    #[test]
    fn debug_lines_and_matchers_print_only_when_asked() {
        let mut quiet = Commands::default();
        assert_eq!(quiet.process("::debug::details"), Line::Nothing);
        assert_eq!(quiet.process("::add-matcher::/x/tsc.json"), Line::AddMatcher("/x/tsc.json".into()));
        assert_eq!(quiet.process("::remove-matcher owner=tsc::"), Line::RemoveMatcher("tsc".into()));
        assert_eq!(quiet.process("##[add-matcher]C:\\x\\tsc.json"), Line::AddMatcher("C:\\x\\tsc.json".into()), "the legacy form");
        assert_eq!(quiet.process("##[remove-matcher owner=tsc]"), Line::RemoveMatcher("tsc".into()));
        let mut loud = Commands::new(true);
        assert_eq!(printed(&mut loud, "::debug::details"), "##[debug]details");
    }

    #[test]
    fn stop_commands_pause_processing_until_the_token() {
        let mut commands = Commands::default();
        assert_eq!(printed(&mut commands, "::stop-commands::pause-123"), "");
        assert_eq!(printed(&mut commands, "::add-mask::not-a-mask"), "::add-mask::not-a-mask");
        assert_eq!(printed(&mut commands, "::group::shown as is"), "::group::shown as is");
        assert_eq!(printed(&mut commands, "::pause-123::"), "");
        assert_eq!(commands.process("::add-mask::now"), Line::AddMask("now".into()));
    }
}
