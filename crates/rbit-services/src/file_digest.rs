//! SHA-256 of files at rest, for checking stored blobs against the digests the database
//! records for them (backups and restores).

use std::{io, path::Path};

use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

/// The size and SHA-256 of the file at `path`, read once in 64 KiB pieces.
///
/// # Errors
///
/// What opening or reading the file fails with.
pub async fn sha256(path: &Path) -> io::Result<(u64, [u8; 32])> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((size, hasher.finalize().into()))
}

/// `digest` as 64 lowercase hex digits.
pub fn hex(digest: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    digest.iter().fold(String::with_capacity(64), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn digests_match_the_known_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        tokio::fs::write(&path, b"abc").await.unwrap();
        let (size, digest) = sha256(&path).await.unwrap();
        assert_eq!(size, 3);
        assert_eq!(hex(&digest), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
