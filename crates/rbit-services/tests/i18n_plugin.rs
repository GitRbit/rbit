//! Live catalogs, durable preferences, atomic package updates and request cancellation.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    future::{Future, poll_fn},
    io::{Cursor, Write},
    sync::Arc,
    task::Poll,
    time::Duration,
};

use rbit_core::{Config, Timestamp, User};
use rbit_plugin_api::PluginState;
use rbit_services::{ServiceError, Services};
use rbit_storage::{Database, KvStore, NewUser, sqlite::language_packs::NewLanguagePack};
use sha2::{Digest, Sha256};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
use zip::{ZipWriter, write::SimpleFileOptions};

const I18N_ID: &str = "rbit.i18n";

struct Fixture {
    _directory: tempfile::TempDir,
    database: Database,
    kv: KvStore,
    config: Arc<Config>,
    services: Services,
    admin: User,
    reader: User,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.updates.check_for_updates = false;
        config.actions.enabled = false;
        config.i18n.enabled = false;
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let mut users = Vec::new();
        for (name, is_admin) in [("alice", true), ("bob", false)] {
            users.push(
                database
                    .accounts()
                    .create_user(NewUser {
                        name: name.parse().unwrap(),
                        email: format!("{name}@example.com").parse().unwrap(),
                        display_name: None,
                        password_hash: "unused".to_owned(),
                        is_admin,
                    })
                    .await
                    .unwrap(),
            );
        }
        let config = Arc::new(config);
        let services = Services::new(Arc::clone(&config), &database, &kv).await.unwrap();
        let reader = users.pop().unwrap();
        let admin = users.pop().unwrap();
        Self { _directory: directory, database, kv, config, services, admin, reader }
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::with_capacity(64), |mut output, byte| {
        write!(output, "{byte:02x}").unwrap();
        output
    })
}

fn archive(id: &str, locale: &str, version: &str, text: &str) -> Vec<u8> {
    let files = BTreeMap::from([("LICENSE", "Permission to use."), ("catalogs/core.ftl", text)]);
    let mut manifest = format!(
        "schema = 1\nid = \"{id}\"\nversion = \"{version}\"\nlocale = \"{locale}\"\ndirection = \"ltr\"\n\
         message_schema = \"^1.0\"\npublisher = \"Language contributors\"\nlicense = \"MIT\"\nlicense_file = \"LICENSE\"\n\
         [catalogs]\ncore = \"catalogs/core.ftl\"\n[checksums]\n",
    );
    for (path, contents) in &files {
        writeln!(manifest, "\"{path}\" = \"{}\"", digest(contents.as_bytes())).unwrap();
    }
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).unix_permissions(0o644);
    writer.start_file("pack.toml", options).unwrap();
    writer.write_all(manifest.as_bytes()).unwrap();
    for (path, contents) in files {
        writer.start_file(path, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[tokio::test]
async fn disabled_install_then_live_update_and_disable_preserve_consistent_old_data_snapshots() {
    let fixture = Fixture::new().await;
    let french = archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n");
    let installed = fixture.services.install_language_pack(&fixture.admin, &french).await.unwrap();
    assert_eq!(installed.message_count, 1);
    assert!(installed.message_total > 1);
    assert!(!fixture.services.i18n_enabled());
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0, "installing data while disabled does not expose plugin UI");
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).lang(), "en");
    fixture.services.set_account_locale(&fixture.reader, Some("fr")).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    let old = fixture.services.locale_snapshot(Some("fr"), None, None);
    let generation = fixture.services.i18n_status().await.unwrap().generation;
    let old_ui = fixture.services.ui_snapshot();
    assert_eq!(old_ui.entries.len(), 1);
    assert_eq!(old_ui.entries[0].owner.as_str(), I18N_ID);
    assert_eq!(Some(old_ui.entries[0].generation), generation);
    assert_eq!((old.lang(), old.text("core", "nav-explore")), ("fr", "Explorer".to_owned()));
    assert_eq!(old.text("core", "nav-sign-in"), "Sign in");
    let revision = fixture.services.plugin_revision();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    assert_eq!(fixture.services.plugin_revision(), revision, "enabling an active provider is idempotent");

    let replacement = archive("example.french", "fr", "2.0.0", "nav-explore = Parcourir\n");
    fixture.services.install_language_pack(&fixture.admin, &replacement).await.unwrap();
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Parcourir");
    assert_eq!(old.text("core", "nav-explore"), "Explorer", "a render keeps one immutable catalog identity");
    assert!(fixture.services.plugin_revision() > revision);
    tokio::time::timeout(Duration::from_secs(2), fixture.services.set_i18n_enabled(&fixture.admin, false)).await.unwrap().unwrap();
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).lang(), "en");
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0);
    assert_eq!(old_ui.entries.len(), 1, "retained presentation snapshots do not keep a generation admitted");
    assert_eq!(old.text("core", "nav-explore"), "Explorer", "data snapshots carry no authority and cannot deadlock disable");
    assert_eq!(fixture.database.language_packs().account_locale(fixture.reader.id()).await.unwrap().as_deref(), Some("fr"));
    assert!(fixture.services.remove_language_pack(&fixture.admin, "example.french").await.unwrap());
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    assert_ne!(fixture.services.i18n_status().await.unwrap().generation, generation);
    let current_ui = fixture.services.ui_snapshot();
    assert_eq!(current_ui.entries.len(), 1);
    assert_eq!(Some(current_ui.entries[0].generation), fixture.services.i18n_status().await.unwrap().generation);
    assert!(current_ui.revision > old_ui.revision);
    assert_eq!(fixture.services.available_locales(), ["en"]);
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).lang(), "en");
    fixture.services.shutdown().await;
    assert_eq!(fixture.services.ui_snapshot().entries.len(), 0);
}

#[tokio::test]
async fn malformed_updates_and_conflicting_providers_leave_storage_and_catalog_unchanged() {
    let fixture = Fixture::new().await;
    let french = archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n");
    fixture.services.install_language_pack(&fixture.admin, &french).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    let revision = fixture.services.plugin_revision();
    let other = archive("example.conflict", "fr", "1.0.0", "nav-explore = Autre\n");
    assert!(matches!(fixture.services.install_language_pack(&fixture.admin, b"not a zip").await, Err(ServiceError::Invalid(_))));
    assert!(matches!(fixture.services.install_language_pack(&fixture.admin, &other).await, Err(ServiceError::Invalid(_))));
    assert_eq!(fixture.services.plugin_revision(), revision);
    let stored = fixture.database.language_packs().list().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].bytes, french);
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Explorer");
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn installed_versions_are_immutable_and_downgrades_do_not_change_catalogs_or_metadata() {
    let fixture = Fixture::new().await;
    let french = archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n");
    let original = fixture.services.install_language_pack(&fixture.admin, &french).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    let revision = fixture.services.plugin_revision();
    assert_eq!(fixture.services.install_language_pack(&fixture.admin, &french).await.unwrap(), original);
    for version in ["1.0.0", "0.9.0", "1.0.0+repacked"] {
        let replacement = archive("example.french", "fr", version, "nav-explore = Autre\n");
        assert!(matches!(fixture.services.install_language_pack(&fixture.admin, &replacement).await, Err(ServiceError::Invalid(_))));
    }
    assert_eq!(fixture.services.plugin_revision(), revision);
    assert_eq!(fixture.services.language_packs(&fixture.admin).await.unwrap(), [original]);
    assert_eq!(fixture.database.language_packs().get("example.french").await.unwrap().unwrap().bytes, french);
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Explorer");
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn package_and_plugin_admin_operations_reject_non_admins_while_own_preferences_remain_available() {
    let fixture = Fixture::new().await;
    let french = archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n");
    assert!(matches!(fixture.services.install_language_pack(&fixture.reader, &french).await, Err(ServiceError::Forbidden)));
    assert!(matches!(fixture.services.remove_language_pack(&fixture.reader, "example.french").await, Err(ServiceError::Forbidden)));
    assert!(matches!(fixture.services.language_packs(&fixture.reader).await, Err(ServiceError::Forbidden)));
    assert!(matches!(fixture.services.set_i18n_enabled(&fixture.reader, true).await, Err(ServiceError::Forbidden)));
    assert_eq!(fixture.database.plugin_settings().desired_enabled(I18N_ID).await.unwrap(), None);
    assert_eq!(fixture.database.language_packs().list().await.unwrap(), Vec::new());
    fixture.services.set_account_locale(&fixture.reader, Some("fr-CA")).await.unwrap();
    assert_eq!(
        fixture.database.accounts().find_session_user(fixture.reader.id(), false).await.unwrap().unwrap().locale.as_deref(),
        Some("fr-CA")
    );
    assert!(matches!(fixture.services.set_account_locale(&fixture.reader, Some("fr\r\nHeader: bad")).await, Err(ServiceError::Invalid(_))));
    assert_eq!(fixture.database.language_packs().account_locale(fixture.reader.id()).await.unwrap().as_deref(), Some("fr-CA"));
    fixture.services.shutdown().await;
}

#[tokio::test]
async fn explicit_intent_and_archives_survive_restart_and_override_deployment_defaults() {
    let fixture = Fixture::new().await;
    fixture
        .services
        .install_language_pack(&fixture.admin, &archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n"))
        .await
        .unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    let prior = fixture.services.locale_snapshot(Some("fr"), None, None);
    fixture.services.shutdown().await;
    let resumed = Services::new(Arc::clone(&fixture.config), &fixture.database, &fixture.kv).await.unwrap();
    assert_eq!(resumed.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Explorer");
    assert_ne!(
        resumed.locale_snapshot(Some("fr"), None, None).revision(),
        prior.revision(),
        "a host recreation must invalidate browser catalog identity even with unchanged archives"
    );
    resumed.set_i18n_enabled(&fixture.admin, false).await.unwrap();
    resumed.shutdown().await;
    let mut enabled_default = (*fixture.config).clone();
    enabled_default.i18n.enabled = true;
    let disabled = Services::new(Arc::new(enabled_default), &fixture.database, &fixture.kv).await.unwrap();
    assert!(!disabled.i18n_enabled());
    assert_eq!(disabled.locale_snapshot(Some("fr"), None, None).lang(), "en");
    assert_eq!(disabled.language_packs(&fixture.admin).await.unwrap().len(), 1);
    disabled.shutdown().await;
}

#[tokio::test]
async fn a_corrupt_stored_package_fails_to_english_and_can_be_removed_without_parsing_it() {
    let fixture = Fixture::new().await;
    let store = fixture.database.language_packs();
    for id in ["example.broken", "example.other-broken"] {
        store
            .put(&NewLanguagePack {
                id,
                locale: "fr",
                version: "1.0.0",
                bytes: b"bad",
                digest: &"0".repeat(64),
                message_count: 1,
                message_total: rbit_messages::MessageSchema::builtin().len(),
                installed_by: Some(fixture.admin.id()),
                updated_at: Timestamp::now(),
            })
            .await
            .unwrap();
    }
    fixture.database.plugin_settings().set_desired_enabled(I18N_ID, true, Some(fixture.admin.id()), Timestamp::now()).await.unwrap();
    fixture.services.shutdown().await;
    let resumed = Services::new(Arc::clone(&fixture.config), &fixture.database, &fixture.kv).await.unwrap();
    assert!(!resumed.i18n_enabled());
    assert_eq!(resumed.ui_snapshot().entries.len(), 0, "failed preparation must not publish staged UI");
    assert_eq!(resumed.locale_snapshot(Some("fr"), None, None).lang(), "en");
    assert_ne!(resumed.i18n_status().await.unwrap().state, PluginState::Active);
    resumed.set_i18n_enabled(&fixture.admin, false).await.unwrap();
    assert!(
        resumed.remove_language_pack(&fixture.admin, "example.other-broken").await.unwrap(),
        "repair must not parse unrelated corrupt packages"
    );
    let repaired = archive(
        "example.broken",
        "fr",
        "2.0.0",
        "nav-explore = Repare
",
    );
    resumed.install_language_pack(&fixture.admin, &repaired).await.unwrap();
    resumed.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    assert_eq!(resumed.i18n_status().await.unwrap().state, PluginState::Active);
    assert_eq!(resumed.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Repare");
    resumed.shutdown().await;
}

#[tokio::test]
async fn cancelling_an_install_waiter_keeps_the_serialized_host_operation_until_commit_and_publication() {
    let fixture = Fixture::new().await;
    let french = archive("example.french", "fr", "1.0.0", "nav-explore = Explorer\n");
    let mut blocker =
        SqliteConnection::connect_with(&SqliteConnectOptions::new().filename(fixture.config.storage.sqlite_path())).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE").execute(&mut blocker).await.unwrap();
    let services = fixture.services.clone();
    let admin = fixture.admin.clone();
    let (started, admitted) = tokio::sync::oneshot::channel();
    let request = tokio::spawn(async move {
        let mut operation = Box::pin(services.install_language_pack(&admin, &french));
        let mut started = Some(started);
        poll_fn(|context| {
            assert!(operation.as_mut().poll(context).is_pending(), "the SQLite write is deliberately blocked");
            started.take().unwrap().send(()).unwrap();
            Poll::Ready(())
        })
        .await;
        operation.await
    });
    admitted.await.unwrap();
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(matches!(
        fixture.services.set_i18n_enabled(&fixture.admin, true).await,
        Err(ServiceError::Conflict("plugin lifecycle operation"))
    ));
    sqlx::query("COMMIT").execute(&mut blocker).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match fixture.services.set_i18n_enabled(&fixture.admin, true).await {
                Ok(()) => break,
                Err(ServiceError::Conflict("plugin lifecycle operation")) => tokio::task::yield_now().await,
                other => panic!("unexpected lifecycle outcome: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(fixture.services.language_packs(&fixture.admin).await.unwrap().len(), 1);
    assert_eq!(fixture.services.locale_snapshot(Some("fr"), None, None).text("core", "nav-explore"), "Explorer");
    fixture.services.shutdown().await;
}
