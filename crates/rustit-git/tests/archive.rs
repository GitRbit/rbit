//! Archives produced by `git archive`, listed by parsing the zip central directory and the
//! decompressed tar stream.

use std::{path::Path, process::Command};

use async_compression::tokio::bufread::GzipDecoder;
use rustit_git::{
    RepositoryReader,
    archive::{ArchiveFormat, Archiver},
};
use tokio::io::{AsyncReadExt, BufReader};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git is installed");
    assert!(status.success(), "git {args:?} failed");
}

fn fixture() -> (tempfile::TempDir, RepositoryReader, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("README.md"), "# Demo\n").unwrap();
    std::fs::write(root.join("docs/user guide.txt"), "with spaces\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "first"]);
    let git_dir = root.join(".git");
    (dir, RepositoryReader::new(git_dir.clone()), git_dir)
}

async fn produce(git_dir: &Path, commit: &str, format: ArchiveFormat) -> Vec<u8> {
    let mut stream = Archiver::new("git").start(git_dir, commit, "demo-main", format).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    bytes
}

fn u16_at(bytes: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> usize {
    usize::try_from(u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])).unwrap()
}

/// Entry names from the zip's central directory.
fn zip_names(zip: &[u8]) -> Vec<String> {
    let end = (0..=zip.len() - 22).rev().find(|&i| zip[i..i + 4] == [0x50, 0x4b, 0x05, 0x06]).expect("end of central directory");
    let count = u16_at(zip, end + 10);
    let mut at = u32_at(zip, end + 16);
    let mut names = Vec::new();
    for _ in 0..count {
        assert_eq!(zip[at..at + 4], [0x50, 0x4b, 0x01, 0x02], "central directory entry");
        let (name_len, extra_len, comment_len) = (u16_at(zip, at + 28), u16_at(zip, at + 30), u16_at(zip, at + 32));
        names.push(String::from_utf8(zip[at + 46..at + 46 + name_len].to_vec()).unwrap());
        at += 46 + name_len + extra_len + comment_len;
    }
    names
}

/// Entry names of a ustar stream (pax headers skipped).
fn tar_names(tar: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0;
    while at + 512 <= tar.len() && tar[at] != 0 {
        let header = &tar[at..at + 512];
        let field = |range: std::ops::Range<usize>| String::from_utf8_lossy(&header[range]).trim_end_matches('\0').to_owned();
        let size = usize::from_str_radix(field(124..136).trim(), 8).unwrap();
        if !matches!(header[156], b'g' | b'x') {
            let prefix = field(345..500);
            let name = field(0..100);
            names.push(if prefix.is_empty() { name } else { format!("{prefix}/{name}") });
        }
        at += 512 + size.div_ceil(512) * 512;
    }
    names
}

#[tokio::test]
async fn zip_archives_hold_every_file_under_the_prefix() {
    let (_dir, reader, git_dir) = fixture();
    let commit = reader.commit_id("main".into()).await.unwrap();
    let zip = produce(&git_dir, &commit, ArchiveFormat::Zip).await;
    let names = zip_names(&zip);
    for expected in ["demo-main/", "demo-main/README.md", "demo-main/docs/user guide.txt"] {
        assert!(names.iter().any(|n| n == expected), "{expected} missing from {names:?}");
    }
}

#[tokio::test]
async fn tar_gz_archives_are_gzipped_tarballs_of_the_commit() {
    let (_dir, reader, git_dir) = fixture();
    let commit = reader.commit_id("main".into()).await.unwrap();
    let gz = produce(&git_dir, &commit, ArchiveFormat::TarGz).await;
    assert_eq!(gz[..2], [0x1f, 0x8b], "gzip magic");

    let mut tar = Vec::new();
    GzipDecoder::new(BufReader::new(gz.as_slice())).read_to_end(&mut tar).await.unwrap();
    let names = tar_names(&tar);
    for expected in ["demo-main/README.md", "demo-main/docs/user guide.txt"] {
        assert!(names.iter().any(|n| n == expected), "{expected} missing from {names:?}");
    }
}

#[tokio::test]
async fn a_failing_archive_ends_in_an_error_not_a_short_file() {
    let (_dir, _reader, git_dir) = fixture();
    let mut stream = Archiver::new("git").start(&git_dir, &"0".repeat(40), "x", ArchiveFormat::Zip).unwrap();
    let mut bytes = Vec::new();
    assert!(stream.read_to_end(&mut bytes).await.is_err(), "git archive of a missing commit must fail the stream");
}
