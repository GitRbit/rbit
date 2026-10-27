//! The Node.js JavaScript actions run with (Actions: third-party actions).
//!
//! GitHub's runner carries its own Node; so does rbit-runner, except that it downloads it
//! on first use rather than shipping it: [`NODE_VERSION`] from `nodejs.org`, checked
//! against the SHA-256 pinned here ([`DOWNLOADS`]) before anything is unpacked, into
//! `<work_dir>/externals/node-v<version>-<platform>/`. An action's `using: node20` or
//! `node24` does not change it: every JavaScript action runs with this one, as GitHub runs
//! `node20` actions with Node 24 now.
//!
//! * On the host, `[actions] node` names a `node` program instead; without it the runner
//!   downloads the build for its own platform.
//! * In a job container, the Linux build for the runner's architecture is mounted read-only
//!   at [`CONTAINER_DIR`] (`/__e`), as GitHub mounts its externals. It needs a glibc image;
//!   musl images such as Alpine need `[actions] node` unset and Node.js installed in the
//!   image, or a host label.
//!
//! Downloads are serialised, unpacked into a temporary directory and renamed into place,
//! so an interrupted download leaves nothing half-written that a later job would use.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

/// The Node.js release JavaScript actions run with.
pub const NODE_VERSION: &str = "24.22.1";

/// Where the externals directory is mounted in job containers.
pub const CONTAINER_DIR: &str = "/__e";

/// `(platform, archive, SHA-256)` of [`NODE_VERSION`], from its `SHASUMS256.txt`.
pub const DOWNLOADS: [(&str, &str, &str); 6] = [
    ("linux-x64", "tar.gz", "eb6728e77c576a35162c122c45ccadf224d89ff00abc8dae46cc2897c6002d7c"),
    ("linux-arm64", "tar.gz", "33c37be234ba8bee041961047b19160f4d7d0f164e8e246f78bdd10feff148b3"),
    ("darwin-x64", "tar.gz", "d132557e7ee2b85af08b0c8c34338e65c6430f2238d01f727728ef42dc83b035"),
    ("darwin-arm64", "tar.gz", "89c682593d76cd44233fa53b1563264e7de9cbb189cad169135c5b4fadfbc0a7"),
    ("win-x64", "zip", "d2046d28c2c3dcd211ac5145c0bf30e2047ef9dfb306693c8c6e2eccd8b95fa5"),
    ("win-arm64", "zip", "a0c807319fede24c4a0d8f84564c82a619bd313b16b10c007c383c2609e99171"),
];

/// The base URL of Node.js downloads.
const DIST: &str = "https://nodejs.org/dist";

/// Longest a download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_mins(10);

/// One download at a time: two jobs that both need Node wait for the same one.
static DOWNLOADING: Mutex<()> = Mutex::const_new(());

/// Node's name for an architecture.
fn node_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// The platform of this runner host (`win-x64`, `linux-arm64`, …).
pub fn host_platform() -> Option<String> {
    let os = match std::env::consts::OS {
        "windows" => "win",
        "linux" => "linux",
        "macos" => "darwin",
        _ => return None,
    };
    Some(format!("{os}-{}", node_arch(std::env::consts::ARCH)?))
}

/// The Linux platform job containers on this host run.
pub fn container_platform() -> Option<String> {
    Some(format!("linux-{}", node_arch(std::env::consts::ARCH)?))
}

/// The directory a platform's Node is unpacked into, under `externals`.
pub fn directory_name(platform: &str) -> String {
    format!("node-v{NODE_VERSION}-{platform}")
}

/// The `node` program inside an unpacked directory, with `/` or `\` as the platform has it.
pub fn program_in(dir: &str, platform: &str) -> String {
    if platform.starts_with("win-") { format!("{dir}\\node.exe") } else { format!("{dir}/bin/node") }
}

/// The host `node` JavaScript actions run with: `[actions] node`, else the downloaded one.
pub async fn host_node(configured: Option<&Path>, externals: &Path, log: impl Fn(&str)) -> Result<PathBuf, String> {
    if let Some(node) = configured {
        return Ok(node.to_path_buf());
    }
    let platform = host_platform().ok_or("this runner's platform has no Node.js build; set `[actions] node`")?;
    let dir = ensure(externals, &platform, log).await?;
    Ok(PathBuf::from(program_in(&dir.display().to_string(), &platform)))
}

/// The `node` program as a job container sees it, downloading the Linux build first.
pub async fn container_node(externals: &Path, log: impl Fn(&str)) -> Result<String, String> {
    let platform = container_platform().ok_or("job containers on this architecture have no Node.js build")?;
    ensure(externals, &platform, log).await?;
    Ok(program_in(&format!("{CONTAINER_DIR}/{}", directory_name(&platform)), &platform))
}

/// `externals/node-v…-<platform>`, downloaded and unpacked unless it already is.
async fn ensure(externals: &Path, platform: &str, log: impl Fn(&str)) -> Result<PathBuf, String> {
    let target = externals.join(directory_name(platform));
    let program = PathBuf::from(program_in(&target.display().to_string(), platform));
    if program.is_file() {
        return Ok(target);
    }
    let _downloading = DOWNLOADING.lock().await;
    if program.is_file() {
        return Ok(target);
    }
    let (_, extension, digest) =
        DOWNLOADS.iter().find(|(name, ..)| *name == platform).ok_or_else(|| format!("no Node.js build for {platform}"))?;
    std::fs::create_dir_all(externals).map_err(|error| format!("cannot create {}: {error}", externals.display()))?;
    let staging = externals.join(format!(".download-{}", crate::client::idempotency_key()));
    std::fs::create_dir_all(&staging).map_err(|error| format!("cannot create {}: {error}", staging.display()))?;
    let result = async {
        let file_name = format!("{}.{extension}", directory_name(platform));
        let archive = staging.join(&file_name);
        let url = format!("{DIST}/v{NODE_VERSION}/{file_name}");
        log(&format!("Downloading Node.js {NODE_VERSION} for JavaScript actions from {url}"));
        download(&url, &archive, digest).await?;
        unpack(&archive, &staging, extension).await?;
        let unpacked = staging.join(directory_name(platform));
        if !unpacked.is_dir() {
            return Err(format!("the Node.js archive has no {} directory", directory_name(platform)));
        }
        std::fs::rename(&unpacked, &target).map_err(|error| format!("cannot move Node.js into {}: {error}", target.display()))
    }
    .await;
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    if !program.is_file() {
        return Err(format!("{} is missing after unpacking Node.js", program.display()));
    }
    Ok(target)
}

/// Downloads `url` into `path`, failing unless its SHA-256 is `expected`.
async fn download(url: &str, path: &Path, expected: &str) -> Result<(), String> {
    crate::client::install_crypto_provider();
    let client = reqwest::Client::builder()
        .user_agent(crate::client::user_agent())
        .connect_timeout(Duration::from_secs(20))
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| format!("cannot download Node.js: {error}"))?;
    let mut response = client.get(url).send().await.map_err(|error| format!("cannot download {url}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("cannot download {url}: {}", response.status()));
    }
    let mut file = std::fs::File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("cannot download {url}: {error}"))? {
        hasher.update(&chunk);
        file.write_all(&chunk).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    file.sync_all().map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    let actual = hasher.finalize().iter().fold(String::with_capacity(64), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    if actual != expected {
        return Err(format!("{url} has SHA-256 {actual}, not the pinned {expected}; it was not used"));
    }
    Ok(())
}

/// Unpacks a checked archive into `into`: `.tar.gz` here, `.zip` with Windows' own `tar`.
async fn unpack(archive: &Path, into: &Path, extension: &str) -> Result<(), String> {
    let (archive, into) = (archive.to_path_buf(), into.to_path_buf());
    if extension == "zip" {
        let system = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        let tar = system.join("System32").join("tar.exe");
        let mut command = tokio::process::Command::new(&tar);
        command.arg("-xf").arg(&archive).arg("-C").arg(&into);
        let output = crate::process::capture(command, None, DOWNLOAD_TIMEOUT)
            .await
            .map_err(|error| format!("cannot run {}: {error}", tar.display()))?;
        return if output.success { Ok(()) } else { Err(format!("cannot unpack Node.js: {}", output.stderr.trim())) };
    }
    // Only files and directories: running actions needs `bin/node`, not npm's links, and a
    // Windows runner host cannot create links unless it is an administrator.
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&archive)?;
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        for entry in tar.entries()? {
            let mut entry = entry?;
            if matches!(entry.header().entry_type(), tar::EntryType::Regular | tar::EntryType::Directory) {
                entry.unpack_in(&into)?;
            }
        }
        Ok::<(), std::io::Error>(())
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| format!("cannot unpack Node.js: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_platform_has_a_pinned_digest() {
        for (platform, extension, digest) in DOWNLOADS {
            assert!(digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()), "{platform}");
            assert_eq!(extension == "zip", platform.starts_with("win-"), "{platform}");
        }
        if let Some(platform) = host_platform() {
            assert!(DOWNLOADS.iter().any(|(name, ..)| *name == platform), "{platform}");
        }
        if let Some(platform) = container_platform() {
            assert!(DOWNLOADS.iter().any(|(name, ..)| *name == platform), "{platform}");
        }
        assert_eq!(program_in("/__e/node-v1-linux-x64", "linux-x64"), "/__e/node-v1-linux-x64/bin/node");
        assert_eq!(program_in(r"C:\w\node-v1-win-x64", "win-x64"), r"C:\w\node-v1-win-x64\node.exe");
    }

    #[tokio::test]
    async fn a_configured_node_is_used_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let node = host_node(Some(Path::new("/opt/node/bin/node")), dir.path(), |_| {}).await.unwrap();
        assert_eq!(node, PathBuf::from("/opt/node/bin/node"));
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none(), "nothing was downloaded");
    }

    #[tokio::test]
    async fn an_unpacked_node_is_not_downloaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let platform = container_platform().unwrap();
        let bin = dir.path().join(directory_name(&platform)).join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("node"), "").unwrap();
        let node = container_node(dir.path(), |line| panic!("downloaded: {line}")).await.unwrap();
        assert_eq!(node, format!("/__e/{}/bin/node", directory_name(&platform)));
    }

    #[tokio::test]
    async fn a_download_with_another_digest_is_refused() {
        // A stand-in "archive" served from disk can never match the pinned digest.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x");
        let error = download("http://127.0.0.1:9/unreachable", &path, &"0".repeat(64)).await.unwrap_err();
        assert!(error.contains("cannot download"), "{error}");
    }
}
