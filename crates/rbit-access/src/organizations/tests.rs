use std::{
    collections::BTreeMap,
    future::Future,
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use super::*;

/// The fakes never suspend, so one poll completes every rule.
pub(crate) fn ready<F: Future>(future: F) -> F::Output {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("in-memory port suspended"),
    }
}

pub(crate) const ORG: u32 = 10;
pub(crate) const OWNER: u32 = 1;
pub(crate) const MEMBER: u32 = 2;
pub(crate) const OUTSIDER: u32 = 3;

/// One organisation with an owner and a member; users are named `u{id}`.
#[derive(Debug, Default)]
pub(crate) struct Directory {
    pub(crate) roles: Mutex<BTreeMap<u32, OrgRole>>,
    pub(crate) repositories: i64,
    pub(crate) calls: Mutex<Vec<&'static str>>,
}

impl Directory {
    pub(crate) fn seeded() -> Self {
        let directory = Self::default();
        directory.roles.lock().unwrap().extend([(OWNER, OrgRole::Owner), (MEMBER, OrgRole::Member)]);
        directory
    }

    fn call(&self, name: &'static str) {
        self.calls.lock().unwrap().push(name);
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl MemberDirectory for Directory {
    type Error = &'static str;
    type Id = u32;

    async fn find_user(&self, name: &str) -> Result<Option<UserAccount<u32>>, &'static str> {
        Ok(name
            .strip_prefix('u')
            .and_then(|id| id.parse().ok())
            .filter(|id| *id != ORG)
            .map(|id| UserAccount { id, name: name.to_owned() }))
    }

    async fn role_of(&self, org: u32, user: u32) -> Result<Option<OrgRole>, &'static str> {
        assert_eq!(org, ORG);
        Ok(self.roles.lock().unwrap().get(&user).copied())
    }
}

/// Handles are lowercase ASCII; anything else reports the identity layer's "username" text.
#[derive(Debug)]
pub(crate) struct Handle;

impl std::str::FromStr for Handle {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.bytes().all(|b| b.is_ascii_lowercase()) && !value.is_empty() { Ok(Self) } else { Err("invalid username") }
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl OrganizationRepository for Directory {
    type Name = Handle;
    type Organization = ();
    type Member = ();
    type Membership = ();
    type BasePermission = u8;

    async fn create(&self, _: NewOrganization<Handle, u32>) -> Result<(), &'static str> {
        self.call("create");
        Ok(())
    }
    async fn find_by_name(&self, name: &str) -> Result<Option<()>, &'static str> {
        Ok((name == "acme").then_some(()))
    }
    async fn members(&self, _: u32) -> Result<Vec<()>, &'static str> {
        Ok(Vec::new())
    }
    async fn memberships_of(&self, _: u32) -> Result<Vec<()>, &'static str> {
        Ok(Vec::new())
    }
    async fn add_member(&self, _: u32, user: u32, role: OrgRole) -> Result<MemberAddition, &'static str> {
        let mut roles = self.roles.lock().unwrap();
        if roles.contains_key(&user) {
            return Ok(MemberAddition::AlreadyMember);
        }
        roles.insert(user, role);
        Ok(MemberAddition::Added)
    }
    async fn set_role(&self, _: u32, user: u32, role: OrgRole) -> Result<MembershipChange, &'static str> {
        let mut roles = self.roles.lock().unwrap();
        let owners = roles.values().filter(|role| role.is_owner()).count();
        match roles.get_mut(&user) {
            None => Ok(MembershipChange::NotMember),
            Some(current) if current.is_owner() && !role.is_owner() && owners == 1 => Ok(MembershipChange::LastOwner),
            Some(current) => {
                *current = role;
                Ok(MembershipChange::Done)
            }
        }
    }
    async fn remove_member(&self, _: u32, user: u32) -> Result<MembershipChange, &'static str> {
        let mut roles = self.roles.lock().unwrap();
        let owners = roles.values().filter(|role| role.is_owner()).count();
        match roles.get(&user) {
            None => Ok(MembershipChange::NotMember),
            Some(role) if role.is_owner() && owners == 1 => Ok(MembershipChange::LastOwner),
            Some(_) => {
                roles.remove(&user);
                Ok(MembershipChange::Done)
            }
        }
    }
    async fn update_profile(&self, _: u32, _: OrgProfileUpdate) -> Result<(), &'static str> {
        self.call("update_profile");
        Ok(())
    }
    async fn base_permission(&self, _: u32) -> Result<u8, &'static str> {
        Ok(2)
    }
    async fn set_base_permission(&self, _: u32, _: u8) -> Result<(), &'static str> {
        self.call("set_base_permission");
        Ok(())
    }
    async fn delete_if_empty(&self, _: u32) -> Result<bool, &'static str> {
        self.call("delete_if_empty");
        Ok(self.repositories == 0)
    }
    async fn count_repositories(&self, _: u32) -> Result<i64, &'static str> {
        Ok(self.repositories)
    }
}

pub(crate) fn actor(id: u32) -> Actor<u32> {
    Actor { id, site_admin: false }
}

const ACME: OrganizationRef<'static, u32> = OrganizationRef { id: ORG, name: "acme" };

fn message<T: std::fmt::Debug>(result: Result<T, AccessError<&'static str>>) -> String {
    match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn creation_policy_admits_admins_and_validates_before_writing() {
    let directory = Directory::seeded();
    let orgs = Organizations::new(&directory);
    let input = CreateOrganization { name: " acme ".into(), ..CreateOrganization::default() };
    assert!(matches!(ready(orgs.create(OrgCreationPolicy::AdminsOnly, actor(OUTSIDER), &input)), Err(AccessError::Forbidden)));
    let admin = Actor { id: OUTSIDER, site_admin: true };
    ready(orgs.create(OrgCreationPolicy::AdminsOnly, admin, &input)).unwrap();
    let bad = CreateOrganization { name: "Acme!".into(), ..CreateOrganization::default() };
    assert_eq!(message(ready(orgs.create(OrgCreationPolicy::AnyUser, actor(OUTSIDER), &bad))), "invalid organization name");
    let long = CreateOrganization { display_name: "x".repeat(101), ..input };
    assert_eq!(
        message(ready(orgs.create(OrgCreationPolicy::AnyUser, actor(OUTSIDER), &long))),
        "display name must be at most 100 characters"
    );
    assert_eq!(*directory.calls.lock().unwrap(), ["create"]);
}

#[test]
fn only_owners_and_site_admins_manage_and_nonmembers_see_nothing() {
    let directory = Directory::seeded();
    let orgs = Organizations::new(&directory);
    for (actor, expected) in [(actor(OWNER), Ok(())), (Actor { id: OUTSIDER, site_admin: true }, Ok(()))] {
        assert_eq!(ready(orgs.require_manager(actor, ORG)).map_err(|_| ()), expected);
    }
    assert!(matches!(ready(orgs.set_base_permission(actor(MEMBER), ORG, 0)), Err(AccessError::Forbidden)));
    assert!(matches!(ready(orgs.update_profile(actor(OUTSIDER), ORG, &UpdateOrganization::default())), Err(AccessError::NotFound)));
    assert!(directory.calls.lock().unwrap().is_empty(), "refused actors never reach a write");
    let viewer = ready(orgs.viewer(ORG, Some(actor(MEMBER)))).unwrap();
    assert_eq!(viewer, ViewerRole { role: Some(OrgRole::Member), can_manage: false });
    assert_eq!(ready(orgs.viewer(ORG, None)).unwrap(), ViewerRole { role: None, can_manage: false });
    let admin = ready(orgs.viewer(ORG, Some(Actor { id: OUTSIDER, site_admin: true }))).unwrap();
    assert_eq!(admin, ViewerRole { role: None, can_manage: true });
}

#[test]
fn membership_changes_explain_refusals_and_keep_the_last_owner() {
    let directory = Directory::seeded();
    let orgs = Organizations::new(&directory);
    assert_eq!(message(ready(orgs.add_member(actor(OWNER), ACME, "  ", OrgRole::Member))), "enter a username");
    assert_eq!(message(ready(orgs.add_member(actor(OWNER), ACME, "u10", OrgRole::Member))), "there is no user called u10");
    assert_eq!(message(ready(orgs.add_member(actor(OWNER), ACME, "u2", OrgRole::Owner))), "u2 is already a member");
    assert_eq!(ready(orgs.add_member(actor(OWNER), ACME, " u3 ", OrgRole::Member)).unwrap().id, OUTSIDER);
    assert_eq!(message(ready(orgs.change_role(actor(OWNER), ACME, "u1", OrgRole::Member))), "acme must keep at least one owner");
    assert_eq!(message(ready(orgs.remove_member(actor(OWNER), ACME, "u4"))), "u4 is not a member of acme");
    let last = message(ready(orgs.leave(actor(OWNER), ACME)));
    assert_eq!(last, "you are the last owner of acme; make another member an owner before leaving");
    assert!(matches!(ready(orgs.leave(actor(4), ACME)), Err(AccessError::NotFound)));
    ready(orgs.leave(actor(MEMBER), ACME)).unwrap();
}

#[test]
fn deletion_needs_the_name_and_no_repositories() {
    let directory = Directory { repositories: 1, ..Directory::seeded() };
    let orgs = Organizations::new(&directory);
    assert_eq!(message(ready(orgs.delete(actor(OWNER), ACME, "other"))), "type acme to confirm");
    assert!(directory.calls.lock().unwrap().is_empty());
    let refused = message(ready(orgs.delete(actor(OWNER), ACME, " ACME ")));
    assert_eq!(refused, "acme still owns 1 repository; delete them or transfer them to another owner first");
    let directory = Directory::seeded();
    ready(Organizations::new(&directory).delete(actor(OWNER), ACME, "acme")).unwrap();
}

#[test]
fn website_and_profile_rules_apply_before_the_update() {
    let directory = Directory::seeded();
    let orgs = Organizations::new(&directory);
    let input = UpdateOrganization { website: "javascript:alert(1)".into(), ..UpdateOrganization::default() };
    assert_eq!(message(ready(orgs.update_profile(actor(OWNER), ORG, &input))), "website must be a link starting with https:// or http://");
    let input = UpdateOrganization { website: "https://acme.example".into(), ..UpdateOrganization::default() };
    ready(orgs.update_profile(actor(OWNER), ORG, &input)).unwrap();
    assert_eq!(*directory.calls.lock().unwrap(), ["update_profile"]);
    assert!(can_create_organization(OrgCreationPolicy::AnyUser, false));
}
