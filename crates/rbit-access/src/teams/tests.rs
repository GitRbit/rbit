use std::{future::Future, sync::Mutex};

use super::*;
use crate::{
    OrgRole,
    organizations::{
        AccessError, Actor, MemberDirectory, OrganizationRef, UserAccount,
        tests::{Directory, MEMBER, ORG, OUTSIDER, OWNER, actor, ready},
    },
};

const ACME: OrganizationRef<'static, u32> = OrganizationRef { id: ORG, name: "acme" };
const TEAM: u8 = 7;

/// One team, `core`, whose members are tracked; repository `acme/rocket` is id 1.
#[derive(Debug)]
struct Store {
    directory: Directory,
    members: Mutex<Vec<u32>>,
    grants: Mutex<Vec<(u8, &'static str)>>,
}

impl Store {
    fn new() -> Self {
        Self { directory: Directory::seeded(), members: Mutex::new(vec![MEMBER]), grants: Mutex::new(Vec::new()) }
    }
}

impl MemberDirectory for Store {
    type Error = &'static str;
    type Id = u32;
    fn find_user(&self, name: &str) -> impl Future<Output = Result<Option<UserAccount<u32>>, &'static str>> + Send {
        self.directory.find_user(name)
    }
    fn role_of(&self, org: u32, user: u32) -> impl Future<Output = Result<Option<OrgRole>, &'static str>> + Send {
        self.directory.role_of(org, user)
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl TeamRepository for Store {
    type TeamId = u8;
    type TeamName = String;
    type Team = u8;
    type Summary = ();
    type Member = u32;
    type RepoId = u8;
    type Permission = &'static str;
    type GrantedRepository = ();

    fn team_id(team: &u8) -> u8 {
        *team
    }
    async fn create(&self, _: u32, name: &String, _: Option<&str>) -> Result<Unique<u8>, &'static str> {
        Ok(if name.eq_ignore_ascii_case("core") { Unique::Taken } else { Unique::Done(TEAM + 1) })
    }
    async fn find_by_name(&self, _: u32, name: &str) -> Result<Option<u8>, &'static str> {
        Ok((name == "core").then_some(TEAM))
    }
    async fn list(&self, _: u32) -> Result<Vec<()>, &'static str> {
        Ok(vec![()])
    }
    async fn update(&self, _: u8, _: &String, _: Option<&str>) -> Result<Unique<()>, &'static str> {
        Ok(Unique::Done(()))
    }
    async fn delete(&self, _: u8) -> Result<(), &'static str> {
        Ok(())
    }
    async fn members(&self, _: u8) -> Result<Vec<u32>, &'static str> {
        Ok(self.members.lock().unwrap().clone())
    }
    async fn is_member(&self, _: u8, user: u32) -> Result<bool, &'static str> {
        Ok(self.members.lock().unwrap().contains(&user))
    }
    async fn add_member(&self, _: u8, user: u32) -> Result<Unique<TeamChange>, &'static str> {
        if self.directory.roles.lock().unwrap().get(&user).is_none() {
            return Ok(Unique::Done(TeamChange::OutsideOrganization));
        }
        let mut members = self.members.lock().unwrap();
        if members.contains(&user) {
            return Ok(Unique::Taken);
        }
        members.push(user);
        Ok(Unique::Done(TeamChange::Done))
    }
    async fn remove_member(&self, _: u8, user: u32) -> Result<bool, &'static str> {
        let mut members = self.members.lock().unwrap();
        let before = members.len();
        members.retain(|member| *member != user);
        Ok(members.len() != before)
    }
    async fn repositories(&self, _: u8, _: Option<u32>, _: bool) -> Result<Vec<()>, &'static str> {
        Ok(Vec::new())
    }
    async fn find_repository(&self, owner: &str, name: &str) -> Result<Option<RepositoryRef<u8>>, &'static str> {
        Ok((owner == "acme" && name == "rocket").then(|| RepositoryRef { id: 1, full_name: "acme/rocket".to_owned() }))
    }
    async fn set_repository(&self, team: u8, _: u8, permission: &'static str) -> Result<TeamChange, &'static str> {
        self.grants.lock().unwrap().push((team, permission));
        Ok(TeamChange::Done)
    }
    async fn remove_repository(&self, _: u8, _: u8) -> Result<bool, &'static str> {
        Ok(!self.grants.lock().unwrap().is_empty())
    }
}

fn message<T: std::fmt::Debug>(result: Result<T, AccessError<&'static str>>) -> String {
    match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

#[test]
fn teams_are_visible_to_members_and_admins_only() {
    let store = Store::new();
    let teams = Teams::new(&store);
    assert!(ready(teams.can_view(Some(actor(MEMBER)), ORG)).unwrap());
    assert!(ready(teams.can_view(Some(Actor { id: OUTSIDER, site_admin: true }), ORG)).unwrap());
    assert!(!ready(teams.can_view(Some(actor(OUTSIDER)), ORG)).unwrap());
    assert!(matches!(ready(teams.list(None, ORG)), Err(AccessError::NotFound)));
    let opened = ready(teams.open(Some(actor(MEMBER)), ORG, "core")).unwrap();
    assert!(opened.is_member && !opened.can_manage);
    let opened = ready(teams.open(Some(actor(OWNER)), ORG, "core")).unwrap();
    assert!(!opened.is_member && opened.can_manage);
}

#[test]
fn owners_manage_with_validated_unique_names() {
    let store = Store::new();
    let teams = Teams::new(&store);
    let input = |name: &str, description: &str| TeamInput { name: name.into(), description: description.into() };
    assert!(matches!(ready(teams.create(actor(MEMBER), ORG, &input("ops", ""))), Err(AccessError::Forbidden)));
    assert!(matches!(ready(teams.delete(actor(OUTSIDER), ORG, TEAM)), Err(AccessError::NotFound)));
    assert_eq!(message(ready(teams.create(actor(OWNER), ORG, &input("Core", "")))), "a team with this name already exists");
    assert_eq!(message(ready(teams.create(actor(OWNER), ORG, &input("ops", "a\nb")))), "description must be a single line of text");
    let long = "x".repeat(351);
    assert_eq!(message(ready(teams.create(actor(OWNER), ORG, &input("ops", &long)))), "description must be at most 350 characters");
    assert_eq!(ready(teams.update(actor(OWNER), ORG, TEAM, &input(" ops ", ""))).unwrap(), "ops");
}

#[test]
fn members_must_belong_to_the_organisation_and_may_leave_themselves() {
    let store = Store::new();
    let teams = Teams::new(&store);
    let refused = message(ready(teams.add_member(actor(OWNER), ACME, TEAM, "u3")));
    assert_eq!(refused, "u3 is not a member of acme; add them to the organization first");
    assert_eq!(message(ready(teams.add_member(actor(OWNER), ACME, TEAM, "u2"))), "u2 is already on this team");
    assert_eq!(ready(teams.ensure_member(actor(OWNER), ACME, TEAM, "u2")).unwrap(), None);
    assert_eq!(ready(teams.ensure_member(actor(OWNER), ACME, TEAM, " u1 ")).unwrap().map(|user| user.id), Some(OWNER));
    assert!(matches!(ready(teams.remove_member(actor(MEMBER), ORG, TEAM, "u1")), Err(AccessError::Forbidden)));
    assert_eq!(ready(teams.remove_member(actor(MEMBER), ORG, TEAM, "u2")).unwrap().id, MEMBER);
    assert_eq!(message(ready(teams.remove_member(actor(OWNER), ORG, TEAM, "u2"))), "u2 is not on this team");
}

#[test]
fn grants_only_target_the_organisations_own_repositories() {
    let store = Store::new();
    let teams = Teams::new(&store);
    assert_eq!(
        message(ready(teams.set_repository(actor(OWNER), ACME, TEAM, "other/rocket", "write"))),
        "acme has no repository called other/rocket"
    );
    assert_eq!(message(ready(teams.set_repository(actor(OWNER), ACME, TEAM, "acme/", "write"))), "enter a repository name");
    assert_eq!(message(ready(teams.remove_repository(actor(OWNER), ACME, TEAM, "rocket"))), "this team has no access to acme/rocket");
    assert_eq!(ready(teams.set_repository(actor(OWNER), ACME, TEAM, " ACME/rocket ", "write")).unwrap(), "acme/rocket");
    assert_eq!(*store.grants.lock().unwrap(), [(TEAM, "write")]);
    assert!(matches!(ready(teams.set_repository(actor(MEMBER), ACME, TEAM, "rocket", "admin")), Err(AccessError::Forbidden)));
    assert_eq!(ready(teams.remove_repository(actor(OWNER), ACME, TEAM, "acme/rocket")).unwrap(), "acme/rocket");
}
