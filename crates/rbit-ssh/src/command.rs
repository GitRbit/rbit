//! Parsing the command a client asks the server to run.
//!
//! Git over SSH runs `ssh git@host "git-upload-pack 'owner/repo.git'"` (or
//! `git-receive-pack`). The command line is *never* given to a shell: it is parsed
//! strictly here into a [`Service`] and an `owner/repo` pair, and anything else is
//! refused. The repository path on disk comes from the database, never from this string.
//!
//! Accepted repository spellings: `owner/repo`, `owner/repo.git`, each optionally with a
//! leading `/` (from `ssh://` URLs), optionally in single or double quotes.
//!
//! Git LFS asks for HTTP credentials with `git-lfs-authenticate <repo> <upload|download>`
//! ([`parse_lfs_authenticate`]); the same repository spellings apply.

use std::fmt;

use rbit_core::lfs::{LfsOid, LfsOperation};
use rbit_transport::Service;

/// Longest command accepted; real ones are short.
const MAX_COMMAND_LEN: usize = 512;
/// Longest single path segment (owner or repository name).
const MAX_SEGMENT_LEN: usize = 100;

/// A Git command the server is willing to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommand {
    pub service: Service,
    pub owner: String,
    /// The repository name without any `.git` suffix.
    pub repo: String,
}

impl GitCommand {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// `git-lfs-authenticate`: the client wants the LFS endpoint and a header for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsAuthenticate {
    pub owner: String,
    /// The repository name without any `.git` suffix.
    pub repo: String,
    pub operation: LfsOperation,
}

impl LfsAuthenticate {
    /// The Git command whose access rules apply: fetching for downloads, pushing for uploads.
    pub fn as_git_command(&self) -> GitCommand {
        let service = if self.operation.is_write() { Service::ReceivePack } else { Service::UploadPack };
        GitCommand { service, owner: self.owner.clone(), repo: self.repo.clone() }
    }
}

/// Why a command was refused; the message is shown to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandError {
    /// Not a Git command at all (e.g. `ls`, `bash`, `git-upload-archive`).
    Unsupported,
    /// A Git command whose repository argument is malformed.
    BadPath,
    /// `git-lfs-authenticate` with an operation other than `upload` or `download`.
    BadLfsOperation,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => {
                f.write_str("only Git commands are available over SSH (git clone, fetch, pull and push); there is no shell access")
            }
            Self::BadPath => f.write_str("the repository must be given as owner/repository, e.g. git@host:alice/project.git"),
            Self::BadLfsOperation => f.write_str("git-lfs-authenticate needs the operation: upload or download"),
        }
    }
}

/// Parses an `exec` request's command line.
pub fn parse(command: &[u8]) -> Result<GitCommand, CommandError> {
    if command.len() > MAX_COMMAND_LEN {
        return Err(CommandError::Unsupported);
    }
    let command = std::str::from_utf8(command).map_err(|_| CommandError::Unsupported)?;
    let (program, argument) = command.trim().split_once(' ').ok_or(CommandError::Unsupported)?;
    let service = Service::from_wire_name(program).ok_or(CommandError::Unsupported)?;
    let (owner, repo) = parse_path(argument.trim()).ok_or(CommandError::BadPath)?;
    Ok(GitCommand { service, owner: owner.to_owned(), repo: repo.to_owned() })
}

/// Parses `git-lfs-authenticate <repo> <operation> [<oid>]` (old clients add the object
/// id, which is ignored). `None` when the command is not `git-lfs-authenticate` at all.
pub fn parse_lfs_authenticate(command: &[u8]) -> Option<Result<LfsAuthenticate, CommandError>> {
    if command.len() > MAX_COMMAND_LEN {
        return None;
    }
    let command = std::str::from_utf8(command).ok()?;
    let mut words = command.split_whitespace();
    if words.next()? != "git-lfs-authenticate" {
        return None;
    }
    let parsed = (|| {
        let (owner, repo) = words.next().and_then(parse_path).ok_or(CommandError::BadPath)?;
        let operation = words.next().and_then(LfsOperation::parse).ok_or(CommandError::BadLfsOperation)?;
        match (words.next(), words.next()) {
            (None, None) => {}
            (Some(oid), None) if LfsOid::parse(oid).is_ok() => {}
            _ => return Err(CommandError::Unsupported),
        }
        Ok(LfsAuthenticate { owner: owner.to_owned(), repo: repo.to_owned(), operation })
    })();
    Some(parsed)
}

/// `'owner/repo.git'` → `("owner", "repo")`.
fn parse_path(argument: &str) -> Option<(&str, &str)> {
    let path = unquote(argument)?;
    let path = path.strip_prefix('/').unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    (is_segment(owner) && is_segment(repo)).then_some((owner, repo))
}

fn unquote(argument: &str) -> Option<&str> {
    for quote in ['\'', '"'] {
        if let Some(inner) = argument.strip_prefix(quote) {
            return inner.strip_suffix(quote);
        }
    }
    Some(argument)
}

/// Letters, digits, `-`, `_` and `.`, not starting with `.` (so never `.` or `..`) or `-`.
fn is_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= MAX_SEGMENT_LEN
        && !segment.starts_with(['.', '-'])
        && segment.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(command: &str) -> GitCommand {
        parse(command.as_bytes()).unwrap_or_else(|error| panic!("{command:?}: {error}"))
    }

    #[test]
    fn accepts_the_spellings_git_uses() {
        let expected = GitCommand { service: Service::UploadPack, owner: "alice".into(), repo: "my-project".into() };
        for command in [
            "git-upload-pack 'alice/my-project.git'",
            "git-upload-pack '/alice/my-project.git'",
            "git-upload-pack 'alice/my-project'",
            "git-upload-pack \"/alice/my-project\"",
            "git-upload-pack alice/my-project.git",
            "  git-upload-pack   'alice/my-project.git'  ",
        ] {
            assert_eq!(ok(command), expected, "{command:?}");
        }
        assert_eq!(ok("git-receive-pack 'alice/my.project.git'").service, Service::ReceivePack);
        assert_eq!(ok("git-receive-pack 'alice/my.project.git'").repo, "my.project");
    }

    #[test]
    fn refuses_anything_that_is_not_a_git_service() {
        for command in ["", "ls", "bash -c id", "git-upload-archive 'alice/p.git'", "git upload-pack 'alice/p.git'", "git-upload-pack"] {
            assert_eq!(parse(command.as_bytes()), Err(CommandError::Unsupported), "{command:?}");
        }
        assert_eq!(parse(&[0xff, 0xfe]), Err(CommandError::Unsupported));
        assert_eq!(parse("git-upload-pack 'a/b'".repeat(100).as_bytes()), Err(CommandError::Unsupported));
    }

    #[test]
    fn parses_git_lfs_authenticate() {
        let parse = |command: &str| parse_lfs_authenticate(command.as_bytes());
        let expected = LfsAuthenticate { owner: "alice".into(), repo: "assets".into(), operation: LfsOperation::Download };
        assert_eq!(parse("git-lfs-authenticate 'alice/assets.git' download"), Some(Ok(expected.clone())));
        assert_eq!(parse("git-lfs-authenticate /alice/assets download"), Some(Ok(expected.clone())));
        let with_oid = format!("git-lfs-authenticate alice/assets.git download {}", "a".repeat(64));
        assert_eq!(parse(&with_oid), Some(Ok(expected)));
        let upload = parse("git-lfs-authenticate alice/assets upload").unwrap().unwrap();
        assert_eq!(upload.as_git_command().service, Service::ReceivePack);
        assert_eq!(upload.as_git_command().full_name(), "alice/assets");

        assert_eq!(parse("git-upload-pack 'alice/assets.git'"), None, "not an LFS command");
        assert_eq!(parse("git-lfs-authenticate '../x' download"), Some(Err(CommandError::BadPath)));
        assert_eq!(parse("git-lfs-authenticate alice/assets"), Some(Err(CommandError::BadLfsOperation)));
        assert_eq!(parse("git-lfs-authenticate alice/assets delete"), Some(Err(CommandError::BadLfsOperation)));
        assert_eq!(parse("git-lfs-authenticate alice/assets download; rm -rf /"), Some(Err(CommandError::BadLfsOperation)));
        assert_eq!(parse("git-lfs-authenticate alice/assets download x y"), Some(Err(CommandError::Unsupported)));
    }

    #[test]
    fn refuses_paths_that_could_escape_or_inject() {
        for path in [
            "'alice'",
            "'alice/'",
            "'/repo.git'",
            "'alice/../bob/p.git'",
            "'../etc/passwd'",
            "'alice/p.git'; rm -rf /",
            "'alice/p.git' --upload-pack=touch",
            "'alice/p$(id).git'",
            "'alice/p.git",
            "'alice/.git'",
            "'alice/..'",
            "'a/b/c'",
            "'alice\\p'",
            "'-alice/p'",
            "'~alice/p'",
        ] {
            let command = format!("git-upload-pack {path}");
            assert_eq!(parse(command.as_bytes()), Err(CommandError::BadPath), "{command:?}");
        }
    }
}
