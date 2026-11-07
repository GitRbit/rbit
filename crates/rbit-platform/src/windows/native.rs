//! The ONLY authored FFI exception. No raw handle/pointer/flags leave this module.
#![allow(unsafe_code)]

use crate::PlatformError;
use std::{
    fs::File,
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::{
    Wdk::Storage::FileSystem::{FileIsRemoteDeviceInformation, NtQueryInformationFile},
    Wdk::System::SystemServices::FILE_IS_REMOTE_DEVICE_INFORMATION,
    Win32::{
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle, GetVolumeInformationByHandleW, MOVEFILE_REPLACE_EXISTING,
            MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        System::{IO::IO_STATUS_BLOCK, SystemServices::FILE_READ_ONLY_VOLUME},
    },
};

pub(super) struct Subject {
    pub(super) directory: File,
    // Last: real exclusive lease survives subject-handle destruction.
    pub(super) _lease: File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub(super) volume: u32,
    pub(super) index: u64,
    pub(super) attributes: u32,
    pub(super) links: u32,
}

// Integer backing never constructs/references a potentially invalid native bool
// or active union member. These assert the exact published binding's ABI storage.
const _: () = assert!(size_of::<[usize; 2]>() == size_of::<IO_STATUS_BLOCK>());
const _: () = assert!(align_of::<[usize; 2]>() >= align_of::<IO_STATUS_BLOCK>());
const _: () = assert!(std::mem::offset_of!(IO_STATUS_BLOCK, Anonymous) == 0);
const _: () = assert!(std::mem::offset_of!(IO_STATUS_BLOCK, Information) == size_of::<usize>());
const _: () = assert!(size_of::<[u8; 1]>() == size_of::<FILE_IS_REMOTE_DEVICE_INFORMATION>());
const _: () = assert!(align_of::<[u8; 1]>() >= align_of::<FILE_IS_REMOTE_DEVICE_INFORMATION>());
const _: () = assert!(std::mem::offset_of!(FILE_IS_REMOTE_DEVICE_INFORMATION, IsRemote) == 0);

struct RemoteRequest {
    io: *mut [usize; 2],
    remote: *mut [u8; 1],
    subject: Option<Subject>,
    completed: bool,
}
impl RemoteRequest {
    fn new(subject: Subject, io: [usize; 2], remote: [u8; 1]) -> Self {
        // Transfer ownership BEFORE submitting native I/O. Moving the request no
        // longer moves/retags a Box that native code may still be accessing.
        Self { io: Box::into_raw(Box::new(io)), remote: Box::into_raw(Box::new(remote)), subject: Some(subject), completed: false }
    }
    fn finish(mut self) -> Subject {
        assert!(self.completed, "native completion must be verified before reclaim");
        // SAFETY: completed is private and set only after actual SUCCESS, final
        // status0 and exact Information length. Both exact allocations originated
        // in Box::into_raw, have no Rust/native live aliases, and are reclaimed once.
        unsafe {
            drop(Box::from_raw(self.io));
            drop(Box::from_raw(self.remote));
        }
        self.subject.take().expect("subject consumed exactly once")
    }
}
impl Drop for RemoteRequest {
    fn drop(&mut self) {
        if let Some(subject) = self.subject.take() {
            // Unproved Drop retains buffers AND the exclusive subject. No Box is
            // reconstructed, no output is inspected, no outstanding handle closes.
            std::mem::forget(subject);
        }
    }
}

pub(super) fn information(file: &File) -> Result<Identity, PlatformError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: File owns a live borrowed handle; binding-defined integer-only
    // output is initialized, correctly aligned and lives through this sync call.
    let success = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(Identity {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        attributes: info.dwFileAttributes,
        links: info.nNumberOfLinks,
    })
}

fn ntfs(file: &File) -> Result<(), PlatformError> {
    let mut filesystem = [0_u16; 16];
    let mut flags = 0_u32;
    // SAFETY: live borrowed handle; initialized WCHAR/integer outputs have exact
    // capacities. Optional unused outputs are null with length0. Win32 BOOL call
    // completes synchronously and retains no output pointer after return.
    let success = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut flags,
            filesystem.as_mut_ptr(),
            16,
        )
    };
    if success == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if filesystem[..5] != [78, 84, 70, 83, 0] || flags & FILE_READ_ONLY_VOLUME != 0 {
        return Err(PlatformError::Unsupported);
    }
    Ok(())
}

/// Consumes the actual exclusively leased subject. Only one query can access its
/// handle/buffers; no clones/public arbitrary handle entry point exists.
pub(super) fn classify(subject: Subject) -> Result<Subject, PlatformError> {
    ntfs(&subject.directory)?;
    let request = RemoteRequest::new(subject, [0; 2], [0]);
    // SAFETY: exact binding size/alignment asserted above. Both initialized heap
    // buffers and the REAL subject handle remain owned by request. No Rust bool/
    // union reference is formed. On ANY non-success status, including PENDING, we
    // do not inspect native-writable outputs and deliberately retain ALL resources.
    let status = unsafe {
        NtQueryInformationFile(
            request.subject.as_ref().expect("unconsumed subject").directory.as_raw_handle(),
            request.io.cast::<IO_STATUS_BLOCK>(),
            request.remote.cast(),
            1,
            FileIsRemoteDeviceInformation,
        )
    };
    complete(request, status)
}

fn complete(mut request: RemoteRequest, status: i32) -> Result<Subject, PlatformError> {
    if status != 0 {
        return Err(PlatformError::Quarantined);
    }
    // STATUS_SUCCESS is the actual completion barrier. Only now inspect outputs.
    // IO_STATUS_BLOCK offset0 status is zero on success; offset sizeof(usize) is
    // Information. No inactive union-member load or native bool is ever performed.
    // SAFETY: the returned native SUCCESS is the completion barrier; these are
    // initialized integer bytes, not a Rust bool/union value. Pending/unknown NEVER
    // reaches this read. No Box/reference reborrow invalidates escaped provenance.
    let io = unsafe { request.io.read() };
    if io[0] != 0 || io[1] != 1 {
        return Err(PlatformError::Quarantined);
    }
    // SAFETY: confirmed completed query with exact1-byte output. Every u8 pattern
    // is valid; never load the binding's bool field even for malformed native data.
    let remote = unsafe { request.remote.read() };
    request.completed = true;
    let subject = request.finish();
    if remote[0] == 0 { Ok(subject) } else { Err(PlatformError::Unsupported) }
}

#[cfg(test)]
pub(super) fn simulated_status(subject: Subject, status: i32) -> Result<Subject, PlatformError> {
    // Fault-model only: no kernel operation is issued. Poisoned bytes must never
    // be examined for pending/unknown status. Retention is still the REAL lease.
    complete(RemoteRequest::new(subject, [usize::MAX; 2], [255]), status)
}

fn wide(path: &Path) -> Result<Vec<u16>, PlatformError> {
    let mut value: Vec<_> = path.as_os_str().encode_wide().take(32_768).collect();
    if !path.is_absolute() || value.len() >= 32_767 || value.contains(&0) {
        return Err(PlatformError::InvalidName);
    }
    value.push(0);
    Ok(value)
}

fn move_entry(source: &Path, target: &Path, flags: u32) -> Result<(), PlatformError> {
    let source = wide(source)?;
    let target = wide(target)?;
    // SAFETY: NUL-terminated, bounded UTF16 arrays remain alive throughout the
    // synchronous call. Flags are PRIVATE constants: no copy, delayed reboot,
    // callback or borrowed pointer survives return. No handle ownership transfer.
    let success = unsafe { MoveFileExW(source.as_ptr(), target.as_ptr(), flags) };
    if success == 0 {
        let error = std::io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(80 | 183) => PlatformError::Conflict,
            Some(17) => PlatformError::Unsupported,
            _ => error.into(),
        });
    }
    Ok(())
}

pub(super) fn publish_new(source: &Path, target: &Path) -> Result<(), PlatformError> {
    move_entry(source, target, MOVEFILE_WRITE_THROUGH)
}
pub(super) fn replace_mutable(source: &Path, target: &Path) -> Result<(), PlatformError> {
    move_entry(source, target, MOVEFILE_WRITE_THROUGH | MOVEFILE_REPLACE_EXISTING)
}
