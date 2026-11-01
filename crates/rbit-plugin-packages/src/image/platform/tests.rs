use super::*;
#[path = "../../../tests/support/image_fixture.rs"]
mod fixture;
use fixture::*;
use std::{
    io::Cursor,
    time::{Duration, Instant},
};
fn control() -> StageControl {
    StageControl::new(Instant::now() + Duration::from_mins(1), MAX_ARCHIVE_WIRE_BYTES).unwrap()
}
fn directory(root: &Path) -> tempfile::TempDir {
    tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir_in(root).unwrap()
}
#[derive(Clone, Copy)]
enum Action {
    None,
    Cancel,
    OutputConflict,
    ReadError,
}
struct FaultRead {
    source: Cursor<Vec<u8>>,
    root: PathBuf,
    control: StageControl,
    action: Action,
    threshold: u64,
    fired: bool,
    interrupt: bool,
}
impl Read for FaultRead {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.interrupt {
            self.interrupt = false;
            return Err(io::ErrorKind::Interrupted.into());
        }
        self.interrupt = true;
        if !self.fired && self.source.position() >= self.threshold {
            // Actual first payload was tentatively written, still0600/nonexec.
            assert_eq!(fs::read(self.root.join("bin/probe")).unwrap(), PROBE);
            assert_eq!(fs::metadata(self.root.join("bin/probe")).unwrap().permissions().mode() & 0o7777, 0o600);
            self.fired = true;
            match self.action {
                Action::None => {}
                Action::Cancel => self.control.cancel(),
                Action::ReadError => return Err(io::Error::other("injected read error with private details")),
                Action::OutputConflict => {
                    DirBuilder::new().mode(0o700).create(self.root.join("cfg"))?;
                    OpenOptions::new().write(true).create_new(true).mode(0o600).open(self.root.join("cfg/settings"))?;
                }
            }
        }
        let count = output.len().min(7);
        self.source.read(&mut output[..count])
    }
}
fn reader(bytes: Vec<u8>, path: &Path, control: StageControl, action: Action) -> FaultRead {
    let size = std::str::from_utf8(&bytes[124..136]).unwrap().trim_matches(['\0', ' ']);
    let threshold = 512 + usize::from_str_radix(size, 8).unwrap().next_multiple_of(512) + 512 + PROBE.len().next_multiple_of(512);
    FaultRead {
        source: Cursor::new(bytes),
        root: path.to_path_buf(),
        control,
        action,
        threshold: threshold as u64,
        fired: false,
        interrupt: true,
    }
}
#[test]
fn interrupted_small_chunks_still_hash_and_materialize_the_same_bytes() {
    let root = private_root();
    let directory = directory(root.path());
    let bytes = archive(&native());
    let control = control();
    let input = reader(bytes.clone(), directory.path(), control.clone(), Action::None);
    let image = finish_prepare(directory, input, &expected(&bytes), bytes.len() as u64, control).unwrap();
    assert_eq!(fs::read(image.entrypoint()).unwrap(), PROBE);
    assert_eq!(fs::read(image.root().join("cfg/settings")).unwrap(), CONFIG);
    image.discard().unwrap();
    assert_eq!(children(root.path()), Vec::<PathBuf>::new());
}
#[test]
fn cancellation_read_and_real_create_new_failures_keep_specific_faults_and_cleanup() {
    let root = private_root();
    let bytes = archive(&native());
    for (action, error) in
        [(Action::Cancel, ImageError::Cancelled), (Action::ReadError, ImageError::Read), (Action::OutputConflict, ImageError::Write)]
    {
        let directory = directory(root.path());
        let control = control();
        let path = directory.path().to_path_buf();
        let input = reader(bytes.clone(), directory.path(), control.clone(), action);
        let failure = finish_prepare(directory, input, &expected(&bytes), bytes.len() as u64, control).unwrap_err();
        assert_eq!(failure.error(), error);
        assert_eq!(failure.cleanup(), CleanupStatus::Complete);
        assert!(!path.exists());
        assert_eq!(children(root.path()), Vec::<PathBuf>::new());
        assert!(!failure.to_string().contains("private details"));
    }
}

#[test]
fn already_opened_object_stays_bound_when_its_original_path_is_replaced() {
    let root = private_root();
    let output = private_root();
    let bytes = archive(&native());
    let path = root.path().join("object");
    fs::write(&path, &bytes).unwrap();
    let source = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path).unwrap();
    fs::rename(&path, root.path().join("previous-inode")).unwrap();
    fs::write(&path, b"replacement path must not be read").unwrap();
    // This exercises the common post-open filesystem core, not an authorized
    // installed package or an adversarial same-UID path-race guarantee.
    let image = finish_prepare(directory(output.path()), source, &expected(&bytes), bytes.len() as u64, control()).unwrap();
    assert_eq!(fs::read(image.entrypoint()).unwrap(), PROBE);
    image.discard().unwrap();
    assert_eq!(fs::read(path).unwrap(), b"replacement path must not be read");
    assert_eq!(children(output.path()), Vec::<PathBuf>::new());
}

struct ShortWriter {
    file: File,
    remaining: Option<usize>,
    written: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    cancel_after_write: Option<StageControl>,
}
impl Write for ShortWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == Some(0) {
            return Err(io::Error::other("injected mid-write private detail"));
        }
        let limit = bytes.len().min(3).min(self.remaining.unwrap_or(usize::MAX));
        let count = self.file.write(&bytes[..limit])?;
        self.written.fetch_add(count, std::sync::atomic::Ordering::Relaxed);
        if let Some(remaining) = &mut self.remaining {
            *remaining -= count;
        }
        if let Some(control) = self.cancel_after_write.take() {
            control.cancel();
        }
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
#[test]
fn real_partial_file_writes_settle_success_injected_failure_and_cancellation() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let root = private_root();
    let bytes = archive(&native());
    for (remaining, cancel, error) in
        [(None, false, None), (Some(7), false, Some(ImageError::Write)), (None, true, Some(ImageError::Cancelled))]
    {
        let control = control();
        let cancellation = cancel.then(|| control.clone());
        let written = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&written);
        // The decorator performs actual prefix writes to the real0600 File, then
        // injects an error/cancel. This is not a real disk-full syscall proof.
        let result = finish_prepare_with_writer(
            directory(root.path()),
            Cursor::new(bytes.clone()),
            &expected(&bytes),
            bytes.len() as u64,
            control,
            move |file| ShortWriter { file, remaining, written: Arc::clone(&count), cancel_after_write: cancellation.clone() },
        );
        if let Some(error) = error {
            let failure = result.unwrap_err();
            assert_eq!(failure.error(), error);
            assert_eq!(failure.cleanup(), CleanupStatus::Complete);
            assert_eq!(written.load(Ordering::Relaxed), if cancel { 3 } else { 7 });
            assert!(!failure.to_string().contains("private detail"));
        } else {
            let image = result.unwrap();
            assert_eq!(written.load(Ordering::Relaxed), PROBE.len() + CONFIG.len());
            assert_eq!(fs::read(image.entrypoint()).unwrap(), PROBE);
            assert_eq!(fs::read(image.root().join("cfg/settings")).unwrap(), CONFIG);
            image.discard().unwrap();
        }
        assert_eq!(children(root.path()), Vec::<PathBuf>::new());
    }
}
