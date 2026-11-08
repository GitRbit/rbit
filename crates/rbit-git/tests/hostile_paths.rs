//! Hostile paths: edit paths that are options, Git metadata spellings or traversal;
//! archives of trees whose entries traverse or link outward; directory links planted in a
//! repository that is moved and deleted; and an upload cancelled while it streams.

use std::{path::Path, time::Duration};

use async_compression::tokio::bufread::GzipDecoder;
use rbit_core::{AccountId, RepoId, Timestamp, file_edit::FilePath, pull::CommitIdentity};
use rbit_git::{
    RepositoryStorage,
    archive::{ArchiveFormat, Archiver},
    edit::{Change, EditError, EditSpec, EditWorkspace},
    relocate::relocate,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};

use crate::hostile_support::{Repo, link_directory, precious_outside};

fn author() -> CommitIdentity {
    CommitIdentity { name: "Ada".into(), email: "ada@example.com".into() }
}

/// Commits new files at paths to `main`.
async fn create(repo: &Repo, paths: &[FilePath]) -> Result<String, EditError> {
    let main = repo.tip("main");
    let workspace = EditWorkspace::open(&repo.env(), &repo.bare).await.unwrap();
    let blob = workspace.write_blob(&b"x\n"[..], 1024).await.unwrap();
    let changes: Vec<_> = paths.iter().map(|path| Change::Create { path: path.clone(), blob: blob.clone() }).collect();
    let spec = EditSpec {
        branch: "main",
        parent: &main,
        new_branch: None,
        changes: &changes,
        message: "m\n",
        author: &author(),
        at: Timestamp(1),
    };
    workspace.commit(&spec).await.map(|committed| committed.commit)
}

#[tokio::test]
async fn edit_paths_never_become_options_git_metadata_or_traversal() {
    let repo = Repo::new(&[("README.md", b"# Demo\n")]);
    for refused in [
        "../outside",
        "a/../../b",
        "/etc/passwd",
        "a\\..\\b",
        "a\0b",
        "a\u{1b}b",
        "C:\\x",
        ".git/config",
        "GIT~1/config",
        ".git. ./hooks/x",
    ] {
        assert!(FilePath::parse(refused).is_err(), "{refused:?}");
    }
    // Accepted by the path type; Git skips them as NTFS spellings of `.git`. Next to
    // an ordinary file, the edit is refused as a whole: no change is silently dropped.
    let main = repo.tip("main");
    let kept = FilePath::parse("kept.txt").unwrap();
    for spelling in [".git::$INDEX_ALLOCATION/config", ".git:stream/config"] {
        let path = FilePath::parse(spelling).unwrap();
        let result = create(&repo, &[kept.clone(), path]).await;
        assert!(matches!(result, Err(EditError::Rejected(_))), "{spelling:?}: {result:?}");
    }
    assert_eq!(repo.tip("main"), main, "nothing was committed");

    let cwd = std::env::current_dir().unwrap();
    for option in ["--output=pwned", "-rf", "--exec=touch"] {
        let commit = create(&repo, &[FilePath::parse(option).unwrap()]).await.unwrap();
        let listed = repo.bare(&["ls-tree", "--name-only", &commit]);
        assert!(listed.lines().any(|name| name == option), "{option:?} is an ordinary file name: {listed}");
    }
    assert!(!cwd.join("pwned").exists() && !repo.dir.path().join("pwned").exists(), "no path became an option");
}

#[derive(Debug)]
struct RawTree(Vec<u8>);

impl gix::objs::WriteTo for RawTree {
    fn write_to(&self, output: &mut dyn std::io::Write) -> std::io::Result<()> {
        output.write_all(&self.0)
    }
    fn kind(&self) -> gix::objs::Kind {
        gix::objs::Kind::Tree
    }
    fn size(&self) -> u64 {
        u64::try_from(self.0.len()).unwrap()
    }
}

/// A commit of a tree with the raw `entries` (mode, name, blob content), which Git's own
/// tools would refuse to write.
fn raw_commit(bare: &Path, entries: &[(&str, &str, &[u8])]) -> String {
    let repo = gix::open_opts(bare, gix::open::Options::isolated()).unwrap();
    let mut bytes = Vec::new();
    for (mode, name, content) in entries {
        let blob = repo.write_blob(content).unwrap().detach();
        bytes.extend_from_slice(format!("{mode} {name}\0").as_bytes());
        bytes.extend_from_slice(blob.as_slice());
    }
    let tree = repo.write_object(RawTree(bytes)).unwrap().detach();
    let signature =
        gix::actor::Signature { name: "Eve".into(), email: "eve@example.com".into(), time: gix::date::Time { seconds: 1, offset: 0 } };
    let commit = gix::objs::Commit {
        tree,
        parents: std::iter::empty().collect(),
        author: signature.clone(),
        committer: signature,
        encoding: None,
        message: "hostile tree".into(),
        extra_headers: Vec::new(),
    };
    repo.write_object(commit).unwrap().detach().to_hex().to_string()
}

async fn read_archive(repo: &Repo, commit: &str, format: ArchiveFormat) -> std::io::Result<Vec<u8>> {
    let archiver = Archiver::new("git", repo.dir.path().join("tmp"));
    let mut stream = archiver.start(&repo.bare, commit, "demo", format).await.unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await?;
    Ok(bytes)
}

/// (name, type flag, link target) of every entry of a ustar stream, pax headers skipped.
fn tar_entries(tar: &[u8]) -> Vec<(String, u8, String)> {
    let mut entries = Vec::new();
    let mut at = 0;
    while at + 512 <= tar.len() && tar[at] != 0 {
        let header = &tar[at..at + 512];
        let field = |range: std::ops::Range<usize>| String::from_utf8_lossy(&header[range]).trim_end_matches('\0').to_owned();
        let size = usize::from_str_radix(field(124..136).trim(), 8).unwrap();
        if !matches!(header[156], b'g' | b'x') {
            let (prefix, name) = (field(345..500), field(0..100));
            entries.push((if prefix.is_empty() { name } else { format!("{prefix}/{name}") }, header[156], field(157..257)));
        }
        at += 512 + size.div_ceil(512) * 512;
    }
    entries
}

#[tokio::test]
async fn archives_never_hold_entries_outside_their_prefix_or_host_files() {
    let repo = Repo::new(&[("README.md", b"# Demo\n")]);
    for traversal in [vec![("100644", "..", b"x".as_slice())], vec![("100644", "a/../../escape", b"x")], vec![("100644", ".git", b"x")]] {
        let commit = raw_commit(&repo.bare, &traversal);
        for format in ArchiveFormat::ALL {
            let result = read_archive(&repo, &commit, format).await;
            assert!(result.is_err(), "{traversal:?} {format:?}: git refuses the tree, so the download fails");
        }
    }

    let (outside, secret) = precious_outside();
    let absolute = secret.to_string_lossy().into_owned();
    let relative = "../../../../../../../../precious.txt";
    let commit = raw_commit(&repo.bare, &[("120000", "abs", absolute.as_bytes()), ("120000", "up", relative.as_bytes())]);
    let gz = read_archive(&repo, &commit, ArchiveFormat::TarGz).await.unwrap();
    let mut tar = Vec::new();
    GzipDecoder::new(BufReader::new(gz.as_slice())).read_to_end(&mut tar).await.unwrap();
    let entries = tar_entries(&tar);
    assert!(entries.contains(&("demo/abs".to_owned(), b'2', absolute.clone())), "{entries:?}");
    assert!(entries.contains(&("demo/up".to_owned(), b'2', relative.to_owned())), "{entries:?}");
    assert!(entries.iter().all(|(name, ..)| name.starts_with("demo/") && !name.split('/').any(|part| part == "..")), "{entries:?}");
    let zip = read_archive(&repo, &commit, ArchiveFormat::Zip).await.unwrap();
    for bytes in [&tar, &zip] {
        assert!(!bytes.windows(7).any(|window| window == b"keep me"), "a link is archived as a link, never the host file");
    }
    drop(outside);
}

#[tokio::test]
async fn moving_and_deleting_a_repository_never_follows_links_out_of_it() {
    let root = tempfile::tempdir().unwrap();
    let storage = RepositoryStorage::new(root.path().join("repos"));
    let path = storage.create(AccountId(1), RepoId(7), "main").await.unwrap();
    let (outside, precious) = precious_outside();
    link_directory(outside.path(), &path.join("planted"));
    link_directory(outside.path(), &path.join("objects").join("zz"));

    let moved = relocate(&storage, RepoId(7), AccountId(1), AccountId(2)).await.unwrap();
    assert!(moved.join("planted").join("precious.txt").exists(), "the link moved with the repository");
    storage.delete(AccountId(2), RepoId(7)).await.unwrap();
    assert!(!moved.exists());
    assert_eq!(std::fs::read_to_string(&precious).unwrap(), "keep me", "deletion removed the links, not their targets");
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
}

fn object_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap())
        .map(|entry| if entry.file_type().unwrap().is_dir() { object_files(&entry.path()) } else { 1 })
        .sum()
}

#[tokio::test]
async fn a_cancelled_upload_leaves_no_partial_object_or_lock() {
    let repo = Repo::new(&[("README.md", b"# Demo\n")]);
    let objects = object_files(&repo.bare.join("objects"));
    let merges = repo.env().tmp_dir.join("merges");
    let (mut client, body) = tokio::io::duplex(64);
    let (env, bare) = (repo.env(), repo.bare.clone());
    let upload = tokio::spawn(async move {
        let workspace = EditWorkspace::open(&env, &bare).await.unwrap();
        workspace.write_blob(body, 1 << 20).await
    });
    client.write_all(b"the first part of an upload").await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !std::fs::read_dir(&merges).is_ok_and(|mut entries| entries.next().is_some()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    upload.abort();
    assert!(upload.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(10), async {
        while std::fs::read_dir(&merges).is_ok_and(|mut entries| entries.next().is_some()) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the cancelled workspace is removed");
    assert_eq!(object_files(&repo.bare.join("objects")), objects, "no object reached the repository");
    drop(client);
    create(&repo, &[FilePath::parse("after.txt").unwrap()]).await.expect("nothing was left locked");
}
