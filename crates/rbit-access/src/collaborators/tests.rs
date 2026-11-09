use std::{collections::BTreeMap, sync::Mutex};

use super::*;
use crate::{
    organizations::{AccessError, UserAccount, tests::ready},
    teams::Unique,
};

const OWNER: u32 = 1;
const ADMIN: u32 = 2;
const BOB: u32 = 3;
const SUSPENDED: u32 = 4;

/// Users are named `u{id}`; id 10 is an organisation. Bob collaborates with `read`.
#[derive(Debug)]
struct Store {
    entries: Mutex<BTreeMap<u32, &'static str>>,
}

impl Store {
    fn new() -> Self {
        Self { entries: Mutex::new(BTreeMap::from([(BOB, "read")])) }
    }
}

#[expect(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl CollaboratorRepository for Store {
    type Error = &'static str;
    type Id = u32;
    type Repo = ();
    type Permission = &'static str;
    type Collaborator = u32;

    async fn find_user(&self, name: &str) -> Result<Option<UserAccount<u32>>, &'static str> {
        Ok(name.strip_prefix('u').and_then(|id| id.parse().ok()).filter(|id| *id != 10).map(|id| UserAccount { id, name: name.to_owned() }))
    }
    async fn is_suspended(&self, user: u32) -> Result<bool, &'static str> {
        Ok(user == SUSPENDED)
    }
    async fn list(&self, (): ()) -> Result<Vec<u32>, &'static str> {
        Ok(self.entries.lock().unwrap().keys().copied().collect())
    }
    async fn permission(&self, (): (), user: u32) -> Result<Option<&'static str>, &'static str> {
        Ok(self.entries.lock().unwrap().get(&user).copied())
    }
    async fn add(&self, (): (), user: u32, permission: &'static str, _: u32) -> Result<Unique<()>, &'static str> {
        let mut entries = self.entries.lock().unwrap();
        if entries.contains_key(&user) {
            return Ok(Unique::Taken);
        }
        entries.insert(user, permission);
        Ok(Unique::Done(()))
    }
    async fn set_permission(&self, (): (), user: u32, permission: &'static str) -> Result<bool, &'static str> {
        Ok(self.entries.lock().unwrap().get_mut(&user).map(|entry| *entry = permission).is_some())
    }
    async fn remove(&self, (): (), user: u32) -> Result<bool, &'static str> {
        Ok(self.entries.lock().unwrap().remove(&user).is_some())
    }
}

const ADMINS: CollaboratorScope<(), u32> = CollaboratorScope { repo: (), owner: OWNER, can_admin: true };
const OTHERS: CollaboratorScope<(), u32> = CollaboratorScope { can_admin: false, ..ADMINS };

fn message<T: std::fmt::Debug>(result: Result<T, AccessError<&'static str>>) -> String {
    match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn administrators_manage_and_refusals_name_the_reason() {
    let store = Store::new();
    let rules = Collaborators::new(&store);
    assert!(matches!(ready(rules.list(OTHERS)), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.add(OTHERS, BOB, "u5", "write")), Err(AccessError::Forbidden)));
    assert_eq!(message(ready(rules.add(ADMINS, ADMIN, " ", "write"))), "enter a username");
    assert_eq!(message(ready(rules.add(ADMINS, ADMIN, "u10", "write"))), "there is no user called u10");
    assert_eq!(message(ready(rules.add(ADMINS, ADMIN, "u1", "write"))), "u1 owns this repository");
    assert_eq!(message(ready(rules.add(ADMINS, ADMIN, "u4", "write"))), "u4 is suspended and cannot be added");
    assert_eq!(message(ready(rules.add(ADMINS, ADMIN, "u3", "write"))), "u3 is already a collaborator");
    assert_eq!(ready(rules.add(ADMINS, ADMIN, " u5 ", "write")).unwrap().id, 5);
    assert_eq!(message(ready(rules.change(ADMINS, "u6", "admin"))), "u6 is not a collaborator");
    assert_eq!(ready(rules.change(ADMINS, "u3", "admin")).unwrap().1, "read");
}

#[test]
fn collaborators_may_leave_and_set_is_idempotent() {
    let store = Store::new();
    let rules = Collaborators::new(&store);
    assert!(matches!(ready(rules.remove(OTHERS, 5, "u3")), Err(AccessError::Forbidden)));
    assert_eq!(ready(rules.remove(OTHERS, BOB, "u3")).unwrap().id, BOB);
    assert_eq!(message(ready(rules.remove(ADMINS, ADMIN, "u3"))), "u3 is not a collaborator");
    assert!(matches!(ready(rules.set(ADMINS, ADMIN, "u3", "read")).unwrap(), SetOutcome::Added(user) if user.id == BOB));
    assert_eq!(ready(rules.set(ADMINS, ADMIN, "u3", "read")).unwrap(), SetOutcome::Unchanged);
    assert!(matches!(ready(rules.set(ADMINS, ADMIN, "u3", "write")).unwrap(), SetOutcome::Changed { from: "read", .. }));
    assert!(matches!(ready(rules.set(OTHERS, BOB, "u3", "admin")), Err(AccessError::Forbidden)));
}
