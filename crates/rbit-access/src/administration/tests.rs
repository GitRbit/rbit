use std::sync::Mutex;

use super::*;
use crate::organizations::{AccessError, Actor, tests::ready};

const ROOT: u32 = 1;
const OTHER_ADMIN: u32 = 2;
const ALICE: u32 = 3;

/// Users `u{id}`; root is the only active administrator unless `admins` says otherwise.
/// Alice solely owns `acme` when `sole_owner`; repository 7 is `alice/rocket`.
#[derive(Debug)]
struct Store {
    admins: Mutex<Vec<u32>>,
    sole_owner: bool,
    writes: Mutex<Vec<&'static str>>,
}

impl Store {
    fn new(admins: &[u32]) -> Self {
        Self { admins: Mutex::new(admins.to_vec()), sole_owner: false, writes: Mutex::new(Vec::new()) }
    }

    fn guarded(&self, id: u32, removes_admin: bool, write: &'static str) -> Moderation {
        let admins = self.admins.lock().unwrap();
        if id > 5 {
            return Moderation::NotFound;
        }
        if removes_admin && admins.as_slice() == [id] {
            return Moderation::LastAdmin;
        }
        self.writes.lock().unwrap().push(write);
        Moderation::Applied
    }
}

#[expect(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl AdministrationRepository for Store {
    type Error = &'static str;
    type Id = u32;
    type RepoId = u32;
    type User = u32;
    type Organization = &'static str;
    type Repository = u32;

    fn user_id(user: &u32) -> u32 {
        *user
    }
    fn user_name(user: &u32) -> &str {
        ["", "u1", "u2", "u3", "u4", "u5"][usize::try_from(*user).unwrap()]
    }
    fn repository_name(_: &u32) -> String {
        "alice/rocket".to_owned()
    }
    async fn list_users(&self, filter: ListFilter<'_>) -> Result<Listing<u32>, &'static str> {
        Ok(Listing { items: vec![u32::try_from(filter.offset).unwrap()], total: 61 })
    }
    async fn find_user(&self, id: u32) -> Result<Option<u32>, &'static str> {
        Ok((id <= 5).then_some(id))
    }
    async fn set_admin(&self, id: u32, is_admin: bool) -> Result<Moderation, &'static str> {
        Ok(self.guarded(id, !is_admin, "set_admin"))
    }
    async fn set_suspended(&self, id: u32, suspended: bool) -> Result<Moderation, &'static str> {
        Ok(self.guarded(id, suspended, "set_suspended"))
    }
    async fn delete_user(&self, id: u32) -> Result<Moderation, &'static str> {
        Ok(self.guarded(id, true, "delete_user"))
    }
    async fn repositories_owned_by(&self, _: u32) -> Result<Vec<(u32, String)>, &'static str> {
        Ok(vec![(7, "rocket".to_owned())])
    }
    async fn organizations_solely_owned_by(&self, _: u32) -> Result<Vec<String>, &'static str> {
        Ok(if self.sole_owner { vec!["acme".to_owned(), "beta".to_owned()] } else { Vec::new() })
    }
    async fn list_organizations(&self, _: ListFilter<'_>) -> Result<Listing<&'static str>, &'static str> {
        Ok(Listing { items: vec!["acme"], total: 1 })
    }
    async fn list_repositories(&self, _: ListFilter<'_>) -> Result<Listing<u32>, &'static str> {
        Ok(Listing { items: Vec::new(), total: 0 })
    }
    async fn find_repository(&self, id: u32) -> Result<Option<u32>, &'static str> {
        Ok((id == 7).then_some(id))
    }
    async fn delete_repository(&self, id: u32) -> Result<bool, &'static str> {
        self.writes.lock().unwrap().push("delete_repository");
        Ok(id == 7)
    }
}

fn admin(id: u32) -> Actor<u32> {
    Actor { id, site_admin: true }
}

fn message<T: std::fmt::Debug>(result: Result<T, AccessError<&'static str>>) -> String {
    match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn only_site_administrators_administer() {
    let store = Store::new(&[ROOT]);
    let rules = Administration::new(&store);
    let user = Actor { id: ALICE, site_admin: false };
    assert!(matches!(ready(rules.users(&user, &ListQuery::default())), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.set_admin(&user, ALICE, true)), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.delete_repository(&user, 7, "alice/rocket")), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.user(&admin(ROOT), 9)), Err(AccessError::NotFound)));
    assert_eq!(*store.writes.lock().unwrap(), Vec::<&str>::new(), "refused actors never reach a write");
}

#[test]
fn administrators_cannot_lock_themselves_or_the_instance_out() {
    let store = Store::new(&[ROOT]);
    let rules = Administration::new(&store);
    assert_eq!(message(ready(rules.set_admin(&admin(ROOT), ROOT, false))), "you can't remove your own administrator rights");
    assert_eq!(message(ready(rules.set_suspended(&admin(ROOT), ROOT, true))), "you can't suspend yourself");
    assert_eq!(message(ready(rules.delete_user(&admin(ROOT), ROOT, "u1"))), "you can't delete your own account here");
    // Another administrator acting on the last active one is refused by the store.
    let refused = message(ready(rules.set_suspended(&admin(OTHER_ADMIN), ROOT, true)));
    assert_eq!(refused, "u1 is the last active administrator and can't be suspended; promote someone else first");
    assert_eq!(ready(rules.set_admin(&admin(ROOT), ROOT, true)).unwrap(), ROOT, "granting rights to oneself is harmless");
    let two = Store::new(&[ROOT, OTHER_ADMIN]);
    ready(Administration::new(&two).set_admin(&admin(ROOT), OTHER_ADMIN, false)).unwrap();
}

#[test]
fn deletions_need_exact_confirmation_and_no_orphaned_organisations() {
    let store = Store { sole_owner: true, ..Store::new(&[ROOT]) };
    let rules = Administration::new(&store);
    assert_eq!(message(ready(rules.delete_user(&admin(ROOT), ALICE, "U3"))), "type the username exactly to confirm");
    let orphaned = message(ready(rules.delete_user(&admin(ROOT), ALICE, " u3 ")));
    assert_eq!(orphaned, "u3 is the only owner of acme, beta; add another owner or delete them first");
    assert_eq!(*store.writes.lock().unwrap(), Vec::<&str>::new());
    let free = Store::new(&[ROOT]);
    let deleted = ready(Administration::new(&free).delete_user(&admin(ROOT), ALICE, "u3")).unwrap();
    assert_eq!((deleted.user, deleted.repositories), (ALICE, vec![(7, "rocket".to_owned())]));
    let refused = message(ready(rules.delete_repository(&admin(ROOT), 7, "alice/Rocket")));
    assert_eq!(refused, "type the repository's full name exactly to confirm");
    assert_eq!(ready(rules.delete_repository(&admin(ROOT), 7, " alice/rocket ")).unwrap(), 7);
}

#[test]
fn list_query_normalises_input() {
    let query = ListQuery { search: format!("  {}  ", "é".repeat(150)), page: 0 };
    assert_eq!(query.search().chars().count(), MAX_SEARCH_LEN);
    assert_eq!(query.page(), 1);
    assert_eq!(query.filter().offset, 0);
    assert_eq!(ListQuery { search: String::new(), page: 3 }.filter().offset, 60);
    let store = Store::new(&[ROOT]);
    let page = ready(Administration::new(&store).users(&admin(ROOT), &ListQuery { search: String::new(), page: 3 })).unwrap();
    assert_eq!((page.items, page.total, page.page, page.per_page), (vec![60], 61, 3, PAGE_SIZE));
}

#[test]
fn paging_bounds() {
    let query = ListQuery { search: String::new(), page: 2 };
    let paged = Paged::new(Listing { items: vec![1; 5], total: 35 }, &query);
    assert!(paged.has_previous());
    assert!(!paged.has_next());
    assert_eq!((paged.first_index(), paged.last_index()), (31, 35));

    let first = Paged::new(Listing { items: vec![1; 30], total: 35 }, &ListQuery::default());
    assert!(!first.has_previous());
    assert!(first.has_next());

    let empty = Paged::<u8>::new(Listing { items: Vec::new(), total: 0 }, &ListQuery::default());
    assert_eq!((empty.first_index(), empty.last_index()), (0, 0));
}

#[test]
fn settings_changes_are_described() {
    let fields = [("site name", "rbit", "Forge"), ("registration", "open", "open"), ("email digest hour", "07:00 UTC", "08:00 UTC")];
    assert_eq!(describe_changes(false, &fields), "site name: rbit → Forge; email digest hour: 07:00 UTC → 08:00 UTC");
    assert_eq!(describe_changes(true, &[("registration", "open", "open")]), "branding changed; ");
    assert_eq!(describe_changes(false, &[]), "");
}
