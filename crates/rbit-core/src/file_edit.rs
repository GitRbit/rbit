//! Editing files in the browser: the paths a web edit may write, how submitted text is
//! turned into file content, and the defaults of the commit form.
//!
//! Milestone 3 workstream E (editing files in the browser) owns this file.
//!
//! # Paths
//!
//! A [`FilePath`] is a repository-relative path validated once, at the boundary: `/`
//! separated, no empty, `.` or `..` component, no `.git` component (nor the spellings
//! Windows treats as `.git`: `.git.`, `.git `, `git~1`), not absolute, no NUL, control
//! characters or backslashes, at most [`MAX_COMPONENT_BYTES`] per component and
//! [`MAX_PATH_BYTES`] in all. Git refuses some of these anyway; refusing them here gives
//! the user a clear message and keeps every later layer simple.
//!
//! # Line endings
//!
//! Browsers submit a `<textarea>` with every line break as CRLF, whatever the file used.
//! [`prepare_text`] therefore first turns every CRLF (and lone CR) into LF, then writes
//! the file in the style the file already had ([`LineEnding::detect`]: CRLF when at least
//! as many lines end in CRLF as in a bare LF), LF for new files. Nothing else changes: no
//! trailing newline is added or removed, so the file ends exactly as the user typed it.

use std::fmt;

/// Longest path a web edit may write, in bytes.
pub const MAX_PATH_BYTES: usize = 1024;
/// Longest single path component (file or directory name), in bytes.
pub const MAX_COMPONENT_BYTES: usize = 255;
/// Longest commit title, in characters.
pub const MAX_TITLE_CHARS: usize = 255;
/// Longest commit description, in characters.
pub const MAX_DESCRIPTION_CHARS: usize = 65_536;

/// Why a path was refused. Messages are safe to show.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FilePathError {
    #[error("enter a file name")]
    Empty,
    #[error("paths can be at most {MAX_PATH_BYTES} bytes long")]
    TooLong,
    #[error("file and directory names can be at most {MAX_COMPONENT_BYTES} bytes long")]
    ComponentTooLong,
    #[error("paths are relative to the repository root and cannot start with /")]
    Absolute,
    #[error("paths cannot contain empty parts (// or a trailing /)")]
    EmptyComponent,
    #[error("paths cannot contain . or .. parts")]
    Relative,
    #[error("paths cannot contain a .git part")]
    GitDirectory,
    #[error("paths cannot contain backslashes or control characters")]
    Character,
}

/// A validated repository-relative file path (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FilePath(String);

impl FilePath {
    /// Validates `path` (surrounding whitespace is ignored).
    pub fn parse(path: &str) -> Result<Self, FilePathError> {
        let path = path.trim();
        if path.is_empty() {
            return Err(FilePathError::Empty);
        }
        if path.len() > MAX_PATH_BYTES {
            return Err(FilePathError::TooLong);
        }
        if path.starts_with('/') {
            return Err(FilePathError::Absolute);
        }
        if path.chars().any(|c| c == '\\' || c.is_control()) {
            return Err(FilePathError::Character);
        }
        for component in path.split('/') {
            validate_component(component)?;
        }
        Ok(Self(path.to_owned()))
    }

    /// `name` inside directory `directory` (empty: the root); both are validated.
    pub fn join(directory: &str, name: &str) -> Result<Self, FilePathError> {
        let directory = directory.trim().trim_end_matches('/');
        if directory.is_empty() { Self::parse(name) } else { Self::parse(&format!("{directory}/{}", name.trim())) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The last component.
    pub fn file_name(&self) -> &str {
        self.0.rsplit_once('/').map_or(self.0.as_str(), |(_, name)| name)
    }

    /// The directory holding the file (empty at the root).
    pub fn parent(&self) -> &str {
        self.0.rsplit_once('/').map_or("", |(parent, _)| parent)
    }

    /// Every proper ancestor directory, outermost first: `a/b/c` → `a`, `a/b`.
    pub fn ancestors(&self) -> impl Iterator<Item = &str> {
        self.0.match_indices('/').map(|(at, _)| &self.0[..at])
    }
}

impl fmt::Display for FilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn validate_component(component: &str) -> Result<(), FilePathError> {
    if component.is_empty() {
        return Err(FilePathError::EmptyComponent);
    }
    if component == "." || component == ".." {
        return Err(FilePathError::Relative);
    }
    if component.len() > MAX_COMPONENT_BYTES {
        return Err(FilePathError::ComponentTooLong);
    }
    // Windows ignores trailing dots and spaces and knows `.git` by its short name too.
    let windows_name = component.trim_end_matches(['.', ' ']);
    if windows_name.eq_ignore_ascii_case(".git") || component.eq_ignore_ascii_case("git~1") {
        return Err(FilePathError::GitDirectory);
    }
    Ok(())
}

/// A file name as uploaded: browsers may send a full path (`C:\Users\…\notes.txt`), of
/// which only the last component counts. Empty when nothing usable is left.
pub fn uploaded_file_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or_default().trim()
}

/// How a text file ends its lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    /// The style of existing content: CRLF when at least as many lines end in CRLF as in
    /// a bare LF (and some do), LF otherwise.
    pub fn detect(content: &[u8]) -> Self {
        let (mut crlf, mut lf) = (0_usize, 0_usize);
        for (at, byte) in content.iter().enumerate() {
            if *byte == b'\n' {
                if at > 0 && content[at - 1] == b'\r' {
                    crlf += 1;
                } else {
                    lf += 1;
                }
            }
        }
        if crlf > 0 && crlf >= lf { Self::CrLf } else { Self::Lf }
    }
}

/// The bytes to commit for `submitted` (a textarea's value) in the `style` of the file
/// (see the module docs): line breaks normalised, nothing added or removed.
pub fn prepare_text(submitted: &str, style: LineEnding) -> Vec<u8> {
    let unified = submitted.replace("\r\n", "\n").replace('\r', "\n");
    match style {
        LineEnding::Lf => unified.into_bytes(),
        LineEnding::CrLf => unified.replace('\n', "\r\n").into_bytes(),
    }
}

/// What a web edit does, for the default commit title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditAction<'a> {
    Create(&'a str),
    Update(&'a str),
    Rename { from: &'a str, to: &'a str },
    Delete(&'a str),
    Upload,
}

impl EditAction<'_> {
    /// The commit title used when the user leaves it empty.
    pub fn default_title(self) -> String {
        match self {
            Self::Create(path) => format!("Create {path}"),
            Self::Update(path) => format!("Update {path}"),
            Self::Rename { from, to } => format!("Rename {from} to {to}"),
            Self::Delete(path) => format!("Delete {path}"),
            Self::Upload => "Add files via upload".to_owned(),
        }
    }
}

/// The commit message from the form's title (the default when empty) and description:
/// title, blank line, description; one trailing newline. Errors are safe to show.
pub fn commit_message(title: &str, description: &str, action: EditAction<'_>) -> Result<String, String> {
    let title = title.trim();
    let title = if title.is_empty() { action.default_title() } else { title.to_owned() };
    if title.contains(['\n', '\r']) {
        return Err("the commit title must be a single line".to_owned());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("the commit title can be at most {MAX_TITLE_CHARS} characters long"));
    }
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(format!("the description can be at most {MAX_DESCRIPTION_CHARS} characters long"));
    }
    let description = description.replace("\r\n", "\n");
    Ok(crate::pull::commit_message(&title, &description))
}

/// The suggested name of a new branch for `username`'s change: the first
/// `{username}-patch-{n}` (from 1) that `taken` says is free.
pub fn patch_branch_name(username: &str, taken: impl Fn(&str) -> bool) -> String {
    (1_u32..=10_000).map(|n| format!("{username}-patch-{n}")).find(|name| !taken(name)).unwrap_or_else(|| format!("{username}-patch"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_paths_are_kept_and_split() {
        let path = FilePath::parse(" docs/guide/intro.md ").unwrap();
        assert_eq!(path.as_str(), "docs/guide/intro.md");
        assert_eq!((path.file_name(), path.parent()), ("intro.md", "docs/guide"));
        assert_eq!(path.ancestors().collect::<Vec<_>>(), ["docs", "docs/guide"]);
        let root = FilePath::parse("README").unwrap();
        assert_eq!((root.file_name(), root.parent(), root.ancestors().count()), ("README", "", 0));
        for fine in [".gitignore", ".github/workflows/ci.yml", "a b/c d.txt", "..rc", "ünïcödé.txt", ".gitmodules", "x.git"] {
            assert!(FilePath::parse(fine).is_ok(), "{fine}");
        }
        assert_eq!(FilePath::join("docs/", "a.md").unwrap().as_str(), "docs/a.md");
        assert_eq!(FilePath::join("", "a.md").unwrap().as_str(), "a.md");
        assert_eq!(FilePath::join("docs", "sub/a.md").unwrap().as_str(), "docs/sub/a.md");
    }

    #[test]
    fn dangerous_paths_are_refused() {
        let refused = |path: &str| FilePath::parse(path).unwrap_err();
        assert_eq!(refused(""), FilePathError::Empty);
        assert_eq!(refused("   "), FilePathError::Empty);
        assert_eq!(refused("/etc/passwd"), FilePathError::Absolute);
        assert_eq!(refused("../outside"), FilePathError::Relative);
        assert_eq!(refused("a/../../b"), FilePathError::Relative);
        assert_eq!(refused("./a"), FilePathError::Relative);
        assert_eq!(refused("a//b"), FilePathError::EmptyComponent);
        assert_eq!(refused("a/"), FilePathError::EmptyComponent);
        for git in [".git/config", "a/.GIT/hooks/x", ".git", "a/.git./b", ".git /x", "GIT~1/config"] {
            assert_eq!(refused(git), FilePathError::GitDirectory, "{git}");
        }
        assert_eq!(refused("a\\b"), FilePathError::Character);
        assert_eq!(refused("a\0b"), FilePathError::Character);
        assert_eq!(refused("a\nb"), FilePathError::Character);
        assert_eq!(refused(&"x".repeat(MAX_COMPONENT_BYTES + 1)), FilePathError::ComponentTooLong);
        assert!(FilePath::parse(&"x".repeat(MAX_COMPONENT_BYTES)).is_ok());
        let long = vec!["abcdefgh"; 120].join("/");
        assert_eq!(refused(&long), FilePathError::TooLong);
        assert_eq!(FilePath::join("a", "../b").unwrap_err(), FilePathError::Relative);
    }

    #[test]
    fn uploaded_names_keep_the_last_component() {
        assert_eq!(uploaded_file_name("C:\\Users\\ada\\notes.txt"), "notes.txt");
        assert_eq!(uploaded_file_name("dir/photo.png"), "photo.png");
        assert_eq!(uploaded_file_name(" plain.md "), "plain.md");
        assert_eq!(uploaded_file_name("trailing/"), "");
    }

    #[test]
    fn line_endings_follow_the_file() {
        assert_eq!(LineEnding::detect(b""), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\nb\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"a\r\nb\r\n"), LineEnding::CrLf);
        assert_eq!(LineEnding::detect(b"a\r\nb\n"), LineEnding::CrLf, "a tie keeps CRLF");
        assert_eq!(LineEnding::detect(b"a\r\nb\nc\n"), LineEnding::Lf);
        assert_eq!(LineEnding::detect(b"\n"), LineEnding::Lf);

        // Browsers send CRLF; the file keeps its style and exactly the user's last line.
        assert_eq!(prepare_text("a\r\nb\r\n", LineEnding::Lf), b"a\nb\n");
        assert_eq!(prepare_text("a\r\nb", LineEnding::Lf), b"a\nb", "no newline is appended");
        assert_eq!(prepare_text("a\r\nb\r\n", LineEnding::CrLf), b"a\r\nb\r\n");
        assert_eq!(prepare_text("a\nb\rc", LineEnding::CrLf), b"a\r\nb\r\nc");
        assert_eq!(prepare_text("", LineEnding::CrLf), b"");
    }

    #[test]
    fn commit_messages_default_and_validate() {
        let message = commit_message("  ", "", EditAction::Update("README.md")).unwrap();
        assert_eq!(message, "Update README.md\n");
        let message = commit_message("Fix typo", "Fixes #1\r\n\r\nThanks", EditAction::Upload).unwrap();
        assert_eq!(message, "Fix typo\n\nFixes #1\n\nThanks\n");
        assert_eq!(EditAction::Rename { from: "a", to: "b" }.default_title(), "Rename a to b");
        assert_eq!(EditAction::Create("x").default_title(), "Create x");
        assert_eq!(EditAction::Delete("x").default_title(), "Delete x");
        assert!(commit_message("two\nlines", "", EditAction::Upload).is_err());
        assert!(commit_message(&"t".repeat(MAX_TITLE_CHARS + 1), "", EditAction::Upload).is_err());
    }

    #[test]
    fn patch_branches_take_the_first_free_number() {
        assert_eq!(patch_branch_name("ada", |_| false), "ada-patch-1");
        assert_eq!(patch_branch_name("ada", |name| ["ada-patch-1", "ada-patch-2"].contains(&name)), "ada-patch-3");
    }
}
