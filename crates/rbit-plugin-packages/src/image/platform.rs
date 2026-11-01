use super::{ImageError, ImageFailure, NativeImageStore, PreparedNativeImage};
use crate::{
    ArchiveError, ArchiveSink, CleanupStatus, MAX_ARCHIVE_WIRE_BYTES, PackageStore, StageControl, StoreError, validate_archive_with_sink,
};
use rbit_plugin_api::{InstalledPackageRef, PackageKind, PackageManifest, PackagePath};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

pub(super) fn open(root: &Path) -> Result<PathBuf, ImageError> {
    private_directory(root)?;
    fs::canonicalize(root).map_err(|_| ImageError::Root)
}
fn private_directory(path: &Path) -> Result<(), ImageError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ImageError::Root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.permissions().mode() & 0o7777 != 0o700 {
        return Err(ImageError::Root);
    }
    Ok(())
}
fn check(control: &StageControl) -> Result<(), ImageError> {
    control.check().map_err(|error| match error {
        StoreError::Cancelled => ImageError::Cancelled,
        StoreError::Deadline => ImageError::Deadline,
        _ => ImageError::Control,
    })
}

pub(super) fn prepare(
    store: &NativeImageStore,
    cas: &PackageStore,
    expected: &InstalledPackageRef,
    wire_bytes: u64,
    control: StageControl,
) -> Result<PreparedNativeImage, ImageFailure> {
    check(&control).map_err(ImageFailure::clean)?;
    if wire_bytes == 0 || wire_bytes >= MAX_ARCHIVE_WIRE_BYTES || wire_bytes + 1 > control.wire_quota() {
        return Err(ImageFailure::clean(ImageError::Control));
    }
    let source = cas.object_path(expected.archive_digest());
    let cas_root = source.parent().ok_or_else(|| ImageFailure::clean(ImageError::Root))?;
    if store.root.starts_with(cas_root) || cas_root.starts_with(store.root.as_ref()) {
        return Err(ImageFailure::clean(ImageError::Root));
    }
    private_directory(&store.root).map_err(ImageFailure::clean)?;
    check(&control).map_err(ImageFailure::clean)?;
    if libc::O_NOFOLLOW == 0 || libc::O_NONBLOCK == 0 {
        return Err(ImageFailure::clean(ImageError::Unsupported));
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(source)
        .map_err(|_| ImageFailure::clean(ImageError::Object))?;
    check(&control).map_err(ImageFailure::clean)?;
    let metadata = file.metadata().map_err(|_| ImageFailure::clean(ImageError::Object))?;
    if !metadata.is_file() || metadata.len() != wire_bytes {
        return Err(ImageFailure::clean(ImageError::Object));
    }
    check(&control).map_err(ImageFailure::clean)?;
    let directory = tempfile::Builder::new()
        .prefix(".rbit-image-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(store.root.as_ref())
        .map_err(|_| ImageFailure::clean(ImageError::Write))?;
    finish_prepare(directory, file, expected, wire_bytes, control)
}

fn finish_prepare(
    directory: tempfile::TempDir,
    source: impl Read,
    expected: &InstalledPackageRef,
    wire_bytes: u64,
    control: StageControl,
) -> Result<PreparedNativeImage, ImageFailure> {
    finish_prepare_with_writer(directory, source, expected, wire_bytes, control, std::convert::identity::<File>)
}
// Private static seam for injected partial-write/failure tests, never a public
// payload callback or a boxed production writer. Production W is exactly File.
fn finish_prepare_with_writer<W: Write>(
    directory: tempfile::TempDir,
    source: impl Read,
    expected: &InstalledPackageRef,
    wire_bytes: u64,
    control: StageControl,
    wrap: impl FnMut(File) -> W,
) -> Result<PreparedNativeImage, ImageFailure> {
    let result = (|| {
        check(&control)?;
        private_directory(directory.path())?;
        let sink_control = control.clone();
        let mut reader = CheckedReader { source, control, bytes: 0, digest: Sha256::new(), fault: None };
        let mut sink = ImageSink { root: directory.path(), expected, control: &sink_control, current: None, fault: None, wrap };
        let archive = validate_archive_with_sink(&mut reader, &mut sink)
            .map_err(|error| reader.fault.or(sink.fault).unwrap_or(ImageError::Archive(error)))?;
        check(&sink_control)?;
        if archive.wire_bytes() != wire_bytes || !crate::digest_matches(&reader.digest.finalize(), expected.archive_digest()) {
            return Err(ImageError::Identity);
        }
        let PackageKind::Native { entrypoint, .. } = &archive.manifest().kind else {
            return Err(ImageError::Kind);
        };
        let entrypoint = directory.path().join(entrypoint.as_str());
        Ok((archive.manifest().clone(), entrypoint))
    })();
    match result {
        Ok((manifest, entrypoint)) => Ok(PreparedNativeImage { directory, expected: expected.clone(), manifest, entrypoint, wire_bytes }),
        Err(error) => Err(cleanup(directory, error)),
    }
}
fn cleanup(directory: tempfile::TempDir, error: ImageError) -> ImageFailure {
    let path = directory.path().to_path_buf();
    // A restrictive umask may leave the newly created, still-empty directory
    // unsearchable. Remove that empty owned name via its private parent without
    // chmod repair; recursive cleanup would need to open/enumerate it first.
    if fs::remove_dir(&path).is_ok() {
        drop(directory);
        return ImageFailure::clean(error);
    }
    match directory.close() {
        Ok(()) => ImageFailure::clean(error),
        Err(_) => ImageFailure { error, cleanup: CleanupStatus::Pending, residual: Some(path) },
    }
}

struct CheckedReader<R> {
    source: R,
    control: StageControl,
    bytes: u64,
    digest: Sha256,
    fault: Option<ImageError>,
}
impl<R: Read> Read for CheckedReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let result = (|| {
            check(&self.control)?;
            let available = self.control.wire_quota().checked_sub(self.bytes).ok_or(ImageError::Quota)?;
            let count = usize::try_from(available.min(output.len() as u64)).map_err(|_| ImageError::Quota)?;
            if count == 0 {
                return Err(ImageError::Quota);
            }
            let read = match self.source.read(&mut output[..count]) {
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(None),
                Err(_) => return Err(ImageError::Read),
            };
            check(&self.control)?;
            self.digest.update(&output[..read]);
            self.bytes += read as u64;
            Ok(Some(read))
        })();
        match result {
            Ok(Some(read)) => Ok(read),
            Ok(None) => Err(io::ErrorKind::Interrupted.into()),
            Err(error) => {
                self.fault.get_or_insert(error);
                Err(io::Error::other("native image source failure"))
            }
        }
    }
}

struct ImageSink<'a, W, F> {
    root: &'a Path,
    expected: &'a InstalledPackageRef,
    control: &'a StageControl,
    current: Option<W>,
    fault: Option<ImageError>,
    wrap: F,
}
impl<W: Write, F: FnMut(File) -> W> ImageSink<'_, W, F> {
    fn record(&mut self, result: Result<(), ImageError>) -> Result<(), ArchiveError> {
        result.map_err(|error| {
            self.fault.get_or_insert(error);
            ArchiveError::Read
        })
    }
    fn begin_file(&mut self, path: &PackagePath) -> Result<(), ImageError> {
        check(self.control)?;
        let mut directory = self.root.to_path_buf();
        let mut parts = path.as_str().split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                check(self.control)?;
                let file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(directory.join(part))
                    .map_err(|_| ImageError::Write)?;
                check(self.control)?;
                if file.metadata().map_err(|_| ImageError::Write)?.permissions().mode() & 0o7777 != 0o600 {
                    return Err(ImageError::Write);
                }
                self.current = Some((self.wrap)(file));
            } else {
                directory.push(part);
                check(self.control)?;
                match DirBuilder::new().mode(0o700).create(&directory) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(_) => return Err(ImageError::Write),
                }
                check(self.control)?;
                private_directory(&directory)?;
            }
        }
        Ok(())
    }
    fn write_chunk(&mut self, mut bytes: &[u8]) -> Result<(), ImageError> {
        while !bytes.is_empty() {
            check(self.control)?;
            let file = self.current.as_mut().ok_or(ImageError::Write)?;
            match file.write(bytes) {
                Ok(0) => return Err(ImageError::Write),
                Ok(count) => bytes = &bytes[count..],
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(ImageError::Write),
            }
            check(self.control)?;
        }
        Ok(())
    }
}
impl<W: Write, F: FnMut(File) -> W> ArchiveSink for ImageSink<'_, W, F> {
    fn manifest(&mut self, manifest: &PackageManifest) -> Result<(), ArchiveError> {
        let result = (|| {
            check(self.control)?;
            if !matches!(manifest.kind, PackageKind::Native { .. }) {
                return Err(ImageError::Kind);
            }
            if &manifest.plugin.id != self.expected.plugin_id() || &manifest.plugin.version != self.expected.version() {
                return Err(ImageError::Identity);
            }
            Ok(())
        })();
        self.record(result)
    }
    fn begin(&mut self, path: &PackagePath) -> Result<(), ArchiveError> {
        let result = self.begin_file(path);
        self.record(result)
    }
    fn chunk(&mut self, bytes: &[u8]) -> Result<(), ArchiveError> {
        let result = self.write_chunk(bytes);
        self.record(result)
    }
    fn end(&mut self) -> Result<(), ArchiveError> {
        let result = check(self.control);
        self.current.take();
        self.record(result)
    }
}

#[cfg(test)]
mod tests;
