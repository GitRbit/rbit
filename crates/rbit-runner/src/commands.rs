//! Workflow commands: `::name key=value,…::message` lines a step prints (§12.5.4
//! "Workflow commands").
//!
//! Supported are `::add-mask::`, `::group::` / `::endgroup::`, the annotations
//! `::error`, `::warning` and `::notice` (with `file`, `line`, `title` and the other
//! parameters, kept as written), `::debug::`, and `::stop-commands::<token>`, which turns
//! command processing off until a line `::<token>::`. The deprecated `::set-output`,
//! `::set-env`, `::add-path` and `::save-state` are refused with a log line pointing at
//! the env files. Anything else is printed as it came.
//!
//! Commands become log lines in the `##[kind]text` form the log viewer understands; an
//! `::add-mask::` line is not printed at all. Values are unescaped as GitHub escapes them
//! (`%25`, `%0D`, `%0A`, and in parameters also `%3A`, `%2C`).

/// One line after command processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Print this (it is masked when it is logged).
    Print(String),
    /// Mask this value from now on; print nothing.
    AddMask(String),
    /// Print nothing (a stop-commands switch).
    Nothing,
}

/// The command state of one step.
#[derive(Debug, Default)]
pub struct Commands {
    /// While set, commands are off until `::<token>::`.
    stopped: Option<String>,
}

impl Commands {
    /// What `line` amounts to.
    pub fn process(&mut self, line: &str) -> Line {
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
            "debug" => Line::Print(format!("##[debug]{message}")),
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
            "set-output" | "set-env" | "add-path" | "save-state" => {
                let file = match command.name {
                    "set-output" => "$GITHUB_OUTPUT",
                    "set-env" => "$GITHUB_ENV",
                    "add-path" => "$GITHUB_PATH",
                    _ => "$GITHUB_STATE (not supported)",
                };
                Line::Print(format!("##[error]The `{}` command is deprecated and disabled by rbit; write to {file} instead", command.name))
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
            Line::AddMask(value) => panic!("{line:?} added the mask {value:?}"),
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
    fn deprecated_commands_are_refused() {
        let mut commands = Commands::default();
        for (line, file) in
            [("::set-output name=x::1", "$GITHUB_OUTPUT"), ("::set-env name=X::1", "$GITHUB_ENV"), ("::add-path::/x", "$GITHUB_PATH")]
        {
            let text = printed(&mut commands, line);
            assert!(text.starts_with("##[error]") && text.contains("deprecated") && text.contains(file), "{text}");
        }
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
