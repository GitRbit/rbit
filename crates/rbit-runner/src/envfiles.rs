//! The env files a step writes to: `GITHUB_OUTPUT`, `GITHUB_ENV`, `GITHUB_PATH` and
//! `GITHUB_STEP_SUMMARY`, also reachable as `RBIT_OUTPUT`, `RBIT_ENV`, `RBIT_PATH` and
//! `RBIT_STEP_SUMMARY` (§12.5.4 "Env files").
//!
//! Each step gets fresh, empty files in the task's `files` directory; after the step they
//! are read and removed. A container job can write that directory, so the files are
//! created and read without following a link (`untrusted`): one a step puts in their
//! place fails the step instead of reading or truncating a file on the host. A file is at most [`MAX_FILE_BYTES`]; a larger one, or one that
//! does not parse, fails the step. Entries are `name=value` or the heredoc form
//!
//! ```text
//! name<<DELIMITER
//! line one
//! line two
//! DELIMITER
//! ```
//!
//! `GITHUB_ENV` cannot set names the runner owns or that would change how later programs
//! load code: `GITHUB_*`, `RBIT_*`, `RUNNER_*`, `NODE_OPTIONS`, `LD_PRELOAD`,
//! `LD_LIBRARY_PATH`, `DYLD_*`, `BASH_ENV` and `ENV`. Such entries are refused with a log
//! line; the step goes on.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

/// Largest env file a step may write.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// One env file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Output,
    Env,
    Path,
    StepSummary,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Output, Self::Env, Self::Path, Self::StepSummary];

    /// The variable without its `GITHUB_` / `RBIT_` prefix.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Output => "OUTPUT",
            Self::Env => "ENV",
            Self::Path => "PATH",
            Self::StepSummary => "STEP_SUMMARY",
        }
    }

    const fn stem(self) -> &'static str {
        match self {
            Self::Output => "output",
            Self::Env => "env",
            Self::Path => "path",
            Self::StepSummary => "step_summary",
        }
    }
}

/// Whether `name` can name an environment variable: `[A-Za-z_][A-Za-z0-9_.-]*`. Anything
/// else (`=`, a space, a newline, a `#`) could change what a container engine's env file
/// says, so a workflow's `env` and `GITHUB_ENV` may use only these.
pub fn valid_variable_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

/// Whether `GITHUB_ENV` may set `name`.
pub fn settable(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    !(["GITHUB_", "RBIT_", "RUNNER_", "DYLD_"].iter().any(|prefix| upper.starts_with(prefix))
        || ["NODE_OPTIONS", "LD_PRELOAD", "LD_LIBRARY_PATH", "BASH_ENV", "ENV"].contains(&upper.as_str()))
}

/// The env files of one step.
#[derive(Debug)]
pub struct EnvFiles {
    dir: PathBuf,
    step: usize,
}

/// What a step wrote to its env files.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub outputs: BTreeMap<String, String>,
    /// `GITHUB_ENV` entries that may be set, in order.
    pub env: Vec<(String, String)>,
    /// `GITHUB_ENV` names that were refused.
    pub refused: Vec<String>,
    /// `GITHUB_PATH` entries, in order.
    pub path: Vec<String>,
    pub summary: String,
}

impl EnvFiles {
    /// Creates the empty files of step `step` in `dir` (mode `0600` on Unix).
    pub fn create(dir: &Path, step: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let files = Self { dir: dir.to_path_buf(), step };
        for kind in Kind::ALL {
            crate::untrusted::create(&files.path(kind), true)?;
        }
        Ok(files)
    }

    /// The file's name inside the `files` directory.
    pub fn file_name(&self, kind: Kind) -> String {
        format!("{}_{}", kind.stem(), self.step)
    }

    /// The file on the runner host.
    pub fn path(&self, kind: Kind) -> PathBuf {
        self.dir.join(self.file_name(kind))
    }

    /// The variables pointing at the files, `GITHUB_*` and `RBIT_*`, with each file's path
    /// as the step sees it (`files_dir` is the `files` directory in the step's view).
    pub fn variables(&self, files_dir: &str) -> Vec<(String, String)> {
        let separator = if files_dir.contains('\\') { '\\' } else { '/' };
        Kind::ALL
            .into_iter()
            .flat_map(|kind| {
                let path = format!("{}{separator}{}", files_dir.trim_end_matches(['/', '\\']), self.file_name(kind));
                [(format!("GITHUB_{}", kind.suffix()), path.clone()), (format!("RBIT_{}", kind.suffix()), path)]
            })
            .collect()
    }

    /// Reads and parses the files, then removes them.
    pub fn collect(self) -> Result<Written, String> {
        let result = self.read();
        for kind in Kind::ALL {
            let _ = fs::remove_file(self.path(kind));
        }
        result
    }

    fn read(&self) -> Result<Written, String> {
        let text = |kind: Kind| -> Result<String, String> {
            let name = format!("GITHUB_{}", kind.suffix());
            let mut bytes = Vec::new();
            match crate::untrusted::open(&self.path(kind)) {
                Ok(file) => {
                    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|error| format!("cannot read {name}: {error}"))?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
                Err(error) => return Err(format!("cannot read {name}: {error}")),
            }
            if bytes.len() as u64 > MAX_FILE_BYTES {
                return Err(format!("{name} is larger than {} MiB", MAX_FILE_BYTES / 1024 / 1024));
            }
            decode(bytes).ok_or_else(|| format!("{name} is neither UTF-8 nor UTF-16"))
        };
        let mut written = Written::default();
        for (name, value) in parse_entries(&text(Kind::Output)?).map_err(|error| format!("GITHUB_OUTPUT: {error}"))? {
            if !valid_output_name(&name) {
                return Err(format!("GITHUB_OUTPUT: `{name}` is not a valid output name"));
            }
            written.outputs.insert(name, value);
        }
        for (name, value) in parse_entries(&text(Kind::Env)?).map_err(|error| format!("GITHUB_ENV: {error}"))? {
            if !valid_variable_name(&name) {
                return Err(format!("GITHUB_ENV: `{}` is not a valid variable name", name.escape_debug()));
            }
            if settable(&name) { written.env.push((name, value)) } else { written.refused.push(name) }
        }
        written.path = text(Kind::Path)?.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect();
        written.summary = text(Kind::StepSummary)?;
        Ok(written)
    }
}

/// An env file's text. Windows PowerShell writes UTF-8 with a byte order mark
/// (`Out-File -Encoding utf8`) and UTF-16 with one (`>>`); both are read as written.
fn decode(mut bytes: Vec<u8>) -> Option<String> {
    let utf16 = |bytes: &[u8], little: bool| {
        let units: Vec<u16> =
            bytes.as_chunks::<2>().0.iter().map(|&pair| if little { u16::from_le_bytes(pair) } else { u16::from_be_bytes(pair) }).collect();
        String::from_utf16(&units).ok()
    };
    match bytes.as_slice() {
        [0xEF, 0xBB, 0xBF, ..] => {
            bytes.drain(..3);
            String::from_utf8(bytes).ok()
        }
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8(bytes).ok(),
    }
}

/// `[A-Za-z_][A-Za-z0-9_-]*`.
fn valid_output_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= rbit_workflow::protocol::MAX_OUTPUT_KEY_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The `name=value` and `name<<DELIMITER` entries of an env file, in order.
pub fn parse_entries(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut entries = Vec::new();
    let mut lines = text.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).enumerate();
    while let Some((number, line)) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let equals = line.find('=');
        let heredoc = line.find("<<");
        match (equals, heredoc) {
            (Some(at), heredoc) if heredoc.is_none_or(|heredoc| at < heredoc) => {
                let name = &line[..at];
                if name.is_empty() {
                    return Err(format!("line {}: an entry needs a name", number + 1));
                }
                entries.push((name.to_owned(), line[at + 1..].to_owned()));
            }
            (_, Some(at)) => {
                let (name, delimiter) = (&line[..at], &line[at + 2..]);
                if name.is_empty() || delimiter.is_empty() {
                    return Err(format!("line {}: `name<<DELIMITER` needs a name and a delimiter", number + 1));
                }
                let mut value: Vec<&str> = Vec::new();
                loop {
                    match lines.next() {
                        Some((_, line)) if line == delimiter => break,
                        Some((_, line)) => value.push(line),
                        None => return Err(format!("line {}: the delimiter `{delimiter}` never comes", number + 1)),
                    }
                }
                entries.push((name.to_owned(), value.join("\n")));
            }
            _ => return Err(format!("line {}: expected `name=value` or `name<<DELIMITER`", number + 1)),
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_forms_parse() {
        let text = "a=1\r\nb=x=y\n\njson<<EOF\n{\n  \"k\": 1\n}\nEOF\nempty<<END\nEND\nlast=\n";
        let entries = parse_entries(text).unwrap();
        assert_eq!(
            entries,
            [
                ("a".into(), "1".into()),
                ("b".into(), "x=y".into()),
                ("json".into(), "{\n  \"k\": 1\n}".into()),
                ("empty".into(), String::new()),
                ("last".into(), String::new())
            ]
        );
        assert!(parse_entries("x<<EOF\nnever closed\n").unwrap_err().contains("never comes"));
        assert!(parse_entries("no separator\n").unwrap_err().contains("line 1"));
        assert!(parse_entries("=value\n").is_err());
        // `=` before `<<` is a plain value that happens to contain `<<`.
        assert_eq!(parse_entries("cmd=a<<b\n").unwrap(), [("cmd".into(), "a<<b".into())]);
    }

    #[test]
    fn steps_write_outputs_env_and_path() {
        let dir = tempfile::tempdir().unwrap();
        let files = EnvFiles::create(dir.path(), 3).unwrap();
        let variables = files.variables("/__w/_files");
        assert!(variables.contains(&("GITHUB_OUTPUT".into(), "/__w/_files/output_3".into())));
        assert!(variables.contains(&("RBIT_ENV".into(), "/__w/_files/env_3".into())));
        assert_eq!(files.variables("C:\\w\\files")[0].1, "C:\\w\\files\\output_3");
        fs::write(files.path(Kind::Output), "version=1.2\nnotes<<X\na\nb\nX\n").unwrap();
        fs::write(files.path(Kind::Env), "FOO=bar\nLD_PRELOAD=/evil.so\ngithub_token=x\nPATHLIKE=ok\n").unwrap();
        fs::write(files.path(Kind::Path), "/opt/tool/bin\n\n  /usr/local/x  \n").unwrap();
        fs::write(files.path(Kind::StepSummary), "# Done\n").unwrap();
        let output = files.path(Kind::Output);
        let written = files.collect().unwrap();
        assert_eq!(written.outputs, BTreeMap::from([("version".into(), "1.2".into()), ("notes".into(), "a\nb".into())]));
        assert_eq!(written.env, [("FOO".into(), "bar".into()), ("PATHLIKE".into(), "ok".into())]);
        assert_eq!(written.refused, ["LD_PRELOAD", "github_token"]);
        assert_eq!(written.path, ["/opt/tool/bin", "/usr/local/x"]);
        assert_eq!(written.summary, "# Done\n");
        assert!(!output.exists(), "the files are removed after the step");
    }

    #[test]
    fn powershell_encodings_are_read() {
        let utf16: Vec<u8> = [0xFF, 0xFE].into_iter().chain("a=1\r\n".encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(decode(utf16).as_deref(), Some("a=1\r\n"));
        assert_eq!(decode(b"\xEF\xBB\xBFa=1\n".to_vec()).as_deref(), Some("a=1\n"));
        assert_eq!(decode(b"\xFF\xFE\x00\xD8".to_vec()), None, "a lone surrogate");
        assert_eq!(decode(vec![0xC3]), None);
    }

    #[test]
    fn oversized_or_malformed_files_fail_the_step() {
        let dir = tempfile::tempdir().unwrap();
        let files = EnvFiles::create(dir.path(), 0).unwrap();
        fs::write(files.path(Kind::Env), "x".repeat(usize::try_from(MAX_FILE_BYTES).unwrap() + 1)).unwrap();
        assert!(files.collect().unwrap_err().contains("larger than 1 MiB"));
        let files = EnvFiles::create(dir.path(), 1).unwrap();
        fs::write(files.path(Kind::Output), "1bad=x\n").unwrap();
        assert!(files.collect().unwrap_err().contains("not a valid output name"));
        // Names an env file could misread are refused, not passed on to a container engine.
        for (index, line) in ["has space=x\n", "#hidden=x\n", "é=x\n"].into_iter().enumerate() {
            let files = EnvFiles::create(dir.path(), 10 + index).unwrap();
            fs::write(files.path(Kind::Env), line).unwrap();
            assert!(files.collect().unwrap_err().contains("not a valid variable name"), "{line:?}");
        }
        assert!(valid_variable_name("_A1.b-c") && !valid_variable_name("1A") && !valid_variable_name("A=B"));
        for name in ["GITHUB_X", "rbit_x", "RUNNER_TEMP", "DYLD_INSERT_LIBRARIES", "BASH_ENV", "env", "NODE_OPTIONS"] {
            assert!(!settable(name), "{name}");
        }
        assert!(settable("CARGO_HOME") && settable("ENVIRONMENT"));
    }

    /// A container step can replace its env files, or the next step's, with links to the
    /// host; the runner neither reads nor truncates what they point to.
    #[cfg(unix)]
    #[test]
    fn links_in_place_of_env_files_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let credential = dir.path().join("credential");
        fs::write(&credential, "rbrt_secret").unwrap();
        let files_dir = dir.path().join("files");

        let files = EnvFiles::create(&files_dir, 0).unwrap();
        let summary = files.path(Kind::StepSummary);
        fs::remove_file(&summary).unwrap();
        std::os::unix::fs::symlink(&credential, &summary).unwrap();
        let error = files.collect().unwrap_err();
        assert!(error.contains("GITHUB_STEP_SUMMARY") && error.contains("not a regular file"), "{error}");
        assert!(!error.contains("rbrt_secret"));
        assert!(!summary.exists() && credential.exists(), "the link is removed, its target kept");

        std::os::unix::fs::symlink(&credential, files_dir.join("env_1")).unwrap();
        let files = EnvFiles::create(&files_dir, 1).unwrap();
        assert_eq!(fs::read_to_string(&credential).unwrap(), "rbrt_secret", "creating the files truncates nothing on the host");
        assert!(!fs::symlink_metadata(files.path(Kind::Env)).unwrap().file_type().is_symlink());
        assert_eq!(files.collect().unwrap(), Written::default());
    }
}
