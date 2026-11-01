use rbit_plugin_api::InstalledPackageRef;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};
pub const PROBE: &[u8] = b"native probe fixture; intentionally not executable";
pub const CONFIG: &[u8] = b"independent nested configuration bytes";
pub fn digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}
pub fn native() -> Value {
    json!({"type":"native","target":"aarch64-apple-darwin","entrypoint":"bin/probe"})
}
pub fn archive(kind: &Value) -> Vec<u8> {
    let files = [("bin/probe", PROBE), ("cfg/settings", CONFIG)];
    let manifest = serde_json::to_vec(&json!({
        "schema_version":1,"plugin":{"id":"unknown.materializer","version":"1.0.0+exact","host_api":"^1"},
        "kind":kind,"files":files.iter().map(|(path,body)| json!({"path":path,"size":body.len(),"sha256":digest(body)})).collect::<Vec<_>>(),
        "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
    })).unwrap();
    let mut output = Vec::new();
    for (path, body) in [("package.json", manifest.as_slice()), files[0], files[1]] {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        output.extend_from_slice(header.as_bytes());
        output.extend_from_slice(body);
        output.resize(output.len().next_multiple_of(512), 0);
    }
    output.resize(output.len() + 1024, 0);
    output
}
pub fn expected(bytes: &[u8]) -> InstalledPackageRef {
    serde_json::from_value(json!({"plugin_id":"unknown.materializer","version":"1.0.0+exact","archive_digest":digest(bytes)})).unwrap()
}
pub fn private_root() -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
}
pub fn children(path: &Path) -> Vec<PathBuf> {
    fs::read_dir(path).unwrap().map(|entry| entry.unwrap().path()).collect()
}
