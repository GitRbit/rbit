//! Intentional retained-handle faults run in children, not the shared test process.
use super::*;
use std::time::Duration;

const CASE: &str = "RBIT_TEST_WIKI_NATIVE_QUARANTINE";
const DIRECTORY: &str = "RBIT_TEST_WIKI_NATIVE_DIRECTORY";

#[test]
fn runtime_abort_and_unaccounted_replacement_retain_actual_catalog_exclusion() {
    for case in ["runtime-abort", "unaccounted-replacement", "post-connect-binding-failure"] {
        let directory = tempfile::tempdir().unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "sqlite::wiki_mutations::full::tests_quarantine::native_quarantine_child", "--nocapture"])
            .env(CASE, case)
            .env(DIRECTORY, directory.path())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{case} child failed");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{case} exceeded subprocess safety deadline");
            }
            // Safety polling only; correctness assertions use real native locks.
            std::thread::sleep(Duration::from_millis(5));
        }
        directory.close().unwrap(); // Child exit releases intentional quarantines.
    }
}

#[test]
fn native_quarantine_child() {
    let Ok(case) = std::env::var(CASE) else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let directory = std::path::PathBuf::from(std::env::var_os(DIRECTORY).unwrap());
    let database = runtime.block_on(crate::Database::open(&directory.join("rbit.db"))).unwrap();
    if case == "post-connect-binding-failure" {
        runtime.block_on(async {
            let lease = database.wiki_catalog_lease().await.unwrap();
            let result = opening::open_after(lease, database.close()).await;
            assert!(matches!(result, Err(WikiWriteError::Catalog(WikiCatalogLeaseError::Changed))));
            let next = crate::Database::open(&directory.join("rbit.db")).await.unwrap();
            assert_excluded(&next).await;
            next.close().await;
        });
        return;
    }
    let writer = runtime.block_on(async { FullWikiWriter::open(database.wiki_catalog_lease().await.unwrap()).await.unwrap() });
    match case.as_str() {
        "runtime-abort" => {
            // Keep the controller alive: its waiting supervisor cannot observe a
            // graceful-close signal before runtime teardown cancels that supervisor.
            drop(runtime);
            drop(writer);
            let next = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
            next.block_on(assert_excluded(&database));
        }
        "unaccounted-replacement" => runtime.block_on(async {
            // Actual old native worker closes, but its generation was NOT accounted
            // by the owner (models a pool-discard path). Never infer exit from size0.
            let connection = writer.0.pool.acquire().await.unwrap();
            connection.close().await.unwrap();
            let acquired = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::select! {
                    acquired = writer.0.pool.acquire() => { assert!(acquired.is_err()); },
                    () = writer.0.state.unproved() => {},
                }
            })
            .await;
            acquired.unwrap();
            assert!(matches!(writer.close().await, Err(WikiWriteError::Quarantined)));
            drop(writer);
            assert_excluded(&database).await;
        }),
        _ => panic!("unknown private test case"),
    }
    // Parent owns cleanup after this process releases intentional retained handles.
    // No production path or global registry/cache is touched by these fault fixtures.
    drop(database);
}

async fn assert_excluded(database: &crate::Database) {
    for _ in 0..3 {
        assert!(matches!(database.wiki_catalog_lease().await, Err(WikiCatalogLeaseError::Busy)));
    }
}
