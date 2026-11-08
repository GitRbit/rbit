//! Mutual consistency of a restored data directory: every row that refers to a
//! repository, a Git ref or a stored file finds it, intact.

use std::path::Path;

use sqlx::Connection;

use super::instance::{connect, git, git_dir, sha256};

/// What [`check`] found, for comparing with what was written.
#[derive(Debug, Default)]
pub struct Counts {
    pub repositories: usize,
    pub releases: usize,
    pub assets: usize,
    pub lfs: usize,
    pub artifacts: usize,
    pub caches: usize,
    pub logs: usize,
}

/// Panics, naming the row, unless everything the database at `data` refers to is present.
pub async fn check(data: &Path) -> Counts {
    let mut db = connect(data).await;
    let mut counts = Counts::default();
    let repos: Vec<(i64, i64)> = sqlx::query_as("SELECT id, owner_id FROM repositories").fetch_all(&mut db).await.unwrap();
    for (id, owner) in &repos {
        let path = data.join("repositories").join(owner.to_string()).join(format!("{id}.git"));
        assert!(path.join("HEAD").is_file(), "repository {id} has no directory");
        git(data, &[&git_dir(&path), "fsck", "--no-progress", "--no-dangling"]);
        counts.repositories += 1;
    }
    let releases: Vec<(i64, i64, String)> = sqlx::query_as(
        "SELECT r.repo_id, p.owner_id, r.tag_name FROM releases r JOIN repositories p ON p.id = r.repo_id WHERE r.is_draft = 0",
    )
    .fetch_all(&mut db)
    .await
    .unwrap();
    for (repo, owner, tag) in &releases {
        let path = data.join("repositories").join(owner.to_string()).join(format!("{repo}.git"));
        git(data, &[&git_dir(&path), "rev-parse", "--verify", "--quiet", &format!("refs/tags/{tag}^{{commit}}")]);
        counts.releases += 1;
    }
    let assets: Vec<(i64, i64, i64, Vec<u8>)> =
        sqlx::query_as("SELECT repo_id, id, size, sha256 FROM release_assets").fetch_all(&mut db).await.unwrap();
    for (repo, id, size, digest) in &assets {
        stored(data, &Path::new("attachments").join(repo.to_string()).join(id.to_string()), *size, digest);
        counts.assets += 1;
    }
    let objects: Vec<(String, i64)> = sqlx::query_as("SELECT oid, size FROM lfs_objects").fetch_all(&mut db).await.unwrap();
    for (oid, size) in &objects {
        let digest: Vec<u8> = (0..32).map(|i| u8::from_str_radix(&oid[i * 2..i * 2 + 2], 16).unwrap()).collect();
        stored(data, &Path::new("lfs").join(&oid[..2]).join(&oid[2..4]).join(oid), *size, &digest);
        counts.lfs += 1;
    }
    let artifacts: Vec<(i64, i64, i64, Vec<u8>)> =
        sqlx::query_as("SELECT run_id, id, size, sha256 FROM actions_artifacts").fetch_all(&mut db).await.unwrap();
    for (run, id, size, digest) in &artifacts {
        stored(data, &Path::new("actions").join("artifacts").join(run.to_string()).join(format!("{id}.tar.gz")), *size, digest);
        counts.artifacts += 1;
    }
    let caches: Vec<(i64, i64, i64, Vec<u8>)> =
        sqlx::query_as("SELECT repo_id, id, size, sha256 FROM actions_caches").fetch_all(&mut db).await.unwrap();
    for (repo, id, size, digest) in &caches {
        stored(data, &Path::new("actions").join("caches").join(repo.to_string()).join(format!("{id}.tar.gz")), *size, digest);
        counts.caches += 1;
    }
    let logs: Vec<(i64, i64, String, i64)> = sqlx::query_as(
        "SELECT j.repo_id, t.id, t.log_state, t.log_bytes FROM actions_tasks t JOIN actions_jobs j ON j.id = t.job_id \
         WHERE t.log_state <> 'expired' AND t.log_bytes > 0",
    )
    .fetch_all(&mut db)
    .await
    .unwrap();
    for (repo, task, state, bytes) in &logs {
        let dir = data.join("actions").join("logs").join(repo.to_string());
        let index = std::fs::read(dir.join(format!("{task}.idx"))).unwrap_or_else(|_| panic!("task {task} has no log index"));
        assert!(index.len() >= 8, "task {task}'s log index is empty");
        let lines = if state == "final" {
            let file = std::fs::File::open(dir.join(format!("{task}.log.gz"))).unwrap_or_else(|_| panic!("task {task} has no log"));
            let mut lines = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(file), &mut lines).unwrap();
            lines
        } else {
            std::fs::read(dir.join(format!("{task}.log"))).unwrap_or_else(|_| panic!("task {task} has no live log"))
        };
        assert!(i64::try_from(lines.len()).unwrap() >= *bytes, "task {task}'s log is shorter than its record");
        counts.logs += 1;
    }
    db.close().await.unwrap();
    counts
}

fn stored(data: &Path, relative: &Path, size: i64, digest: &[u8]) {
    let bytes = std::fs::read(data.join(relative)).unwrap_or_else(|_| panic!("{} is missing", relative.display()));
    assert_eq!(i64::try_from(bytes.len()).unwrap(), size, "{} has the wrong size", relative.display());
    assert_eq!(sha256(&bytes).as_slice(), digest, "{} has the wrong digest", relative.display());
}
