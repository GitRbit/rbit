//! Actual native immutable Git writes/reads in a catalog-bound temporary root.
use rbit_core::{AccountId, RepoId, Timestamp};
use rbit_git::wikis::{PrepareWikiPage, WikiObjectError, WikiObjects};
use rbit_kernel::{AdmissionError, BlockingBudget};
use rbit_wikis::{WikiContent, WikiGeneration, WikiPage, WikiPageId, WikiStoreId, policy::WikiHead, reading::WikiRevisionStore};
use std::num::NonZeroUsize;

fn budget() -> BlockingBudget {
    BlockingBudget::new(NonZeroUsize::new(1).unwrap())
}
fn store(byte: u8) -> WikiStoreId {
    WikiStoreId::try_from([byte; 16]).unwrap()
}
fn request(text: &str, parent: Option<rbit_wikis::WikiRevisionId>) -> PrepareWikiPage {
    PrepareWikiPage {
        wiki: rbit_wikis::WikiId::new(1).unwrap(),
        repository: RepoId(7),
        page: WikiPageId::new(11).unwrap(),
        parent,
        content: WikiContent::try_from(text.to_owned()).unwrap(),
        actor: AccountId(1),
        at: Timestamp(123),
    }
}
fn page(revision: rbit_wikis::WikiRevisionId) -> WikiPage {
    WikiPage {
        wiki: rbit_wikis::WikiId::new(1).unwrap(),
        repository: RepoId(7),
        id: WikiPageId::new(11).unwrap(),
        path: "Guide".parse().unwrap(),
        title: "Guide".parse().unwrap(),
        head: WikiHead { generation: WikiGeneration::INITIAL, revision },
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}

#[tokio::test]
async fn preparation_is_unpublished_and_exact_old_revision_remains_readable() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    let first = objects.prepare(request("first <script>text</script>", None)).await.unwrap();
    let second = objects.prepare(request("second", Some(first))).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(objects.read_page(store(1), &page(first)).await.unwrap().as_str(), "first <script>text</script>");
    assert_eq!(objects.read_page(store(1), &page(second)).await.unwrap().as_str(), "second");
    let raw = gix::open_opts(directory.path().join("7/1.git"), gix::open::Options::isolated()).unwrap();
    assert!(raw.head_id().is_err(), "immutable preparation must never select/publish HEAD");
}

#[tokio::test]
async fn admission_and_store_mismatch_precede_object_filesystem_effects() {
    let directory = tempfile::tempdir().unwrap();
    let work = budget();
    let objects = WikiObjects::open(directory.path(), store(1), work.clone()).unwrap();
    let held = work.try_reserve().unwrap();
    assert!(matches!(objects.prepare(request("body", None)).await, Err(WikiObjectError::Admission(AdmissionError::Busy))));
    assert!(!directory.path().join("7/1.git").exists());
    let revision = "a".repeat(40).parse().unwrap();
    assert!(matches!(objects.read_page(store(2), &page(revision)).await, Err(WikiObjectError::Binding)));
    drop(held);
    work.close();
    assert!(matches!(objects.prepare(request("body", None)).await, Err(WikiObjectError::Admission(AdmissionError::Closed))));
    assert!(!directory.path().join("7/1.git").exists());
}

#[test]
fn root_binding_is_persistent_and_foreign_or_oversized_marker_is_not_overwritten() {
    let directory = tempfile::tempdir().unwrap();
    WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    assert!(matches!(WikiObjects::open(directory.path(), store(2), budget()), Err(WikiObjectError::Binding)));
    assert_eq!(std::fs::read(directory.path().join("wiki-store-v1")).unwrap(), [1; 16]);
    std::fs::write(directory.path().join("wiki-store-v1"), [1; 17]).unwrap();
    assert!(matches!(WikiObjects::open(directory.path(), store(1), budget()), Err(WikiObjectError::Binding)));
    assert_eq!(std::fs::metadata(directory.path().join("wiki-store-v1")).unwrap().len(), 17);
}

#[test]
fn existing_unmarked_content_cannot_be_claimed_as_a_dedicated_wiki_root() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("foreign-content"), "preserve").unwrap();
    assert!(matches!(WikiObjects::open(directory.path(), store(1), budget()), Err(WikiObjectError::Binding)));
    assert!(!directory.path().join("wiki-store-v1").exists());
    assert_eq!(std::fs::read_to_string(directory.path().join("foreign-content")).unwrap(), "preserve");
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

fn raw_tree(repo: &gix::Repository, entries: &[(&str, &[u8])]) -> rbit_wikis::WikiRevisionId {
    let mut bytes = Vec::new();
    for (name, content) in entries {
        let blob = repo.write_blob(content).unwrap().detach();
        bytes.extend_from_slice(b"100644 ");
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(blob.as_slice());
    }
    let tree = repo.write_object(RawTree(bytes)).unwrap().detach();
    let signature = gix::actor::Signature {
        name: "Fixture".into(),
        email: "fixture@example.com".into(),
        time: gix::date::Time { seconds: 1, offset: 0 },
    };
    let commit = gix::objs::Commit {
        tree,
        parents: std::iter::empty().collect(),
        author: signature.clone(),
        committer: signature,
        encoding: None,
        message: "Synthetic malformed tree".into(),
        extra_headers: Vec::new(),
    };
    rbit_wikis::WikiRevisionId::try_from(repo.write_object(commit).unwrap().detach().as_slice()).unwrap()
}

#[tokio::test]
async fn duplicate_page_names_and_unsorted_trees_are_rejected_on_read_and_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    objects.prepare(request("initial", None)).await.unwrap();
    let raw = gix::open_opts(directory.path().join("7/1.git"), gix::open::Options::isolated()).unwrap();
    let cases: [[(&str, &[u8]); 2]; 2] = [[("11.md", b"one"), ("11.md", b"two")], [("2.md", b"one"), ("11.md", b"two")]];
    for entries in cases {
        let malformed = raw_tree(&raw, &entries);
        assert!(matches!(objects.read_page(store(1), &page(malformed)).await, Err(WikiObjectError::Format)));
        assert!(matches!(objects.prepare(request("replacement", Some(malformed))).await, Err(WikiObjectError::Format)));
    }
}

#[tokio::test]
async fn oversized_binary_and_nul_blob_objects_fail_before_returning_content() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    objects.prepare(request("initial", None)).await.unwrap();
    let raw = gix::open_opts(directory.path().join("7/1.git"), gix::open::Options::isolated()).unwrap();
    let large = vec![b'x'; rbit_wikis::MAX_WIKI_CONTENT_BYTES + 1];
    for content in [large.as_slice(), b"binary\xff".as_slice(), b"nul\0text".as_slice()] {
        let revision = raw_tree(&raw, &[("11.md", content)]);
        assert!(matches!(objects.read_page(store(1), &page(revision)).await, Err(WikiObjectError::Format)));
    }
}

#[cfg(unix)]
#[test]
fn linked_object_root_is_rejected_without_creating_a_marker_outside_root() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = directory.path().join("linked");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    assert!(matches!(WikiObjects::open(&link, store(1), budget()), Err(WikiObjectError::Binding)));
    assert!(!outside.path().join("wiki-store-v1").exists());
}

#[cfg(windows)]
#[test]
fn junction_object_root_is_rejected_without_creating_a_marker_outside_root() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = directory.path().join("linked");
    crate::hostile_support::link_directory(outside.path(), &link);
    assert!(matches!(WikiObjects::open(&link, store(1), budget()), Err(WikiObjectError::Binding)));
    assert!(!outside.path().join("wiki-store-v1").exists());
}

/// A link (a junction on Windows) planted at the fan-out directory a new loose object
/// would be written to: the write is refused and nothing lands outside the root.
#[tokio::test]
async fn linked_object_fan_out_is_refused_without_writing_outside_root() {
    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    let first = objects.prepare(request("first", None)).await.unwrap();
    let fan_out = directory.path().join("7/1.git/objects");
    let (text, prefix) = (0..10_000)
        .map(|n| format!("hostile fan-out {n}"))
        .map(|text| {
            let id = gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, text.as_bytes()).unwrap();
            (text, id.to_hex().to_string()[..2].to_owned())
        })
        .find(|(_, prefix)| !fan_out.join(prefix).exists())
        .unwrap();
    crate::hostile_support::link_directory(outside.path(), &fan_out.join(&prefix));
    assert!(matches!(objects.prepare(request(&text, Some(first))).await, Err(WikiObjectError::Binding)));
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0, "no object was written through the link");
    assert_eq!(objects.read_page(store(1), &page(first)).await.unwrap().as_str(), "first");
}

#[tokio::test]
async fn incarnation_marker_refuses_copied_foreign_scope_and_unmarked_legacy_layout() {
    let directory = tempfile::tempdir().unwrap();
    let objects = WikiObjects::open(directory.path(), store(1), budget()).unwrap();
    let revision = objects.prepare(request("old private page", None)).await.unwrap();
    let old = directory.path().join("7/1.git");
    let new = directory.path().join("7/2.git");
    std::fs::rename(&old, &new).unwrap();
    let mut selected = page(revision);
    selected.wiki = rbit_wikis::WikiId::new(2).unwrap();
    assert!(matches!(objects.read_page(store(1), &selected).await, Err(WikiObjectError::Binding)));
    let mut preparation = request("new", None);
    preparation.wiki = selected.wiki;
    assert!(matches!(objects.prepare(preparation).await, Err(WikiObjectError::Binding)));
    std::fs::rename(&new, directory.path().join("7.git")).unwrap();
    // A repository-ID-only namespace remains untouched; it is never adopted.
    assert!(objects.read_page(store(1), &page(revision)).await.is_err());
    let fresh = objects.prepare(request("new scope", None)).await.unwrap();
    assert_eq!(objects.read_page(store(1), &page(fresh)).await.unwrap().as_str(), "new scope");
    assert!(directory.path().join("7.git/wiki-incarnation-v1").exists());
}
