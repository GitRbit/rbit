//! Batch answers, upload checks and lock rules without SQL, disk or tokens.
#[path = "support/fork_models.rs"]
mod models;
use rbit_contracts::AccountId;
use rbit_repositories::lfs::{
    BatchItem, BatchPort, LfsRefusal, LockIdFilter, MAX_BATCH_OBJECTS, ObjectDecision, Verification, admit_store, admit_transfer,
    decide_batch, list_lock_id, locker, may_unlock, next_cursor, page_size, parse_cursor, upload_refusal, verify_stored,
};
use std::{
    collections::HashMap,
    future::{Future, ready},
    sync::Mutex,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
#[derive(Default)]
struct Port {
    linked: HashMap<String, u64>,
    network: HashMap<String, u64>,
    on_disk: bool,
    calls: Mutex<Vec<String>>,
}
impl Port {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn trace(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}
impl BatchPort for Port {
    type Oid = String;
    type Error = &'static str;
    fn parse_oid(&self, oid: &str) -> Result<String, String> {
        if oid.len() == 2 { Ok(oid.to_owned()) } else { Err(format!("{oid} is not an object id")) }
    }
    fn linked(&self, oids: &[String]) -> impl Future<Output = Result<HashMap<String, u64>, Self::Error>> + Send {
        self.log(format!("linked:{}", oids.join(",")));
        ready(Ok(oids.iter().filter_map(|oid| self.linked.get(oid).map(|size| (oid.clone(), *size))).collect()))
    }
    fn through_network(&self, oid: &String) -> impl Future<Output = Result<Option<u64>, Self::Error>> + Send {
        self.log(format!("network:{oid}"));
        ready(Ok(self.network.get(oid).copied()))
    }
    fn bytes_present(&self, oid: &String) -> impl Future<Output = bool> + Send {
        self.log(format!("disk:{oid}"));
        ready(self.on_disk)
    }
}
fn item(oid: &str, size: i64) -> BatchItem<'_> {
    BatchItem { oid, size }
}
#[test]
fn transfers_check_write_access_archive_then_batch_size() {
    assert_eq!(admit_transfer(true, false, true, 1), Err(LfsRefusal::Forbidden));
    assert_eq!(admit_transfer(true, true, true, 1), Err(LfsRefusal::Archived));
    assert_eq!(admit_transfer(false, false, true, MAX_BATCH_OBJECTS), Ok(()));
    assert_eq!(
        admit_transfer(false, true, false, MAX_BATCH_OBJECTS + 1),
        Err(LfsRefusal::Invalid("a batch can name at most 1000 objects".into()))
    );
}
#[test]
fn downloads_use_links_then_the_network_once_per_object_and_remember_it() {
    let port = Port { linked: HashMap::from([("aa".into(), 5)]), network: HashMap::from([("bb".into(), 7)]), ..Port::default() };
    let objects = [item("aa", 1), item("bb", 2), item("bb", 2), item("cc", 3), item("bad", 1), item("dd", -1)];
    let decisions = run(decide_batch(&port, false, &objects, 10)).unwrap();
    assert_eq!(
        decisions,
        [
            ObjectDecision::Download { oid: "aa".into(), stored: 5 },
            ObjectDecision::Download { oid: "bb".into(), stored: 7 },
            ObjectDecision::Download { oid: "bb".into(), stored: 7 },
            ObjectDecision::Missing,
            ObjectDecision::Invalid("bad is not an object id".into()),
            ObjectDecision::Invalid("the size cannot be negative".into()),
        ]
    );
    assert_eq!(port.trace(), ["linked:aa,bb,bb,cc,dd", "network:bb", "network:cc"]);
}
#[test]
fn uploads_skip_only_present_bytes_of_the_same_size_and_respect_the_limit() {
    let port = Port { linked: HashMap::from([("aa".into(), 5), ("bb".into(), 5)]), on_disk: true, ..Port::default() };
    let objects = [item("aa", 5), item("bb", 6), item("cc", 11), item("dd", 10)];
    let decisions = run(decide_batch(&port, true, &objects, 10)).unwrap();
    assert_eq!(
        decisions,
        [
            ObjectDecision::Present,
            ObjectDecision::Upload { oid: "bb".into(), size: 6 },
            ObjectDecision::TooLarge,
            ObjectDecision::Upload { oid: "dd".into(), size: 10 },
        ]
    );
    assert_eq!(port.trace(), ["linked:aa,bb,cc,dd", "disk:aa", "network:cc", "network:dd"]);
    let missing_bytes = Port { linked: HashMap::from([("aa".into(), 5)]), ..Port::default() };
    assert_eq!(
        run(decide_batch(&missing_bytes, true, &[item("aa", 5)], 10)).unwrap(),
        [ObjectDecision::Upload { oid: "aa".into(), size: 5 }]
    );
}
#[test]
fn stored_uploads_and_verification_keep_their_messages() {
    assert_eq!(admit_store(false, true, 99, 1), Err(LfsRefusal::Forbidden));
    assert_eq!(admit_store(true, true, 99, 1), Err(LfsRefusal::Archived));
    assert_eq!(admit_store(true, false, 2, 1), Err(LfsRefusal::TooLarge));
    assert_eq!(admit_store(true, false, 1, 1), Ok(()));
    assert_eq!(upload_refusal(4, None).unwrap(), "the upload is larger than the announced 4 bytes");
    assert_eq!(upload_refusal(4, Some((3, true))).unwrap(), "received 3 bytes, but the object has 4");
    assert_eq!(upload_refusal(4, Some((4, false))).unwrap(), "the content does not match the object id");
    assert_eq!(upload_refusal(4, Some((4, true))), None);
    assert_eq!(verify_stored(Some(4), 4), Verification::Verified);
    assert_eq!(verify_stored(Some(3), 4), Verification::Mismatch("the object has 3 bytes, not 4".into()));
    assert_eq!(verify_stored(None, 4), Verification::Missing);
}
#[test]
fn locks_need_an_account_with_write_access_and_page_by_id() {
    let user = models::user();
    assert!(locker(true, Some(&user)).is_some());
    assert!(locker(false, Some(&user)).is_none());
    assert!(locker(true, None).is_none());
    assert!(may_unlock(models::repository(1).owner.id, &user, false, false), "the fixture owner holds its own lock");
    assert!(!may_unlock(AccountId(9), &user, false, true));
    assert!(!may_unlock(AccountId(9), &user, true, false));
    assert!(may_unlock(AccountId(9), &user, true, true));
    assert_eq!((page_size(None), page_size(Some(0)), page_size(Some(5)), page_size(Some(5000))), (100, 1, 5, 100));
    assert_eq!(parse_cursor(Some("")), Ok(None));
    assert_eq!(parse_cursor(Some("7")), Ok(Some(7)));
    assert_eq!(parse_cursor(Some("x")), Err("the cursor is not valid".into()));
    assert_eq!(
        (list_lock_id(None), list_lock_id(Some("3")), list_lock_id(Some("x"))),
        (LockIdFilter::Any, LockIdFilter::Id(3), LockIdFilter::Malformed)
    );
    let mut locks = vec![1_i64, 2, 3];
    assert_eq!(next_cursor(&mut locks, 2, |id| *id), Some("2".into()));
    assert_eq!(locks, [1, 2]);
    assert_eq!(next_cursor(&mut locks, 2, |id| *id), None);
}
