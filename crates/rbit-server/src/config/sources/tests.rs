//! Closed temporary fixtures: configuration parsing must not touch store contents.
use super::*;
use rbit_storage::{Work, count_work};
fn deployment_file(directory: &Path, data: &Path) -> std::path::PathBuf {
    let file = directory.join("rbit.toml");
    let path = data.to_string_lossy().replace('\\', "/");
    std::fs::write(&file, format!("[storage]\ndata_dir = {path:?}\n[repository]\ndefault_branch = \"trunk\"\n")).unwrap();
    file
}
#[tokio::test]
async fn deployment_parse_succeeds_without_opening_invalid_sqlite_or_key_paths() {
    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let mut config = Config::default();
    config.storage.data_dir = data.clone();
    std::fs::write(config.storage.sqlite_path(), b"not a SQLite database").unwrap();
    // A directory is not a readable sealing key; parse must not try to load it.
    std::fs::create_dir(config.storage.secret_key_path()).unwrap();
    let file = deployment_file(directory.path(), &data);
    let (sources, work) = count_work(std::future::ready(ConfigSources::parse(Some(&file)))).await;
    let sources = sources.unwrap();
    assert_eq!(work, Work::default());
    assert!(!format!("{sources:?}").contains(data.to_str().unwrap()));
    assert_eq!(sources.deployment().storage.data_dir, data);
    assert_eq!(sources.finish(None).unwrap().repository.default_branch, "trunk");
    assert_eq!(std::fs::read(config.storage.sqlite_path()).unwrap(), b"not a SQLite database");
    assert!(config.storage.secret_key_path().is_dir());
    assert!(!config.storage.kv_path().exists());
}
#[tokio::test]
async fn supplied_owned_database_hydrates_sealed_saved_settings_with_one_query_and_keeps_deployment_storage() {
    let directory = tempfile::tempdir().unwrap();
    let mut deployment = Config::default();
    deployment.storage.data_dir = directory.path().join("data");
    std::fs::create_dir(&deployment.storage.data_dir).unwrap();
    let file = deployment_file(directory.path(), &deployment.storage.data_dir);
    let database = Database::open(&deployment.storage.sqlite_path()).await.unwrap();
    let mut saved = deployment.clone();
    saved.storage.data_dir = directory.path().join("must-not-open");
    saved.repository.default_branch = "develop".into();
    saved.server.site_name = "Saved fixture".into();
    saved.mail.smtp_password = "fixture saved secret".into();
    let mut document = serde_json::to_value(&saved).unwrap();
    database.sealing_key().seal_secret_fields(&mut document).unwrap();
    database.instance_settings().save(&[(rbit_services::admin::configuration::CONFIGURATION_KEY, &document.to_string())]).await.unwrap();
    let sources = ConfigSources::parse(Some(&file)).unwrap();
    let (stored, work) = count_work(sources.read_saved(&database)).await;
    assert_eq!(work, Work { acquisitions: 1, statements: 1, cache_writes: 0 });
    let stored = stored.unwrap();
    assert!(!format!("{stored:?}").contains("fixture saved secret"));
    let effective = sources.finish(stored).unwrap();
    assert_eq!(effective.server.site_name, "Saved fixture");
    assert_eq!(effective.repository.default_branch, "develop");
    assert_eq!(effective.mail.smtp_password, "fixture saved secret");
    assert_eq!(effective.storage.data_dir, deployment.storage.data_dir);
    assert!(!directory.path().join("must-not-open").exists());
    assert!(!deployment.storage.kv_path().exists());
    database.close().await;
}
