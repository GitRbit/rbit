//! Fuzz-only oracles and bounded, checksum-aware archive generation.
use rbit_plugin_packages::validate_archive;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    io::{self, Cursor, Read},
};

struct Chunked<'a> {
    input: Cursor<&'a [u8]>,
    chunk: usize,
    calls: usize,
}
impl Read for Chunked<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        if self.calls % 37 == 1 {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let len = bytes.len().min(self.chunk);
        self.input.read(&mut bytes[..len])
    }
}

pub fn check(data: &[u8]) -> bool {
    let plain = validate_archive(Cursor::new(data));
    let chunk = if data.len() < 4096 { 1 + usize::from(data.first().copied().unwrap_or(0) % 97) } else { 97 };
    let fragmented = validate_archive(Chunked { input: Cursor::new(data), chunk, calls: 0 });
    assert_eq!(plain, fragmented, "reader fragmentation or Interrupted changed validation");
    if let Ok(result) = &plain {
        result.manifest().validate().unwrap();
        assert_eq!(result.wire_bytes(), data.len() as u64);
        assert_eq!(result.payload_bytes(), result.manifest().files.iter().map(|f| f.size).sum::<u64>());
    }
    plain.is_ok()
}

fn header(path: &str, size: usize) -> [u8; 512] {
    let mut h = tar::Header::new_ustar();
    h.set_mode(0o644);
    h.set_uid(0);
    h.set_gid(0);
    h.set_mtime(0);
    h.set_size(size as u64);
    h.set_entry_type(tar::EntryType::Regular);
    h.set_path(path).unwrap();
    h.set_cksum();
    *h.as_bytes()
}
fn entry(out: &mut Vec<u8>, path: &str, body: &[u8]) {
    out.extend_from_slice(&header(path, body.len()));
    out.extend_from_slice(body);
    out.resize(out.len().next_multiple_of(512), 0);
}
fn repair(bytes: &mut [u8]) {
    let mut h = tar::Header::from_byte_slice(bytes).clone();
    h.set_cksum();
    bytes.copy_from_slice(h.as_bytes());
}

/// Input controls select a bounded valid archive or one deliberately invalid mutation.
/// All generation bounds are independent of declared attacker-controlled sizes.
pub fn structured(data: &[u8]) -> (Vec<u8>, bool) {
    let selector = data.first().copied().unwrap_or(0) % 20;
    let adapter = data.get(3).copied().unwrap_or(0) % 3;
    let length = [0, 1, 511, 512, 513, 65535, 65536, 65537][usize::from(data.get(1).copied().unwrap_or(0) % 8)];
    let length = if adapter == 0 { length } else { length.max(1) };
    let body: Vec<_> = (0..length).map(|i| data.get(2 + i % data.len().saturating_sub(2).max(1)).copied().unwrap_or(b'x')).collect();
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(&body) {
        write!(digest, "{byte:02x}").unwrap();
    }
    let path = if data.get(2).copied().unwrap_or(0) & 1 == 0 { "data/file".to_owned() } else { format!("{}/file", "p".repeat(110)) };
    let kind = match adapter {
        0 => json!({"type":"data"}),
        1 => json!({"type":"native","target":"aarch64-apple-darwin","entrypoint":path}),
        _ => json!({"type":"wasm","entrypoint":path}),
    };
    let manifest = json!({"schema_version":1,"plugin":{"id":"example.fuzz","version":"1.0.0","host_api":"^1"},"kind":kind,"files":[{"path":path,"size":body.len(),"sha256":digest}],"budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}});
    let descriptor = serde_json::to_vec(&manifest).unwrap();
    let mut out = Vec::new();
    entry(&mut out, "package.json", &descriptor);
    let payload = out.len();
    entry(&mut out, &path, &body);
    out.resize(out.len() + 1024, 0);
    match selector {
        0 => {}
        1..=7 => {
            out[payload + 156] = [b'1', b'2', b'5', b'L', b'x', b'g', b'S'][usize::from(selector - 1)];
            repair(&mut out[payload..payload + 512]);
        }
        8 => {
            if body.is_empty() {
                out[payload + 148] ^= 1;
            } else {
                out[payload + 512] ^= 1;
            }
        }
        9 => {
            out[payload + 124..payload + 136].copy_from_slice(b"00000000001\0");
            if length == 1 {
                out[payload + 134] = b'2';
            }
            repair(&mut out[payload..payload + 512]);
        }
        10 => {
            let len = usize::from(data.get(3).copied().unwrap_or(1)) % out.len();
            out.truncate(len);
        }
        11 => out.push(data.get(3).copied().unwrap_or(0)),
        12 => {
            out[payload - 1] = 1;
        }
        13 => {
            let p = out.len() - 512;
            out[p] = 1;
        }
        14 => {
            let p = out.len() - 1024;
            let extra = out[payload..p].to_vec();
            out.splice(p..p, extra);
        }
        15 => {
            out[512] = 0;
        }
        16 => {
            out[payload..payload + 100].fill(0);
            out[payload..payload + 7].copy_from_slice(b"../file");
            repair(&mut out[payload..payload + 512]);
        }
        17 => {
            out[payload + 345..payload + 500].fill(0);
            out[payload + 345..payload + 347].copy_from_slice(b"..");
            repair(&mut out[payload..payload + 512]);
        }
        18 => {
            out[payload + 124] = 0x80;
            repair(&mut out[payload..payload + 512]);
        }
        19 => {
            out[..100].fill(0);
            out[..5].copy_from_slice(b"other");
            repair(&mut out[..512]);
        }
        _ => unreachable!(),
    }
    (out, selector == 0)
}
