use std::{
    cell::Cell,
    fmt::Write as _,
    io::{self, Cursor, Read},
    rc::Rc,
};

use rbit_plugin_api::{MAX_PACKAGE_MANIFEST_BYTES, PackageKind, PackageManifest};
use rbit_plugin_packages::{ArchiveError, validate_archive};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha256(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(encoded, "{byte:02x}").unwrap();
    }
    encoded
}

fn descriptor(files: &[(&str, &[u8])]) -> Value {
    json!({
        "schema_version":1,
        "plugin":{"id":"example.archive", "version":"1.0.0", "host_api":"^1"},
        "kind":{"type":"data"},
        "files":files.iter().map(|(path,body)| json!({"path":path,"size":body.len(),"sha256":sha256(body)})).collect::<Vec<_>>(),
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    })
}
fn header(path: &str, size: u64) -> [u8; 512] {
    let mut header = tar::Header::new_ustar();
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_size(size);
    header.set_entry_type(tar::EntryType::Regular);
    header.set_path(path).unwrap();
    header.set_cksum();
    *header.as_bytes()
}
fn checksum(bytes: &mut [u8]) {
    let header = tar::Header::from_byte_slice(bytes);
    let mut header = header.clone();
    header.set_cksum();
    bytes.copy_from_slice(header.as_bytes());
}
fn entry(output: &mut Vec<u8>, header: &[u8; 512], body: &[u8]) {
    output.extend_from_slice(header);
    output.extend_from_slice(body);
    output.resize(output.len().next_multiple_of(512), 0);
}
fn archive(manifest: &Value, files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let manifest = serde_json::to_vec(manifest).unwrap();
    entry(&mut bytes, &header("package.json", manifest.len() as u64), &manifest);
    for (path, body) in files {
        entry(&mut bytes, &header(path, body.len() as u64), body);
    }
    bytes.resize(bytes.len() + 1024, 0);
    bytes
}
fn payload_offset(bytes: &[u8]) -> usize {
    let size = tar::Header::from_byte_slice(&bytes[..512]).entry_size().unwrap();
    512 + usize::try_from(size).unwrap().next_multiple_of(512)
}

#[test]
fn validates_manifest_and_streams_regular_payloads_in_any_order() {
    let files: &[(&str, &[u8])] = &[("bin/plugin", b"native payload"), ("data/empty", b""), ("data/catalog.json", b"{}")];
    let manifest = descriptor(files);
    let reversed: Vec<_> = files.iter().copied().rev().collect();
    let bytes = archive(&manifest, &reversed);
    let result = validate_archive(Cursor::new(&bytes)).unwrap();
    assert_eq!(result.manifest(), &serde_json::from_value::<PackageManifest>(manifest).unwrap());
    assert!(matches!(result.manifest().kind, PackageKind::Data {}));
    assert_eq!(result.payload_bytes(), files.iter().map(|(_, body)| body.len() as u64).sum::<u64>());
    assert_eq!(result.wire_bytes(), bytes.len() as u64);
}

#[test]
fn supports_raw_ustar_prefix_paths_without_normalizing_them() {
    let path = format!("{}/plugin", "a".repeat(110));
    let files: &[(&str, &[u8])] = &[(&path, b"payload")];
    validate_archive(Cursor::new(archive(&descriptor(files), files))).unwrap();
}

#[test]
fn manifest_must_be_first_exact_bounded_and_strict_json() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let good = archive(&descriptor(files), files);
    for path in ["Package.json", "other.json"] {
        let mut bytes = good.clone();
        let original = tar::Header::from_byte_slice(&bytes[..512]).entry_size().unwrap();
        bytes[..512].copy_from_slice(&header(path, original));
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::ManifestFirst));
    }
    for size in [0, MAX_PACKAGE_MANIFEST_BYTES as u64 + 1, u64::MAX >> 32] {
        let bytes = header("package.json", size);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::ManifestSize));
    }
    let mut invalid = descriptor(files);
    invalid["unexpected-secret-field"] = json!("sensitive-value");
    let error = validate_archive(Cursor::new(archive(&invalid, files))).unwrap_err();
    assert_eq!(error, ArchiveError::Manifest);
    assert!(!error.to_string().contains("sensitive"));
    assert!(!error.to_string().contains("secret"));
    let mut bytes = Vec::new();
    entry(&mut bytes, &header("package.json", 2), b"{}");
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Manifest));
}

#[test]
fn reserves_descriptor_path_and_descendants_case_insensitively() {
    for path in ["package.json", "PACKAGE.JSON", "Package.Json/child"] {
        let files: &[(&str, &[u8])] = &[(path, b"payload")];
        assert_eq!(validate_archive(Cursor::new(archive(&descriptor(files), files))), Err(ArchiveError::ReservedPath));
    }
}

#[test]
fn rejects_links_directories_devices_sparse_and_extension_records() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let good = archive(&descriptor(files), files);
    let offset = payload_offset(&good);
    for kind in [0, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'L', b'K', b'S', b'x', b'g', b'Z'] {
        let mut bytes = good.clone();
        bytes[offset + 156] = kind;
        checksum(&mut bytes[offset..offset + 512]);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Header), "{kind}");
    }
}

#[test]
fn raw_names_are_checked_before_any_tar_path_normalization() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let good = archive(&descriptor(files), files);
    let offset = payload_offset(&good);
    for name in [
        b"./data/file".as_slice(),
        b"../file",
        b"/data/file",
        b"data//file",
        b"data\\file",
        b"C:file",
        b"data\0junk",
        b"data/../file",
        b"CON.txt",
        b"\xff",
    ] {
        let mut bytes = good.clone();
        bytes[offset..offset + 100].fill(0);
        bytes[offset..offset + name.len()].copy_from_slice(name);
        checksum(&mut bytes[offset..offset + 512]);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Path), "{name:?}");
    }
    for prefix in [b"..".as_slice(), b"/root", b"safe\0junk"] {
        let mut bytes = good.clone();
        bytes[offset + 345..offset + 500].fill(0);
        bytes[offset + 345..offset + 345 + prefix.len()].copy_from_slice(prefix);
        checksum(&mut bytes[offset..offset + 512]);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Path));
    }
}

#[test]
fn rejects_unknown_header_metadata_numeric_extensions_and_bad_checksums() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let good = archive(&descriptor(files), files);
    let offset = payload_offset(&good);
    for (position, byte) in [
        (100, b'1'),
        (108, b'1'),
        (116, b'1'),
        (136, b'1'),
        (157, b'x'),
        (257, b'x'),
        (263, b'x'),
        (265, b'x'),
        (297, b'x'),
        (329, b'1'),
        (337, b'1'),
        (500, b'x'),
        (124, 0x80),
        (125, b' '),
        (135, b' '),
    ] {
        let mut bytes = good.clone();
        bytes[offset + position] = byte;
        checksum(&mut bytes[offset..offset + 512]);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Header), "{position}");
    }
    let mut bytes = good;
    bytes[offset + 1] ^= 1;
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Checksum));
}

#[test]
fn checks_exact_sizes_digests_and_each_declared_file_once() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload"), ("data/other", b"other")];
    let manifest = descriptor(files);
    let good = archive(&manifest, files);
    let offset = payload_offset(&good);
    let mut bytes = good.clone();
    bytes[offset + 512] ^= 1;
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Digest));
    let mut bytes = good.clone();
    bytes[offset..offset + 512].copy_from_slice(&header("data/file", 8));
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::FileSize));
    for replacement in ["data/undeclared", "DATA/file"] {
        let mut bytes = good.clone();
        bytes[offset..offset + 512].copy_from_slice(&header(replacement, 7));
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::UnexpectedFile));
    }
    assert_eq!(validate_archive(Cursor::new(archive(&manifest, &[files[0], files[0]]))), Err(ArchiveError::UnexpectedFile));
    assert_eq!(validate_archive(Cursor::new(archive(&manifest, &files[..1]))), Err(ArchiveError::MissingFile));
    // A third payload is not hidden behind the declared count.
    assert_eq!(validate_archive(Cursor::new(archive(&descriptor(&files[..1]), files))), Err(ArchiveError::Terminator));
}

#[test]
fn rejects_nonzero_padding_truncation_and_every_trailing_byte() {
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let good = archive(&descriptor(files), files);
    let offset = payload_offset(&good);
    for position in [offset - 1, offset + 512 + 7] {
        let mut bytes = good.clone();
        bytes[position] = 1;
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Padding));
    }
    for length in [0, 1, 511, 512, offset - 1, offset + 511, offset + 513, good.len() - 1024, good.len() - 512, good.len() - 1] {
        assert_eq!(validate_archive(Cursor::new(&good[..length])), Err(ArchiveError::Truncated), "{length}");
    }
    for trailing in [vec![0], vec![1], vec![0; 512], good.clone()] {
        let mut bytes = good.clone();
        bytes.extend(trailing);
        assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::TrailingData));
    }
    let mut bytes = good;
    let second_zero = bytes.len() - 512;
    bytes[second_zero] = 1;
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::Terminator));
}

struct Chunked<R> {
    reader: R,
    largest: Rc<Cell<usize>>,
    read_bytes: Rc<Cell<usize>>,
    interrupted: bool,
    max_chunk: usize,
}
impl<R: Read> Read for Chunked<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.largest.set(self.largest.get().max(buffer.len()));
        if self.interrupted {
            self.interrupted = false;
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        let count = buffer.len().min(self.max_chunk);
        let read = self.reader.read(&mut buffer[..count])?;
        self.read_bytes.set(self.read_bytes.get() + read);
        Ok(read)
    }
}
#[test]
fn handles_short_interrupted_reads_and_large_payloads_with_fixed_buffer_requests() {
    let body = vec![b'x'; 3 * 64 * 1024 + 17];
    let files: &[(&str, &[u8])] = &[("data/large", &body)];
    let bytes = archive(&descriptor(files), files);
    for max_chunk in [1, 97] {
        let largest = Rc::new(Cell::new(0));
        let read_bytes = Rc::new(Cell::new(0));
        let reader = Chunked {
            reader: Cursor::new(&bytes),
            largest: Rc::clone(&largest),
            read_bytes: Rc::clone(&read_bytes),
            interrupted: true,
            max_chunk,
        };
        let result = validate_archive(reader).unwrap();
        assert_eq!(result.wire_bytes(), read_bytes.get() as u64);
        assert!(largest.get() <= 64 * 1024);
        assert_eq!(read_bytes.get(), bytes.len());
    }
}

#[test]
fn read_errors_and_declared_size_attacks_fail_before_streaming_attacker_lengths() {
    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("sensitive reader detail"))
        }
    }
    assert_eq!(validate_archive(Failing), Err(ArchiveError::Read));
    assert_eq!(ArchiveError::Read.to_string(), "package archive input read failed");
    let files: &[(&str, &[u8])] = &[("data/file", b"payload")];
    let mut bytes = archive(&descriptor(files), files);
    let offset = payload_offset(&bytes);
    bytes[offset..offset + 512].copy_from_slice(&header("data/file", 8_000_000_000));
    bytes.truncate(offset + 512);
    assert_eq!(validate_archive(Cursor::new(bytes)), Err(ArchiveError::FileSize));
}
