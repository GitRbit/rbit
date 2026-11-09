//! A backup taken while pushes, releases, LFS uploads, artifact uploads, wiki changes and
//! deletions run
//! restores into an instance where every row finds what it refers to.

use std::path::Path;

use rbit_core::{Visibility, lfs::LfsOid};
use rbit_services::{CreateRepository, releases::ReleaseInput, wikis::pages::WikiLookup};
use sqlx::Connection;

use super::{
    consistency,
    instance::{self, connect, push_commit, rbit, sha256, succeeded},
    round_trip::manifest,
};

/// Repositories besides `project`, so the backup takes long enough to overlap the writes.
const EXTRA_REPOSITORIES: usize = 8;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_backup_taken_during_writes_restores_consistently() {
    let dir = tempfile::tempdir().unwrap();
    let (data, backup, restored) = (dir.path().join("data"), dir.path().join("backup"), dir.path().join("restored"));
    let seeded = Box::pin(instance::populate(&data)).await;
    let open = instance::open(&data).await;
    let ada = open.ada().await;
    for index in 0..EXTRA_REPOSITORIES {
        let input = CreateRepository { name: format!("extra-{index}"), description: String::new(), visibility: Visibility::Public };
        open.services.repositories.create(&ada, input).await.unwrap();
        let path = open.services.repositories.open(Some(&ada), "ada", &format!("extra-{index}")).await.unwrap().disk_path;
        push_commit(seeded.work.path(), &path, "extra.txt", &format!("extra {index}\n"), None);
    }
    let repo = open.services.repositories.open(Some(&ada), "ada", "project").await.unwrap();
    let repo_id = repo.repo.repo.id.0;

    let mut backup_process = tokio::process::Command::new(env!("CARGO_BIN_EXE_rbit"))
        .args(["backup", "--output", backup.to_str().unwrap()])
        .env("RBIT_STORAGE__DATA_DIR", &data)
        .env("RBIT_ACTIONS__ENABLED", "true")
        .env_remove("RBIT_CONFIG")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut rounds = 0_u32;
    let mut releases = Vec::new();
    loop {
        rounds += 1;
        let tag = format!("t{rounds}");
        let (work, project) = (seeded.work.path().to_owned(), seeded.project.clone());
        let pushed = tag.clone();
        tokio::task::spawn_blocking(move || push_commit(&work, &project, "round.txt", &format!("round {pushed}\n"), Some(&pushed)))
            .await
            .unwrap();
        let input = ReleaseInput {
            tag_name: tag.clone(),
            name: tag.clone(),
            notes: String::new(),
            is_draft: false,
            is_prerelease: false,
            target: String::new(),
        };
        let release = open.services.releases.create(&repo, &ada, input).await.unwrap();
        let body = format!("asset of {tag}").into_bytes();
        open.services.releases.upload_asset(&repo, &ada, release.id, "file.bin", "application/octet-stream", &body[..]).await.unwrap();
        releases.push(release.id);
        if rounds.is_multiple_of(3) {
            // Deleting a release removes its file: a backup must not keep the row alone.
            open.services.releases.delete(&repo, releases.remove(0)).await.unwrap();
        }
        let content = format!("lfs object of round {rounds}").into_bytes();
        let oid = LfsOid::from_digest(sha256(&content));
        open.services.lfs.store(&repo, &oid, content.len() as u64, &content[..]).await.unwrap();
        upload_artifact(&data, repo_id, seeded.run, rounds).await;
        // Wiki pages are created, and every third round one is deleted again.
        let wikis = &open.services.wikis;
        let created = wikis.create(&repo, &ada, &instance::wiki_input(&format!("Round {rounds}"), "", "round\n")).await.unwrap();
        if rounds.is_multiple_of(3) {
            let WikiLookup::Page(view) = wikis.page(&repo, Some(&ada), &created).await.unwrap() else { panic!("no page") };
            wikis.delete(&repo, &ada, view.page.id, view.page.head.generation).await.unwrap();
        }
        if let Some(status) = backup_process.try_wait().unwrap() {
            assert!(status.success(), "backup failed");
            break;
        }
    }
    assert!(rounds >= 2, "the writes overlapped the backup ({rounds} rounds)");
    open.close().await;

    let manifest = manifest(&backup);
    succeeded(&rbit(&restored, &["restore", "--from", backup.to_str().unwrap()], &[]), "restore");
    let found = consistency::check(&restored).await;
    assert_eq!(found.repositories, 2 + EXTRA_REPOSITORIES);
    assert_eq!(manifest["attachments"].as_u64().unwrap(), found.assets as u64);
    assert_eq!(manifest["lfs_objects"].as_u64().unwrap(), found.lfs as u64);
    assert_eq!(manifest["artifacts"].as_u64().unwrap(), found.artifacts as u64);
    assert!(found.releases >= 1 && found.lfs >= 2 && found.artifacts >= 1);
    assert_eq!((found.wikis, manifest["wikis"].as_u64()), (1, Some(1)));
}

/// Records an artifact, then moves its file into place, in the Actions service's order.
async fn upload_artifact(data: &Path, repo: i64, run: i64, round: u32) {
    let bytes = format!("artifact of round {round}").into_bytes();
    let mut db = connect(data).await;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO actions_artifacts (run_id, repo_id, attempt, name, size, sha256, created_at, expires_at) \
         VALUES (?, ?, 1, ?, ?, ?, 0, 4102444800) RETURNING id",
    )
    .bind(run)
    .bind(repo)
    .bind(format!("round-{round}"))
    .bind(i64::try_from(bytes.len()).unwrap())
    .bind(sha256(&bytes).to_vec())
    .fetch_one(&mut db)
    .await
    .unwrap();
    db.close().await.unwrap();
    let path = data.join("actions").join("artifacts").join(run.to_string()).join(format!("{id}.tar.gz"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
