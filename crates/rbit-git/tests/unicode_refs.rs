//! Existing Git ref identity and macOS precomposition survive table upgrades.
use crate::hostile_support::{Repo, git};
use rbit_git::RepositoryReader;
use std::fmt::Write as _;

fn refs(repo: &Repo, loose: &[&str], packed: &[(&str, &str)]) {
    let tip = repo.tip("main");
    for name in loose {
        let path = repo.bare.join("refs/heads").join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("{tip}\n")).unwrap();
    }
    let mut entries = packed.to_vec();
    entries.push(("main", &tip));
    entries.sort_by_key(|entry| entry.0);
    let mut bytes = "# pack-refs with: peeled fully-peeled sorted\n".to_owned();
    for (name, id) in entries {
        writeln!(bytes, "{id} refs/heads/{name}").unwrap();
    }
    std::fs::write(repo.bare.join("packed-refs"), bytes).unwrap();
}

#[tokio::test]
async fn unicode17_packed_ref_identities_remain_distinct_for_plain_and_pinned_readers() {
    let repo = Repo::new(&[("README.md", b"approved\n")]);
    repo.config("core.precomposeUnicode", "true");
    let first = repo.tip("main");
    git(&repo.work(), &["commit", "-q", "--allow-empty", "-m", "second identity"]);
    repo.bare(&["fetch", "-q", repo.work().to_str().unwrap(), "main:refs/heads/other-source"]);
    let second = repo.tip("other-source");
    assert_ne!(first, second);
    let original = "α\u{1ade}\u{0345}";
    let distinct = "ᾳ\u{1ade}";
    // These are valid imported/transported packed refs even on filesystems that
    // cannot represent the first spelling as a loose filename. Unicode 18 would
    // compose across the newly assigned mark and resolve it to the second ref.
    refs(&repo, &[], &[(original, &first), (distinct, &second)]);
    assert_eq!(repo.tip(original), first);
    assert_eq!(repo.tip(distinct), second);
    let plain = RepositoryReader::new(repo.bare.clone());
    for reader in [plain.clone(), plain.pinned()] {
        for _ in 0..2 {
            assert_eq!(reader.commit_id(original.into()).await.unwrap(), first);
            assert_eq!(reader.commit_id(distinct.into()).await.unwrap(), second);
            assert_eq!(reader.split_revision_path(format!("{original}/README.md")).await.unwrap(), (original.into(), "README.md".into()));
            assert_eq!(reader.blob(original.into(), "README.md".into(), 1024).await.unwrap().text(), Some("approved\n"));
            reader.close();
        }
    }
}

async fn native_controls(repo: &Repo, queries: &[&str]) {
    let expected = repo.bare(&["for-each-ref", "--format=%(refname:short)", "refs/heads"]);
    let mut expected = expected.lines().map(str::to_owned).collect::<Vec<_>>();
    expected.sort();
    let plain = RepositoryReader::new(repo.bare.clone());
    for reader in [plain.clone(), plain.pinned()] {
        let mut actual = reader.refs().await.unwrap().into_iter().map(|reference| reference.name).collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, expected);
        for name in queries {
            assert_eq!(reader.commit_id((*name).into()).await.unwrap(), repo.tip(name));
        }
    }
}

#[tokio::test]
async fn unicode17_raw_nfc_nfd_loose_and_packed_controls_agree_with_native_git() {
    let repo = Repo::new(&[("README.md", b"control\n")]);
    repo.config("core.precomposeUnicode", "false");
    let tip = repo.tip("main");
    let loose = ["loose/Café", "loose/The\u{0301}me"];
    let packed = [("packed/Café", tip.as_str()), ("packed/The\u{0301}me", tip.as_str())];
    refs(&repo, &loose, &packed);
    native_controls(&repo, &[loose[0], loose[1], packed[0].0, packed[1].0]).await;
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn unicode17_macos_nfc_nfd_controls_keep_precomposition() {
    let repo = Repo::new(&[("README.md", b"control\n")]);
    repo.config("core.precomposeUnicode", "true");
    let tip = repo.tip("main");
    refs(&repo, &["loose/Café", "loose/The\u{0301}me"], &[("packed/Café", &tip), ("packed/Théme", &tip)]);
    native_controls(&repo, &["loose/Café", "loose/Théme", "packed/Café", "packed/Théme"]).await;
    // Keep gitoxide's existing decomposed-query support too. Native Git checks
    // the NFC packed spelling; its CLI does not accept the NFD packed alias.
    let plain = RepositoryReader::new(repo.bare.clone());
    for reader in [plain.clone(), plain.pinned()] {
        for name in ["loose/Cafe\u{0301}", "loose/The\u{0301}me", "packed/Cafe\u{0301}", "packed/The\u{0301}me"] {
            assert_eq!(reader.commit_id(name.into()).await.unwrap(), tip);
        }
    }
}
