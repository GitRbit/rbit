//! The disk adapter behind `rbit_actions::logs::TaskLogFiles`: one task's log files under
//! `<data_dir>/actions/logs/<repo id>/`. Write ordering, syncs, recovery, gzip and reads
//! are decided in `rbit-actions`.
use super::types::LogFiles;
use rbit_actions::logs::{LogPart, TaskLogFiles};
use std::{
    fs::{File, OpenOptions},
    io,
    path::PathBuf,
};

impl LogFiles {
    fn path(&self, part: LogPart) -> PathBuf {
        match part {
            LogPart::Log => self.log.clone(),
            LogPart::Index => self.index.clone(),
            LogPart::Gzip => self.gzip.clone(),
            LogPart::GzipPartial => self.gzip.with_extension("gz.partial"),
        }
    }
}

impl TaskLogFiles for LogFiles {
    type File = File;

    fn prepare_directory(&self) -> io::Result<()> {
        match self.log.parent() {
            Some(dir) => std::fs::create_dir_all(dir),
            None => Ok(()),
        }
    }

    fn open(&self, part: LogPart) -> io::Result<File> {
        File::open(self.path(part))
    }

    fn open_writable(&self, part: LogPart) -> io::Result<File> {
        OpenOptions::new().create(true).truncate(false).read(true).write(true).open(self.path(part))
    }

    fn open_append(&self, part: LogPart) -> io::Result<File> {
        OpenOptions::new().create(true).append(true).open(self.path(part))
    }

    fn create(&self, part: LogPart) -> io::Result<File> {
        File::create(self.path(part))
    }

    fn rename(&self, from: LogPart, to: LogPart) -> io::Result<()> {
        std::fs::rename(self.path(from), self.path(to))
    }

    fn remove(&self, part: LogPart) -> io::Result<()> {
        std::fs::remove_file(self.path(part))
    }
}
