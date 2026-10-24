//! Deterministic cache, concurrency and repository-lifetime regressions.

use super::*;
use std::{
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};
use tokio::sync::oneshot;

fn key(path: &Path) -> Key {
    let (repository, _) = repository_identity(path).unwrap();
    Key {
        repository: Arc::new(repository),
        repo: RepoId(1),
        owner: AccountId(1),
        location: path.to_owned(),
        inputs: None,
        request: 0,
        commit: "a".repeat(40),
        path: "file.txt".into(),
        max_file_bytes: 1024,
    }
}

async fn calculate(service: &BlameService, key: Key, calls: &Arc<AtomicUsize>) -> Arc<Blame> {
    let calls = Arc::clone(calls);
    service
        .get_or_compute(key, Duration::from_secs(2), move |permit| async move {
            let _permit = permit;
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Computed { value: Blame::Binary, cacheable: true })
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn cached_results_reuse_data_and_separate_every_input() {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let initial = key(directory.path());
    let service = BlameService::bounded(16, MAX_BYTES, 2);
    let calls = Arc::new(AtomicUsize::new(0));
    let first = calculate(&service, initial.clone(), &calls).await;
    let cached = calculate(&service, key(directory.path()), &calls).await;
    assert!(Arc::ptr_eq(&first, &cached));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let mut changed = vec![initial.clone(); 7];
    changed[0].repo = RepoId(2);
    changed[1].owner = AccountId(2);
    changed[2].commit = "b".repeat(40);
    changed[3].path = "another.txt".into();
    changed[4].max_file_bytes += 1;
    changed[5].location = other.path().to_owned();
    changed[6].repository = key(other.path()).repository;
    for key in changed {
        calculate(&service, key, &calls).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn cache_enforces_entry_and_retained_byte_limits() {
    let directory = tempfile::tempdir().unwrap();
    let first = key(directory.path());
    let mut second = first.clone();
    second.path = "another.txt".into();
    let calls = Arc::new(AtomicUsize::new(0));
    let service = BlameService::bounded(1, MAX_BYTES, 1);
    calculate(&service, first.clone(), &calls).await;
    calculate(&service, second.clone(), &calls).await;
    assert_eq!(service.inner.state().ready.len(), 1);
    calculate(&service, first.clone(), &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 3, "the first entry was evicted");

    let weight = retained_bytes(&first, &Blame::Binary);
    let service = BlameService::bounded(10, weight, 1);
    calculate(&service, first.clone(), &calls).await;
    calculate(&service, second, &calls).await;
    assert!(service.inner.state().bytes <= weight);
    assert!(service.inner.state().ready.len() <= 1);

    let service = BlameService::bounded(10, weight - 1, 1);
    calculate(&service, first.clone(), &calls).await;
    calculate(&service, first, &calls).await;
    assert!(service.inner.state().ready.is_empty(), "oversized results are returned but never cached");
}

#[tokio::test]
async fn hunk_metadata_counts_against_the_retained_byte_budget() {
    let directory = tempfile::tempdir().unwrap();
    let key = key(directory.path());
    let commit = CommitSummary {
        id: "a".repeat(40),
        short_id: "a".repeat(7),
        summary: "summary".into(),
        body: Some("body".repeat(2048)),
        author_name: "author".into(),
        author_email: "author@example.com".into(),
        authored_at: rbit_core::Timestamp(1),
        parent_ids: Vec::new(),
    };
    let blame = Blame::Blamed(Box::new(FileBlame {
        commit: commit.clone(),
        text: "text\n".into(),
        blob_id: "b".repeat(40),
        hunks: vec![BlameHunk { commit, start_line: 1, line_count: 1, source_path: Some("old/file.txt".into()) }],
    }));
    assert!(retained_bytes(&key, &blame) > 16 * 1024);
    let service = BlameService::bounded(8, 16 * 1024, 1);
    let result = service
        .get_or_compute(key, Duration::from_secs(2), |permit| async move {
            let _permit = permit;
            Ok(Computed { value: blame, cacheable: true })
        })
        .await
        .unwrap();
    assert!(matches!(result.as_ref(), Blame::Blamed(_)));
    assert!(service.inner.state().ready.is_empty());
}

#[tokio::test]
async fn cancellation_and_deadlines_do_not_release_blocking_work_or_duplicate_it() {
    let directory = tempfile::tempdir().unwrap();
    let first = key(directory.path());
    let service = BlameService::bounded(8, MAX_BYTES, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, has_started) = oneshot::channel();
    let (finish, wait_finish) = std::sync::mpsc::channel();
    let request_service = service.clone();
    let request_key = first.clone();
    let request_calls = Arc::clone(&calls);
    let request = tokio::spawn(async move {
        request_service
            .get_or_compute(request_key, Duration::from_secs(60), move |permit| async move {
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    request_calls.fetch_add(1, Ordering::SeqCst);
                    started.send(()).unwrap();
                    wait_finish.recv().unwrap();
                    Ok(Computed { value: Blame::Binary, cacheable: true })
                })
                .await?
            })
            .await
    });
    has_started.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert_eq!(service.inner.permits.available_permits(), 0);

    let result = service.get_or_compute(first.clone(), Duration::ZERO, |_| async { panic!("must share the running job") }).await.unwrap();
    assert!(matches!(result.as_ref(), Blame::TooLarge));
    assert!(service.inner.state().ready.is_empty(), "the response timeout is not cached");
    let mut other = first.clone();
    other.path = "another.txt".into();
    let denied = service.get_or_compute(other, Duration::from_secs(1), |_| async { panic!("job limit exceeded") }).await;
    assert!(matches!(denied, Err(ServiceError::RateLimited { .. })));

    finish.send(()).unwrap();
    let result = calculate(&service, first.clone(), &calls).await;
    assert!(matches!(result.as_ref(), Blame::Binary));
    let repeated = calculate(&service, first, &calls).await;
    assert!(Arc::ptr_eq(&result, &repeated));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(service.inner.permits.available_permits(), 1);
}

#[tokio::test]
async fn errors_are_not_cached() {
    let directory = tempfile::tempdir().unwrap();
    let key = key(directory.path());
    let service = BlameService::bounded(8, MAX_BYTES, 1);
    let error = service
        .get_or_compute(key.clone(), Duration::from_secs(2), |permit| async move {
            let _permit = permit;
            Err(GitError::NotFound("file"))
        })
        .await;
    assert!(matches!(error, Err(ServiceError::NotFound)));
    assert!(service.inner.state().ready.is_empty());
    let result = calculate(&service, key, &Arc::new(AtomicUsize::new(0))).await;
    assert!(matches!(result.as_ref(), Blame::Binary));
}

#[test]
fn identity_survives_moves_without_blocking_deletion_or_path_reuse() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("repo.git");
    let moved = directory.path().join("moved.git");
    std::fs::create_dir(&path).unwrap();
    let original = key(&path);
    std::fs::rename(&path, &moved).unwrap();
    assert_eq!(original.repository, key(&moved).repository);
    std::fs::remove_dir_all(&moved).unwrap();
    std::fs::create_dir(&path).unwrap();
    let replacement = key(&path);
    assert_ne!(original, replacement, "same numeric id and path must not reuse an old repository's data");
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn local_inputs_include_shallow_attributes_and_index_and_are_bounded() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("info")).unwrap();
    let complete = local_inputs(directory.path()).unwrap();
    for name in ["shallow", "info/attributes", "index"] {
        let path = directory.path().join(name);
        std::fs::write(&path, "a\n").unwrap();
        let changed = local_inputs(directory.path()).unwrap();
        assert_ne!(complete, changed);
        std::fs::write(&path, "b\n").unwrap();
        assert_ne!(changed, local_inputs(directory.path()).unwrap());
        std::fs::remove_file(&path).unwrap();
        assert_eq!(complete, local_inputs(directory.path()).unwrap());
        std::fs::File::create(&path).unwrap().set_len(MAX_INPUT_BYTES + 1).unwrap();
        assert!(local_inputs(directory.path()).is_none());
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(local_inputs(directory.path()).is_none(), "unreadable/non-regular inputs bypass reuse");
        std::fs::remove_dir(&path).unwrap();
    }
}

fn git(directory: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "init.defaultBranch=main", "-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn fixture() -> (tempfile::TempDir, RepoContext) {
    use rbit_core::{
        Account, AccountKind, RepoAccess, Timestamp, Visibility,
        repository::{OwnedRepository, Repository},
    };
    use rbit_git::RepositoryReader;

    let directory = tempfile::tempdir().unwrap();
    git(directory.path(), &["init", "--quiet"]);
    std::fs::write(directory.path().join("file.txt"), "first\n").unwrap();
    git(directory.path(), &["add", "file.txt"]);
    git(directory.path(), &["commit", "--quiet", "-m", "first"]);
    let path = directory.path().join(".git");
    let repo = RepoContext {
        repo: OwnedRepository {
            owner: Account {
                id: AccountId(1),
                kind: AccountKind::User,
                name: "alice".parse().unwrap(),
                display_name: None,
                bio: None,
                created_at: Timestamp(1),
            },
            repo: Repository {
                id: RepoId(1),
                owner_id: AccountId(1),
                name: "demo".parse().unwrap(),
                description: None,
                visibility: Visibility::Private,
                default_branch: "main".into(),
                created_at: Timestamp(1),
                updated_at: Timestamp(1),
                pushed_at: None,
                archived_at: None,
            },
        },
        access: RepoAccess::Read,
        reader: RepositoryReader::new(path.clone()),
        disk_path: path,
    };
    (directory, repo)
}

#[tokio::test]
async fn real_blame_resolves_moving_refs_and_rechecks_access_before_cache_lookup() {
    let (directory, mut repo) = fixture();
    let service = BlameService::new();
    let original = service.blame(&repo, "main", "file.txt").await.unwrap();
    let by_commit = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(Arc::ptr_eq(&original.result, &by_commit.result));

    std::fs::write(directory.path().join("file.txt"), "changed\n").unwrap();
    git(directory.path(), &["commit", "--quiet", "-am", "second"]);
    let changed = service.blame(&repo, "main", "file.txt").await.unwrap();
    assert_ne!(original.commit_id, changed.commit_id);
    let Blame::Blamed(file) = changed.result.as_ref() else { panic!("expected text blame") };
    assert_eq!(file.text, "changed\n");
    repo.access = rbit_core::RepoAccess::None;
    assert!(matches!(service.blame(&repo, "main", "file.txt").await, Err(ServiceError::NotFound)));

    // Cached data and identity handles must not retain Git mappings or leases that
    // would prevent a transfer or deletion after the request has completed.
    let moved = directory.path().join("moved.git");
    std::fs::rename(&repo.disk_path, &moved).unwrap();
    std::fs::remove_dir_all(&moved).unwrap();
}

#[tokio::test]
async fn real_blame_refreshes_head_effective_configuration_and_shallow_inputs() {
    let (directory, repo) = fixture();
    let service = BlameService::new();
    let original = service.blame(&repo, "main", "file.txt").await.unwrap();
    // HEAD affects attribute lookup even when blaming the original immutable commit.
    std::fs::write(directory.path().join("file.txt"), "second\n").unwrap();
    git(directory.path(), &["commit", "--quiet", "-am", "second"]);
    let head_changed = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(!Arc::ptr_eq(&original.result, &head_changed.result));
    let repeated = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(Arc::ptr_eq(&head_changed.result, &repeated.result));

    git(directory.path(), &["config", "diff.algorithm", "minimal"]);
    let configured = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(!Arc::ptr_eq(&repeated.result, &configured.result));
    git(directory.path(), &["config", "diff.algorithm", "histogram"]);
    let reconfigured = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(!Arc::ptr_eq(&configured.result, &reconfigured.result));

    // The reader opens Git in isolated mode, which does not follow includes. Their
    // external contents affect neither the effective configuration nor the blame.
    let included = repo.disk_path.join("blame-config");
    std::fs::write(&included, "[diff]\nalgorithm = minimal\n").unwrap();
    git(directory.path(), &["config", "include.path", "blame-config"]);
    let with_include = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    std::fs::write(&included, "[diff]\nalgorithm = histogram\n").unwrap();
    let ignored_include = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(Arc::ptr_eq(&with_include.result, &ignored_include.result));

    std::fs::write(repo.disk_path.join("shallow"), format!("{}\n", original.commit_id)).unwrap();
    let shallow = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(!Arc::ptr_eq(&ignored_include.result, &shallow.result));
    let repeated = service.blame(&repo, &original.commit_id, "file.txt").await.unwrap();
    assert!(Arc::ptr_eq(&shallow.result, &repeated.result));
}

#[tokio::test]
async fn unsupported_or_oversized_configuration_bypasses_reuse() {
    let (directory, repo) = fixture();
    let service = BlameService::new();
    let attributes = directory.path().join("attributes");
    std::fs::write(&attributes, "").unwrap();
    for (name, value) in
        [("core.attributesFile", attributes.to_str().unwrap()), ("filter.example.clean", "cat"), ("diff.example.binary", "true")]
    {
        git(directory.path(), &["config", name, value]);
        let first = service.blame(&repo, "main", "file.txt").await.unwrap();
        let second = service.blame(&repo, "main", "file.txt").await.unwrap();
        assert!(!Arc::ptr_eq(&first.result, &second.result), "{name} must bypass reuse");
        assert!(service.inner.state().ready.is_empty());
        // Removing the section also removes empty driver/filter declarations.
        let section = name.rsplit_once('.').unwrap().0;
        if section == "core" {
            git(directory.path(), &["config", "--unset", name]);
        } else {
            git(directory.path(), &["config", "--remove-section", section]);
        }
    }
    let mut configuration = std::fs::OpenOptions::new().append(true).open(repo.disk_path.join("config")).unwrap();
    writeln!(configuration, "[blamecache]\npadding = {}", "x".repeat(64 * 1024)).unwrap();
    let first = service.blame(&repo, "main", "file.txt").await.unwrap();
    let second = service.blame(&repo, "main", "file.txt").await.unwrap();
    assert!(!Arc::ptr_eq(&first.result, &second.result));
    assert!(service.inner.state().ready.is_empty());
}

#[tokio::test]
async fn jobs_invalidated_during_calculation_return_data_without_caching() {
    let directory = tempfile::tempdir().unwrap();
    let key = key(directory.path());
    let service = BlameService::new();
    let before = local_inputs(directory.path()).unwrap();
    let path = directory.path().to_owned();
    let result = service
        .get_or_compute(key.clone(), Duration::from_secs(2), |permit| async move {
            let _permit = permit;
            // Deterministically change an input between its initial fingerprint and
            // completion, without relying on a long enough real Git calculation.
            std::fs::write(path.join("shallow"), "changed\n").unwrap();
            Ok(Computed { value: Blame::Binary, cacheable: local_inputs(&path).as_ref() == Some(&before) })
        })
        .await
        .unwrap();
    assert!(matches!(result.as_ref(), Blame::Binary));
    assert!(service.inner.state().ready.is_empty());
    let calls = Arc::new(AtomicUsize::new(0));
    calculate(&service, key, &calls).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
