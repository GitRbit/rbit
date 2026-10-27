//! Files the runner reads and writes on the host in directories a container job can write
//! too: the task's workspace, `temp` and `files` (§12.5.2).
//!
//! A job can leave a symbolic link, a named pipe or a directory where the runner expects a
//! file of its own, and its link would resolve on the host. So the runner never follows a
//! link there: [`create`] replaces whatever is at the path with a new file (`O_EXCL`, so a
//! link planted in between makes it fail rather than be followed), and [`open`] opens only
//! a regular file (`O_NOFOLLOW`, and `O_NONBLOCK` so a pipe cannot hold the runner up).
//!
//! The directories themselves are bind-mount points, which a container cannot replace.
//! Paths deeper inside the workspace are checked with [`crosses_link`]; the built-in steps
//! that work there run while the job container is paused, so nothing changes under them.

use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

/// Creates a new, empty file at `path`, removing a file or link already there; mode `0600`
/// on Unix when `private`.
pub fn create(path: &Path, private: bool) -> io::Result<File> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("{} is a directory", path.display())));
        }
        Ok(_) => fs::remove_file(path)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        if private {
            options.mode(0o600);
        }
    }
    #[cfg(not(unix))]
    let _ = private;
    options.open(path)
}

/// Opens the regular file at `path` for reading, refusing a link, a pipe or a device.
/// Anything else is turned away before it is opened, so opening a device has no effect;
/// the type is checked again on the open file, in case it changed in between.
pub fn open(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(not_regular(path));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a link itself, not what it points to.
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(|error| match error.raw_os_error() {
        #[cfg(unix)]
        Some(libc::ELOOP) => not_regular(path),
        _ => error,
    })?;
    let metadata = file.metadata()?;
    if metadata.is_file() && !metadata.file_type().is_symlink() { Ok(file) } else { Err(not_regular(path)) }
}

fn not_regular(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("{} is not a regular file", path.display()))
}

/// Whether `relative`, or any directory on the way to it under `root`, is a symbolic link.
pub fn crosses_link(root: &Path, relative: &Path) -> bool {
    let mut path = root.to_path_buf();
    relative.components().any(|component| {
        path.push(component);
        fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        os::unix::fs::PermissionsExt as _,
    };

    use super::*;

    #[test]
    fn create_replaces_a_link_instead_of_writing_through_it() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("credential");
        fs::write(&outside, "secret").unwrap();
        let path = dir.path().join("summary");
        std::os::unix::fs::symlink(&outside, &path).unwrap();

        create(&path, true).unwrap().write_all(b"mine").unwrap();
        assert_eq!(fs::read_to_string(&outside).unwrap(), "secret", "the link's target is untouched");
        assert!(!fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&path).unwrap(), "mine");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

        fs::create_dir(dir.path().join("dir")).unwrap();
        assert!(create(&dir.path().join("dir"), false).is_err());
    }

    #[test]
    fn open_reads_only_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("credential");
        fs::write(&outside, "secret").unwrap();
        let mut text = String::new();
        open(&outside).unwrap().read_to_string(&mut text).unwrap();
        assert_eq!(text, "secret");

        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert_eq!(open(&link).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(open(dir.path()).unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert_eq!(open(&dir.path().join("missing")).unwrap_err().kind(), io::ErrorKind::NotFound);

        if let Some(mkfifo) = crate::process::find_program("mkfifo") {
            let fifo = dir.path().join("fifo");
            assert!(std::process::Command::new(mkfifo).arg(&fifo).status().unwrap().success());
            assert_eq!(open(&fifo).unwrap_err().kind(), io::ErrorKind::InvalidInput, "a pipe neither blocks nor is read");
        }
    }

    #[test]
    fn links_on_the_way_are_found() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("real/sub")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("link")).unwrap();
        assert!(!crosses_link(dir.path(), Path::new("real/sub")));
        assert!(crosses_link(dir.path(), Path::new("link/sub")));
        assert!(crosses_link(dir.path(), Path::new("link")));
        assert!(!crosses_link(dir.path(), Path::new("missing/x")));
    }
}
