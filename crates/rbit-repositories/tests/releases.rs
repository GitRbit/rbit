//! Release rules over in-memory ports: visibility, permissions, validation, limits and
//! publication, without SQL, Git, files or an async runtime.
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_repositories::releases::{
    AssetFiles, AssetLimits, AssetUpload, NewAsset, NewRelease, Release, ReleaseAccess, ReleaseAsset, ReleaseAssetId, ReleaseAuthor,
    ReleaseChanges, ReleaseDetail, ReleaseError, ReleaseId, ReleaseInput, ReleasePort, ReleaseRepository, ReleaseSource, ReleaseWriter,
    Releases, TagError, human_bytes,
};
use std::{
    collections::HashMap,
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

const REPO: RepoId = RepoId(7);
const ALICE: AccountId = AccountId(1);
fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected suspension"),
    }
}
fn release(id: i64, tag: &str, draft: bool) -> Release {
    Release {
        id: ReleaseId(id),
        repo_id: REPO,
        tag_name: tag.into(),
        name: tag.into(),
        notes: String::new(),
        is_draft: draft,
        is_prerelease: false,
        author: Some(ReleaseAuthor { id: ALICE, name: "alice".into() }),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        published_at: (!draft).then_some(Timestamp(1)),
        source: ReleaseSource::User,
    }
}
fn asset(id: i64, release: i64, name: &str, uploader: Option<AccountId>) -> ReleaseAsset {
    ReleaseAsset {
        id: ReleaseAssetId(id),
        release_id: ReleaseId(release),
        repo_id: REPO,
        name: name.into(),
        size: 1,
        content_type: String::new(),
        sha256: [0; 32],
        download_count: 0,
        uploader_id: uploader,
        created_at: Timestamp(1),
        source: ReleaseSource::User,
    }
}

/// Releases and assets with a log of every call; ids are assigned in order.
#[derive(Default)]
struct Store {
    releases: Mutex<Vec<Release>>,
    assets: Mutex<Vec<ReleaseAsset>>,
    calls: Mutex<Vec<String>>,
}
impl Store {
    fn log(&self, call: impl Into<String>) {
        self.calls.lock().unwrap().push(call.into());
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn get(&self, id: ReleaseId) -> Option<Release> {
        self.releases.lock().unwrap().iter().find(|r| r.id == id).cloned()
    }
}
impl ReleaseRepository for Store {
    type Error = &'static str;
    fn count(&self, _: RepoId, drafts: bool) -> impl Future<Output = Result<u64, &'static str>> + Send {
        self.log("count");
        ready(Ok(self.releases.lock().unwrap().iter().filter(|r| drafts || !r.is_draft).count() as u64))
    }
    fn list(&self, _: RepoId, drafts: bool, limit: u32, offset: u32) -> impl Future<Output = Result<Vec<Release>, &'static str>> + Send {
        self.log(format!("list {limit} {offset}"));
        let all = self.releases.lock().unwrap();
        ready(Ok(all.iter().filter(|r| drafts || !r.is_draft).skip(offset as usize).take(limit as usize).cloned().collect()))
    }
    fn assets_of(&self, ids: &[ReleaseId]) -> impl Future<Output = Result<HashMap<ReleaseId, Vec<ReleaseAsset>>, &'static str>> + Send {
        let mut map: HashMap<_, Vec<_>> = HashMap::new();
        for a in self.assets.lock().unwrap().iter().filter(|a| ids.contains(&a.release_id)) {
            map.entry(a.release_id).or_default().push(a.clone());
        }
        ready(Ok(map))
    }
    fn find(&self, _: RepoId, id: ReleaseId) -> impl Future<Output = Result<Option<Release>, &'static str>> + Send {
        self.log("find");
        ready(Ok(self.get(id)))
    }
    fn find_by_tag(&self, _: RepoId, tag: &str) -> impl Future<Output = Result<Option<Release>, &'static str>> + Send {
        self.log("find_by_tag");
        ready(Ok(self.releases.lock().unwrap().iter().find(|r| r.tag_name == tag).cloned()))
    }
    fn latest(&self, _: RepoId) -> impl Future<Output = Result<Option<Release>, &'static str>> + Send {
        ready(Ok(None))
    }
    fn assets(&self, release: ReleaseId) -> impl Future<Output = Result<Vec<ReleaseAsset>, &'static str>> + Send {
        self.log("assets");
        ready(Ok(self.assets.lock().unwrap().iter().filter(|a| a.release_id == release).cloned().collect()))
    }
    fn create(&self, new: NewRelease<'_>) -> impl Future<Output = Result<ReleaseId, &'static str>> + Send {
        self.log(format!("create {} {:?} {:?}", new.name, new.author_id, new.source));
        let mut all = self.releases.lock().unwrap();
        let id = ReleaseId(i64::try_from(all.len()).unwrap() + 1);
        let mut row = release(id.0, new.tag_name, new.is_draft);
        row.author = None;
        all.push(row);
        ready(Ok(id))
    }
    fn update(&self, id: ReleaseId, changes: ReleaseChanges<'_>) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.log(format!("update {}", changes.is_draft));
        let mut all = self.releases.lock().unwrap();
        let row = all.iter_mut().find(|r| r.id == id).unwrap();
        row.is_draft = changes.is_draft;
        row.published_at = row.published_at.or(changes.published_at);
        ready(Ok(()))
    }
    fn delete(&self, id: ReleaseId) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.log("delete");
        self.releases.lock().unwrap().retain(|r| r.id != id);
        ready(Ok(()))
    }
    fn count_assets(&self, release: ReleaseId) -> impl Future<Output = Result<u32, &'static str>> + Send {
        self.log("count_assets");
        ready(Ok(self.assets.lock().unwrap().iter().filter(|a| a.release_id == release).count().try_into().unwrap()))
    }
    fn find_asset(&self, release: ReleaseId, name: &str) -> impl Future<Output = Result<Option<ReleaseAsset>, &'static str>> + Send {
        self.log("find_asset");
        ready(Ok(self.assets.lock().unwrap().iter().find(|a| a.release_id == release && a.name == name).cloned()))
    }
    fn find_asset_by_id(&self, _: RepoId, id: ReleaseAssetId) -> impl Future<Output = Result<Option<ReleaseAsset>, &'static str>> + Send {
        ready(Ok(self.assets.lock().unwrap().iter().find(|a| a.id == id).cloned()))
    }
    fn insert_asset_limited(&self, _: NewAsset<'_>, _: u32) -> impl Future<Output = Result<Option<ReleaseAsset>, &'static str>> + Send {
        ready(Err("the file port inserts"))
    }
    fn delete_asset(&self, id: ReleaseAssetId) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.log("delete_asset");
        self.assets.lock().unwrap().retain(|a| a.id != id);
        ready(Ok(()))
    }
    fn count_download(&self, _: ReleaseAssetId) -> impl Future<Output = Result<(), &'static str>> + Send {
        ready(Ok(()))
    }
    fn asset_ids_for_repo(&self, _: RepoId) -> impl Future<Output = Result<Vec<ReleaseAssetId>, &'static str>> + Send {
        ready(Ok(Vec::new()))
    }
}

/// Tags that exist, the outcome of creating one, and what was announced.
#[derive(Default)]
struct Port {
    store: Store,
    tags: Vec<&'static str>,
    tag_error: Option<TagError<&'static str>>,
    events: Mutex<Vec<String>>,
}
impl ReleasePort for Port {
    type Store = Store;
    type Error = &'static str;
    fn store(&self) -> &Store {
        &self.store
    }
    fn tag_exists(&self, tag: &str) -> impl Future<Output = Result<bool, &'static str>> + Send {
        self.store.log("tag_exists");
        ready(Ok(self.tags.contains(&tag)))
    }
    fn create_tag(&self, tag: &str, target: &str) -> impl Future<Output = Result<(), TagError<&'static str>>> + Send {
        self.store.log(format!("create_tag {tag} {target}"));
        let outcome = match &self.tag_error {
            None => Ok(()),
            Some(TagError::InvalidName) => Err(TagError::InvalidName),
            Some(TagError::Exists) => Err(TagError::Exists),
            Some(TagError::MissingTarget) => Err(TagError::MissingTarget),
            Some(TagError::Failure(error)) => Err(TagError::Failure(*error)),
        };
        ready(outcome)
    }
    fn published(&self, release: ReleaseId, actor: Option<AccountId>) {
        self.events.lock().unwrap().push(format!("published {} {actor:?}", release.0));
    }
    fn created(&self, tag: &str, draft: bool) {
        self.events.lock().unwrap().push(format!("created {tag} {draft}"));
    }
}

/// Upload bytes of a given size; the release is full when `full`.
struct Files {
    size: u64,
    full: bool,
    placed: Option<AssetUpload>,
    received: bool,
}
impl AssetFiles for Files {
    type Received = u64;
    type Error = &'static str;
    fn receive(&mut self, max: u64) -> impl Future<Output = Result<Option<u64>, &'static str>> + Send {
        self.received = true;
        ready(Ok((self.size <= max).then_some(self.size)))
    }
    fn place(&mut self, upload: AssetUpload, size: u64, _: u32) -> impl Future<Output = Result<Option<ReleaseAsset>, &'static str>> + Send {
        let placed = asset(99, upload.release_id.0, &upload.name, upload.uploader_id);
        self.placed = Some(upload);
        ready(Ok((!self.full).then_some(ReleaseAsset { size, ..placed })))
    }
}
fn files(size: u64) -> Files {
    Files { size, full: false, placed: None, received: false }
}

const READER: ReleaseAccess = ReleaseAccess { repo: REPO, can_write: false, archived: false };
const WRITER: ReleaseAccess = ReleaseAccess { repo: REPO, can_write: true, archived: false };
const LIMITS: AssetLimits = AssetLimits { max_assets: 2, max_bytes: 10 * 1024 };
fn input(tag: &str, target: &str, draft: bool) -> ReleaseInput {
    ReleaseInput { tag_name: tag.into(), target: target.into(), is_draft: draft, ..ReleaseInput::default() }
}
fn seeded() -> Port {
    let port = Port { tags: vec!["v1", "v2"], ..Port::default() };
    *port.store.releases.lock().unwrap() = vec![release(1, "v1", false), release(2, "v2", true)];
    *port.store.assets.lock().unwrap() = vec![asset(5, 1, "app.zip", Some(AccountId(3))), asset(6, 2, "draft.zip", None)];
    port
}

#[test]
fn drafts_exist_for_writers_only() {
    let port = seeded();
    let page = run(Releases::list(&port.store, READER, 9)).unwrap();
    assert_eq!((page.total, page.page, page.releases.len()), (1, 1, 1), "the page is clamped");
    assert!(matches!(run(Releases::by_tag(&port.store, READER, "v2")), Err(ReleaseError::NotFound)));
    assert!(matches!(run(Releases::asset_named(&port.store, READER, "v2", "draft.zip")), Err(ReleaseError::NotFound)));
    assert!(matches!(run(Releases::asset(&port.store, READER, ReleaseAssetId(6))), Err(ReleaseError::NotFound)));
    assert_eq!(run(Releases::list(&port.store, WRITER, 1)).unwrap().total, 2);
    assert_eq!(run(Releases::asset_named(&port.store, WRITER, "v2", "draft.zip")).unwrap().id, ReleaseAssetId(6));
    let (detail, _) = run(Releases::page(&port.store, WRITER, 5, 1)).unwrap();
    assert_eq!(detail[0].assets[0].name, "draft.zip");
}

#[test]
fn people_are_authors_and_uploaders_once_in_order() {
    let detail = |r, a| ReleaseDetail { release: r, assets: a };
    let people = Releases::people(&[
        detail(release(1, "v1", false), vec![asset(1, 1, "a", Some(AccountId(3))), asset(2, 1, "b", None)]),
        detail(release(2, "v2", false), vec![asset(3, 2, "c", Some(ALICE))]),
    ]);
    assert_eq!(people, [ALICE, AccountId(3)]);
}

#[test]
fn writers_tasks_and_archived_repositories() {
    let port = seeded();
    let foreign = ReleaseWriter::Task { repo: RepoId(8), contents_write: true };
    let read_only = ReleaseWriter::Task { repo: REPO, contents_write: false };
    for writer in [foreign, read_only] {
        assert!(matches!(run(Releases::create(&port, WRITER, writer, input("v1", "", false))), Err(ReleaseError::Forbidden)));
    }
    let person = ReleaseWriter::Person(ALICE);
    assert!(matches!(run(Releases::update(&port, READER, person, ReleaseId(1), input("v1", "", false))), Err(ReleaseError::Forbidden)));
    let archived = ReleaseAccess { archived: true, ..WRITER };
    assert!(matches!(run(Releases::create(&port, archived, person, input("v1", "", false))), Err(ReleaseError::Archived)));
    assert!(matches!(Releases::require_writer::<()>(ReleaseAccess { archived: true, ..READER }), Err(ReleaseError::Forbidden)));
    assert!(port.store.calls().is_empty(), "refusals touch nothing");
}

#[test]
fn creating_validates_then_creates_the_tag_and_announces_once() {
    let person = ReleaseWriter::Person(ALICE);
    let port = seeded();
    let refused = |input| match run(Releases::create(&port, WRITER, person, input)) {
        Err(ReleaseError::Invalid(message)) => message,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(input(" ", "", false)), "choose the tag this release is for");
    assert_eq!(refused(ReleaseInput { name: "a\nb".into(), ..input("v1", "", false) }), "the title cannot contain control characters");
    assert_eq!(refused(input("v3", " ", false)), "there is no tag called v3; push it or choose a branch to create it from");

    let made = run(Releases::create(&port, WRITER, person, input("v3", "main", false))).unwrap();
    assert_eq!(made.tag_name, "v3");
    assert!(port.store.calls().ends_with(&[
        "tag_exists".into(),
        "find_by_tag".into(),
        "create_tag v3 main".into(),
        "create v3 Some(AccountId(1)) User".into(),
        "find".into()
    ]));
    assert_eq!(*port.events.lock().unwrap(), ["published 3 Some(AccountId(1))", "created v3 false"]);

    let task = ReleaseWriter::Task { repo: REPO, contents_write: true };
    let draft = run(Releases::create(&port, WRITER, task, input("v1", "", true)));
    assert!(draft.is_ok(), "tags that exist need no target");
    assert!(port.store.calls().contains(&"create v1 None Actions".into()));
    assert_eq!(port.events.lock().unwrap().last().unwrap(), "created v1 true", "drafts are not published");
}

#[test]
fn a_missing_tag_already_released_or_refused_by_git() {
    let person = ReleaseWriter::Person(ALICE);
    let port = Port { tags: vec![], ..seeded() };
    assert!(matches!(
        run(Releases::create(&port, WRITER, person, input("v1", "main", false))),
        Err(ReleaseError::Conflict("a release for this tag"))
    ));
    assert!(!port.store.calls().iter().any(|c| c.starts_with("create_tag")));
    for (error, expected) in
        [(TagError::InvalidName, "v9 is not a valid tag name"), (TagError::MissingTarget, "there is no branch or commit called main")]
    {
        let port = Port { tags: vec![], tag_error: Some(error), ..Port::default() };
        match run(Releases::create(&port, WRITER, person, input("v9", "main", false))) {
            Err(ReleaseError::Invalid(message)) => assert_eq!(message, expected),
            other => panic!("{other:?}"),
        }
    }
    let port = Port { tag_error: Some(TagError::Exists), ..Port::default() };
    assert!(matches!(run(Releases::create(&port, WRITER, person, input("v9", "main", false))), Err(ReleaseError::Conflict("the tag"))));
    let port = Port { tag_error: Some(TagError::Failure("git")), ..Port::default() };
    assert!(matches!(run(Releases::create(&port, WRITER, person, input("v9", "main", false))), Err(ReleaseError::Failure("git"))));
}

#[test]
fn editing_needs_the_tag_and_publishes_a_draft_once() {
    let person = ReleaseWriter::Person(ALICE);
    let port = seeded();
    assert!(matches!(run(Releases::update(&port, WRITER, person, ReleaseId(9), input("v1", "", false))), Err(ReleaseError::NotFound)));
    match run(Releases::update(&port, WRITER, person, ReleaseId(2), input("v7", "main", false))) {
        Err(ReleaseError::Invalid(message)) => assert_eq!(message, "there is no tag called v7; push it first"),
        other => panic!("{other:?}"),
    }
    let published = run(Releases::update(&port, WRITER, person, ReleaseId(2), input("v2", "", false))).unwrap();
    assert!(published.published_at.is_some());
    run(Releases::update(&port, WRITER, person, ReleaseId(2), input("v2", "", true))).unwrap();
    run(Releases::update(&port, WRITER, person, ReleaseId(2), input("v2", "", false))).unwrap();
    assert_eq!(*port.events.lock().unwrap(), ["published 2 Some(AccountId(1))"]);
}

#[test]
fn deleting_rows_returns_the_files_to_remove() {
    let port = seeded();
    assert!(matches!(run(Releases::delete_asset(&port.store, REPO, ReleaseId(1), ReleaseAssetId(6))), Err(ReleaseError::NotFound)));
    assert_eq!(run(Releases::delete_asset(&port.store, REPO, ReleaseId(1), ReleaseAssetId(5))).unwrap().name, "app.zip");
    let (gone, assets) = run(Releases::delete(&port.store, REPO, ReleaseId(2))).unwrap();
    assert_eq!((gone.tag_name.as_str(), assets.len()), ("v2", 1));
    assert!(port.store.get(ReleaseId(2)).is_none());
    assert!(matches!(run(Releases::delete(&port.store, REPO, ReleaseId(2))), Err(ReleaseError::NotFound)));
}

#[test]
fn uploads_check_names_limits_and_size_before_placing() {
    let person = ReleaseWriter::Person(ALICE);
    let port = seeded();
    let upload = |files: &mut Files, release: i64, name: &str| {
        run(Releases::upload(&port, files, WRITER, person, ReleaseId(release), name, " text/plain\u{7}\n ", LIMITS))
    };
    let invalid = |result: Result<ReleaseAsset, ReleaseError<&'static str>>| match result {
        Err(ReleaseError::Invalid(message)) => message,
        other => panic!("{other:?}"),
    };
    let mut refused = files(1);
    assert!(matches!(upload(&mut refused, 9, "x"), Err(ReleaseError::NotFound)));
    assert_eq!(invalid(upload(&mut refused, 1, ".hidden")), "file names cannot start with a dot");
    assert!(matches!(upload(&mut refused, 1, "app.zip"), Err(ReleaseError::Conflict("a file with this name"))));
    port.store.assets.lock().unwrap().push(asset(7, 1, "b.zip", None));
    assert_eq!(invalid(upload(&mut refused, 1, "c.zip")), "a release can have at most 2 files");
    assert!(!refused.received, "refusals read no bytes");

    assert_eq!(invalid(upload(&mut files(10 * 1024 + 1), 2, "big.zip")), "files can be at most 10 KiB");
    assert_eq!(invalid(upload(&mut Files { full: true, ..files(1) }, 2, "late.zip")), "a release can have at most 2 files");
    let mut ok = files(3);
    assert_eq!(upload(&mut ok, 2, "ok.zip").unwrap().size, 3);
    let placed = ok.placed.unwrap();
    assert_eq!((placed.content_type.as_str(), placed.uploader_id, placed.source), ("text/plain", Some(ALICE), ReleaseSource::User));
    let mut untyped = files(1);
    run(Releases::upload(&port, &mut untyped, WRITER, person, ReleaseId(2), "raw.bin", "\u{7}", LIMITS)).unwrap();
    assert_eq!(untyped.placed.unwrap().content_type, "application/octet-stream");
    let task = ReleaseWriter::Task { repo: RepoId(8), contents_write: true };
    let foreign = run(Releases::upload(&port, &mut files(1), WRITER, task, ReleaseId(2), "t.bin", "", LIMITS));
    assert!(matches!(foreign, Err(ReleaseError::Forbidden)));
}

#[test]
fn limits_read_naturally() {
    assert_eq!(human_bytes(512 * 1024 * 1024), "512 MiB");
    assert_eq!(human_bytes(10 * 1024), "10 KiB");
    assert_eq!(human_bytes(1000), "1000 bytes");
}
