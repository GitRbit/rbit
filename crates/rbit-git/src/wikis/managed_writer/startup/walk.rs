//! Retain the actual directory iterators; never collect/sort a directory's names.
use super::WikiObjectError;
use crate::wikis::layout;
use std::{
    fs::{self, ReadDir},
    path::{Path, PathBuf},
};

pub(super) enum Step {
    Progress,
    Object(PathBuf, gix::ObjectId),
    End,
}
pub(super) struct Walk {
    root: ReadDir,
    prefix: Option<(String, ReadDir)>,
}
fn io(error: std::io::Error) -> WikiObjectError {
    crate::GitError::Io(error).into()
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn directory(path: &Path) -> Result<ReadDir, WikiObjectError> {
    layout::no_links(path)?;
    if !fs::symlink_metadata(path).map_err(io)?.is_dir() {
        return Err(WikiObjectError::Binding);
    }
    fs::read_dir(path).map_err(io)
}
impl Walk {
    pub(super) fn open(path: &Path) -> Result<Self, WikiObjectError> {
        Ok(Self { root: directory(path)?, prefix: None })
    }
    pub(super) fn next(&mut self) -> Result<Step, WikiObjectError> {
        if let Some((prefix, entries)) = &mut self.prefix {
            let Some(entry) = entries.next() else {
                self.prefix = None;
                return Ok(Step::Progress);
            };
            let entry = entry.map_err(io)?;
            let name = entry.file_name().into_string().map_err(|_| WikiObjectError::Format)?;
            layout::no_links(&entry.path())?;
            if !hex(&name, 38) || !entry.file_type().map_err(io)?.is_file() {
                return Err(WikiObjectError::Format);
            }
            let id = gix::ObjectId::from_hex(format!("{prefix}{name}").as_bytes()).map_err(|_| WikiObjectError::Format)?;
            return Ok(Step::Object(entry.path(), id));
        }
        let Some(entry) = self.root.next() else { return Ok(Step::End) };
        let entry = entry.map_err(io)?;
        let name = entry.file_name().into_string().map_err(|_| WikiObjectError::Format)?;
        if matches!(name.as_str(), "info" | "pack") {
            // Foreign info, alternates, packs and partial pack artifacts quarantine
            // this implementation; no pack index is implicitly trusted or loaded.
            if directory(&entry.path())?.next().transpose().map_err(io)?.is_some() {
                return Err(WikiObjectError::Format);
            }
        } else if hex(&name, 2) {
            self.prefix = Some((name, directory(&entry.path())?));
        } else {
            return Err(WikiObjectError::Format);
        }
        Ok(Step::Progress)
    }
}
