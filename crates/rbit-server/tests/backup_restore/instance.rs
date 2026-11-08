//! A realistic instance: an administrator with a pending authenticator secret, a
//! repository with branches, lightweight and annotated tags, Git LFS objects,
//! a release with a file, an Actions secret, a finished run with an artifact, a cache
//! entry and a compressed log, and a running task with a live log; plus an empty
//! repository with a non-default `HEAD`.

use std::{
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rbit_core::{Config, Timestamp, User, Visibility, actions::SecretVisibility, lfs::LfsOid};
use rbit_services::{CreateRepository, Services, SignUp, actions::secrets::ActionsTarget, releases::ReleaseInput};
use rbit_storage::{kv::KvStore, sqlite::Database};
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, Connection, SqliteConnection};

pub const TOTP_SEED: &[u8] = b"twenty-byte-totpseed";
pub const SERVER_SECRET: &[u8] = b"sealed server value";
pub const ACTIONS_SECRET: &str = "deploy-token-value";

pub fn config(data: &Path) -> Config {
    let mut config = Config::default();
    data.clone_into(&mut config.storage.data_dir);
    config.actions.enabled = true;
    config
}

pub struct Open {
    pub database: Database,
    /// Held open as a running server holds it.
    pub _kv: KvStore,
    pub services: Services,
}

pub async fn open(data: &Path) -> Open {
    let config = config(data);
    std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
    let database = Database::open_with_key_file(&config.storage.sqlite_path(), &config.storage.secret_key_path()).await.unwrap();
    let kv = KvStore::open(&config.storage.kv_path()).unwrap();
    let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
    Open { database, _kv: kv, services }
}

impl Open {
    pub async fn ada(&self) -> User {
        let id = self.database.accounts().find_account_by_name("ada").await.unwrap().unwrap().id;
        self.database.accounts().find_user(id).await.unwrap().unwrap()
    }

    pub async fn close(self) {
        drop(self.services);
        self.database.close().await;
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=Ada", "-c", "user.email=ada@example.com", "-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

pub fn git_dir(path: &Path) -> String {
    format!("--git-dir={}", path.display())
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Commits `file` with `content` in `work` and pushes `main` (and `tag`, if given) to `bare`.
pub fn push_commit(work: &Path, bare: &Path, file: &str, content: &str, tag: Option<&str>) {
    std::fs::write(work.join(file), content).unwrap();
    git(work, &["add", "."]);
    git(work, &["commit", "-q", "-m", &format!("Change {file}")]);
    let mut refspecs = vec!["HEAD:refs/heads/main".to_owned()];
    if let Some(tag) = tag {
        refspecs.push(format!("HEAD:refs/tags/{tag}"));
    }
    let mut args = vec!["push", "-q", bare.to_str().unwrap()];
    args.extend(refspecs.iter().map(String::as_str));
    git(work, &args);
}

/// What [`populate`] created, for checking a restored copy.
pub struct Seeded {
    pub project: PathBuf,
    pub empty: PathBuf,
    pub work: tempfile::TempDir,
    pub run: i64,
    pub lfs: Vec<(LfsOid, Vec<u8>)>,
    /// Data-directory-relative stored files and their bytes.
    pub files: Vec<(PathBuf, Vec<u8>)>,
}

pub async fn populate(data: &Path) -> Seeded {
    let open = open(data).await;
    let services = &open.services;
    let signup = SignUp { username: "ada".into(), email: "ada@example.com".into(), password: "correct horse battery".into() };
    let ada = services.accounts.create_user(signup, Some(true)).await.unwrap();
    let mut paths = Vec::new();
    for name in ["project", "empty"] {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility: Visibility::Private };
        services.repositories.create(&ada, input).await.unwrap();
        paths.push(services.repositories.open(Some(&ada), "ada", name).await.unwrap().disk_path);
    }
    std::fs::write(paths[1].join("HEAD"), "ref: refs/heads/trunk\n").unwrap();

    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "-q"]);
    push_commit(work.path(), &paths[0], "README.md", "# Project\n", Some("v1"));
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(work.path().join("feature.txt"), "feature\n").unwrap();
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "-q", "-m", "Feature"]);
    git(work.path(), &["tag", "-a", "v2", "-m", "Annotated"]);
    git(work.path(), &["push", "-q", paths[0].to_str().unwrap(), "feature", "refs/tags/v2"]);
    git(work.path(), &["checkout", "-q", "main"]);

    open.database.server_secrets().get_or_insert("probe", SERVER_SECRET).await.unwrap();
    assert!(open.database.two_factor().begin_enrolment(ada.id(), TOTP_SEED).await.unwrap());

    let repo = services.repositories.open(Some(&ada), "ada", "project").await.unwrap();
    let mut lfs = Vec::new();
    for content in [&b"large file content"[..], b"another large file"] {
        let oid = LfsOid::from_digest(sha256(content));
        services.lfs.store(&repo, &oid, content.len() as u64, content).await.unwrap();
        lfs.push((oid, content.to_vec()));
    }
    let release = ReleaseInput {
        tag_name: "v1".into(),
        name: "First".into(),
        notes: String::new(),
        is_draft: false,
        is_prerelease: false,
        target: String::new(),
    };
    let release = services.releases.create(&repo, &ada, release).await.unwrap();
    let asset = services.releases.upload_asset(&repo, &ada, release.id, "app.zip", "application/zip", &b"release bytes"[..]).await.unwrap();
    let mut files =
        vec![(Path::new("attachments").join(repo.repo.repo.id.0.to_string()).join(asset.id.0.to_string()), b"release bytes".to_vec())];

    let actions = services.actions.as_ref().expect("Actions is enabled");
    open.database.actions_settings().set_enabled(repo.repo.repo.id, true, Timestamp::now()).await.unwrap();
    actions.put_secret(&ada, ActionsTarget::Repository(&repo), "DEPLOY", ACTIONS_SECRET, SecretVisibility::All).await.unwrap();
    let (run, action_files) = seed_actions(data, repo.repo.repo.id.0).await;
    files.extend(action_files);
    open.close().await;
    Seeded { project: paths.remove(0), empty: paths.remove(0), work, run, lfs, files }
}

pub async fn connect(data: &Path) -> SqliteConnection {
    let options = rbit_storage::sqlite::writer_compatibility::register(sqlx::sqlite::SqliteConnectOptions::new());
    options.filename(data.join("rbit.db")).foreign_keys(true).busy_timeout(std::time::Duration::from_secs(30)).connect().await.unwrap()
}

/// A finished run whose task left an artifact, a cache entry and a compressed log, and a
/// running task with a live log, written as the Actions service writes them.
async fn seed_actions(data: &Path, repo: i64) -> (i64, Vec<(PathBuf, Vec<u8>)>) {
    let mut db = connect(data).await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let run: i64 = sqlx::query_scalar(
        "INSERT INTO actions_runs (repo_id, run_number, workflow_path, workflow_name, title, event, ref, sha, trusted, status, \
         event_payload, created_at) VALUES (?, 1, '.rbit/workflows/ci.yml', 'CI', 'Build', 'push', 'refs/heads/main', ?, 1, 3, '{}', 0) \
         RETURNING id",
    )
    .bind(repo)
    .bind(sha)
    .fetch_one(&mut db)
    .await
    .unwrap();
    let mut files = Vec::new();
    let mut tasks = Vec::new();
    for (key, state) in [("build", "final"), ("deploy", "live")] {
        let job: i64 = sqlx::query_scalar(
            "INSERT INTO actions_jobs (run_id, repo_id, job_key, name, payload, status, created_at) VALUES (?, ?, ?, ?, x'', 3, 0) RETURNING id",
        )
        .bind(run)
        .bind(repo)
        .bind(key)
        .bind(key)
        .fetch_one(&mut db)
        .await
        .unwrap();
        let lines = format!("{key} line one\n{key} line two\n");
        let task: i64 = sqlx::query_scalar(
            "INSERT INTO actions_tasks (job_id, attempt, status, started_at, heartbeat_at, log_lines, log_bytes, log_state) \
             VALUES (?, 1, 3, 0, 0, 2, ?, ?) RETURNING id",
        )
        .bind(job)
        .bind(i64::try_from(lines.len()).unwrap())
        .bind(state)
        .fetch_one(&mut db)
        .await
        .unwrap();
        let dir = Path::new("actions").join("logs").join(repo.to_string());
        let first = u64::try_from(lines.find('\n').unwrap() + 1).unwrap();
        let index = [first.to_le_bytes(), (lines.len() as u64).to_le_bytes()].concat();
        files.push((dir.join(format!("{task}.idx")), index));
        if state == "final" {
            let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gzip.write_all(lines.as_bytes()).unwrap();
            files.push((dir.join(format!("{task}.log.gz")), gzip.finish().unwrap()));
        } else {
            files.push((dir.join(format!("{task}.log")), lines.into_bytes()));
        }
        tasks.push((job, task));
    }
    let artifact = b"artifact tarball".to_vec();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO actions_artifacts (run_id, repo_id, attempt, job_id, name, size, sha256, created_at, expires_at) \
         VALUES (?, ?, 1, ?, 'dist', ?, ?, 0, 4102444800) RETURNING id",
    )
    .bind(run)
    .bind(repo)
    .bind(tasks[0].0)
    .bind(i64::try_from(artifact.len()).unwrap())
    .bind(sha256(&artifact).to_vec())
    .fetch_one(&mut db)
    .await
    .unwrap();
    files.push((Path::new("actions").join("artifacts").join(run.to_string()).join(format!("{id}.tar.gz")), artifact));
    let cache = b"cache tarball".to_vec();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO actions_caches (repo_id, scope, key, version, size, sha256, created_at, used_at) \
         VALUES (?, 'heads/main', 'deps', ?, ?, ?, 0, 0) RETURNING id",
    )
    .bind(repo)
    .bind("a".repeat(64))
    .bind(i64::try_from(cache.len()).unwrap())
    .bind(sha256(&cache).to_vec())
    .fetch_one(&mut db)
    .await
    .unwrap();
    files.push((Path::new("actions").join("caches").join(repo.to_string()).join(format!("{id}.tar.gz")), cache));
    db.close().await.unwrap();
    for (path, bytes) in &files {
        let path = data.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    (run, files)
}

/// Runs the real executable against `data` with extra environment `env`.
pub fn rbit(data: &Path, args: &[&str], env: &[(&str, &Path)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rbit"));
    command.args(args).env("RBIT_STORAGE__DATA_DIR", data).env("RBIT_ACTIONS__ENABLED", "true").env_remove("RBIT_CONFIG");
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().unwrap()
}

pub fn succeeded(output: &std::process::Output, what: &str) {
    assert!(output.status.success(), "{what} failed: {}", String::from_utf8_lossy(&output.stderr));
}

pub fn failed_with(output: &std::process::Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected a failure mentioning {message:?}");
    assert!(stderr.contains(message), "expected {message:?} in:\n{stderr}");
}
