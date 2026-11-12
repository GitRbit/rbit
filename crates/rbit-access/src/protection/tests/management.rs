use std::{future::Future, sync::Mutex};

use super::super::{application::required_contexts, *};
use crate::{
    RepoAccess,
    organizations::{AccessError, tests::ready},
    protection::application::distinct,
    teams::Unique,
};

/// A context that refuses blanks and more than ten characters.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Context(String);

impl std::str::FromStr for Context {
    type Err = &'static str;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 10 { Err("bad context") } else { Ok(Self(value.to_owned())) }
    }
}

type Rule = ProtectionRule<u32, u32, Context>;

/// Repository 1 with one stored rule on `main` naming team 7; user 5 is on team 7.
#[derive(Debug, Default)]
struct Store {
    saved: Mutex<Vec<String>>,
}

#[expect(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl RuleRepository for Store {
    type Error = &'static str;
    type Repo = u32;
    type User = u32;
    type Team = u32;
    type Context = Context;
    type Record = Rule;

    fn rule(record: &Rule) -> &Rule {
        record
    }
    fn into_rule(record: Rule) -> Rule {
        record
    }
    async fn list(&self, _: u32) -> Result<Vec<Rule>, &'static str> {
        let settings = RuleSettings { restrict_pushers: true, ..RuleSettings::default() };
        let pattern = "main".parse().unwrap();
        Ok(vec![ProtectionRule { pattern, settings, allowed_users: vec![], allowed_teams: vec![7], required_contexts: vec![] }])
    }
    async fn get(&self, repo: u32, id: ProtectionId) -> Result<Option<Rule>, &'static str> {
        Ok(if id == ProtectionId(1) { self.list(repo).await?.pop() } else { None })
    }
    async fn teams_of(&self, _: u32, user: u32) -> Result<Vec<u32>, &'static str> {
        Ok(if user == 5 { vec![7] } else { vec![] })
    }
    async fn create(&self, _: u32, input: &StoredInput<Self>, _: u32) -> Result<Unique<RuleChange>, &'static str> {
        if input.pattern.as_str() == "main" {
            return Ok(Unique::Taken);
        }
        self.saved.lock().unwrap().push(input.pattern.to_string());
        Ok(Unique::Done(if input.teams.contains(&9) { RuleChange::TeamOutsideOrganization } else { RuleChange::Saved(ProtectionId(2)) }))
    }
    async fn update(&self, _: u32, id: ProtectionId, _: &StoredInput<Self>) -> Result<Unique<RuleChange>, &'static str> {
        Ok(Unique::Done(RuleChange::Saved(id)))
    }
    async fn delete(&self, _: u32, id: ProtectionId) -> Result<bool, &'static str> {
        Ok(id == ProtectionId(1))
    }
}

#[derive(Debug)]
struct Directory;

#[expect(clippy::unused_async_trait_impl, reason = "in-memory fakes complete on first poll")]
impl RuleDirectory for Directory {
    type Error = &'static str;
    type User = u32;
    type Team = u32;
    type OwnerTeam = &'static str;

    async fn writer(&self, name: &str) -> Result<Writer<u32>, &'static str> {
        Ok(match name {
            "writer" => Writer::Writer(5),
            "Reader" | "reader" => Writer::WithoutWrite("Reader".to_owned()),
            _ => Writer::Unknown,
        })
    }
    async fn team_named(&self, name: &str) -> Result<Option<u32>, &'static str> {
        Ok(match name {
            "core" => Some(7),
            "gone" => Some(9),
            _ => None,
        })
    }
    fn owner_teams(&self) -> impl Future<Output = Result<Vec<&'static str>, &'static str>> + Send {
        std::future::ready(Ok(vec!["core"]))
    }
}

const ORG: RuleScope<'static, u32> = RuleScope { repo: 1, owner: "acme", organization: true, can_admin: true };

fn message<T: std::fmt::Debug>(result: Result<T, AccessError<&'static str>>) -> String {
    match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

fn draft<'a>(pattern: &'a str, settings: RuleSettings, users: &'a [String], teams: &'a [String]) -> RuleDraft<'a> {
    RuleDraft { pattern, settings, users, teams, contexts: &[] }
}

#[test]
fn only_administrators_manage_and_inputs_are_validated_before_writing() {
    let (store, directory) = (Store::default(), Directory);
    let rules = RuleManagement::new(&store, &directory);
    let restricted = RuleSettings { restrict_pushers: true, ..RuleSettings::default() };
    let member = RuleScope { can_admin: false, ..ORG };
    assert!(matches!(ready(rules.list(member)), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.create(member, 1, draft("dev", restricted, &[], &[]))), Err(AccessError::Forbidden)));
    assert!(matches!(ready(rules.get(ORG, ProtectionId(3))), Err(AccessError::NotFound)));
    assert_eq!(message(ready(rules.create(ORG, 1, draft(" ", restricted, &[], &[])))), "enter a branch name or pattern");
    let users = ["nobody".to_owned()];
    assert_eq!(message(ready(rules.create(ORG, 1, draft("dev", restricted, &users, &[])))), "there is no user called nobody");
    let users = ["reader".to_owned()];
    let refused = message(ready(rules.create(ORG, 1, draft("dev", restricted, &users, &[]))));
    assert_eq!(refused, "Reader does not have write access to this repository");
    let teams = ["ghosts".to_owned()];
    assert_eq!(message(ready(rules.create(ORG, 1, draft("dev", restricted, &[], &teams)))), "acme has no team called ghosts");
    let personal = RuleScope { organization: false, ..ORG };
    let teams = ["core".to_owned()];
    let refused = message(ready(rules.create(personal, 1, draft("dev", restricted, &[], &teams))));
    assert_eq!(refused, "only repositories of organizations can allow teams");
    assert_eq!(ready(rules.owner_teams(personal)).unwrap(), Vec::<&str>::new());
    assert_eq!(ready(rules.owner_teams(ORG)).unwrap(), ["core"]);
    let many: Vec<String> = (0..=MAX_ALLOWED).map(|n| format!("u{n}")).collect();
    assert_eq!(
        message(ready(rules.create(ORG, 1, draft("dev", restricted, &many, &[])))),
        "a rule can allow at most 50 users and 50 teams"
    );
    assert!(store.saved.lock().unwrap().is_empty(), "nothing invalid reaches the store");
}

#[test]
fn store_outcomes_become_the_same_messages() {
    let (store, directory) = (Store::default(), Directory);
    let rules = RuleManagement::new(&store, &directory);
    let restricted = RuleSettings { restrict_pushers: true, ..RuleSettings::default() };
    assert_eq!(message(ready(rules.create(ORG, 1, draft("main", restricted, &[], &[])))), "a rule for this pattern already exists");
    let teams = ["gone".to_owned()];
    assert_eq!(message(ready(rules.create(ORG, 1, draft("dev", restricted, &[], &teams)))), "allowed teams must be teams of acme");
    let users = [" writer ".to_owned(), "writer".to_owned()];
    let (id, input) = ready(rules.create(ORG, 1, draft("dev", restricted, &users, &[]))).unwrap();
    assert_eq!((id, input.users), (ProtectionId(2), vec![5]));
    let unrestricted = draft("dev", RuleSettings::default(), &users, &teams);
    assert!(ready(rules.create(ORG, 1, unrestricted)).unwrap().1.users.is_empty(), "lists are dropped without the flag");
    let (current, _) = ready(rules.update(ORG, ProtectionId(1), draft("dev", restricted, &[], &[]))).unwrap();
    assert_eq!(current.pattern.as_str(), "main");
    assert!(matches!(ready(rules.delete(ORG, ProtectionId(3))), Err(AccessError::NotFound)));
}

#[test]
fn required_contexts_are_validated() {
    let contexts = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
    let parsed: Vec<Context> = required_contexts::<_, ()>(&contexts(&["ci/build", " ", "lint: all", "ci/build"])).unwrap();
    let names: Vec<_> = parsed.iter().map(|context| context.0.as_str()).collect();
    assert_eq!(names, ["ci/build", "lint: all"], "in order, distinct, spaces allowed");
    let message = |result: Result<Vec<Context>, AccessError<()>>| match result {
        Err(AccessError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    };
    assert!(message(required_contexts(&contexts(&["", "  "]))).contains("at least one"));
    let many: Vec<String> = (0..=MAX_REQUIRED_CONTEXTS).map(|n| format!("c{n}")).collect();
    assert!(message(required_contexts(&many)).contains("at most"));
    assert_eq!(message(required_contexts(&["x".repeat(11)])), "required check: bad context");
}

#[test]
fn summaries_and_names() {
    assert_eq!(summary(&RuleSettings::default()), "no force pushes, no deletion");
    let all = RuleSettings {
        require_pull_request: true,
        required_approvals: 2,
        restrict_pushers: true,
        allow_admin_bypass: true,
        ..RuleSettings::default()
    };
    assert_eq!(summary(&all), "no force pushes, no deletion, 2 approvals, restricted, admins bypass");
    let none = RuleSettings { deny_force_push: false, deny_deletion: false, ..RuleSettings::default() };
    assert_eq!(summary(&none), "no restrictions");
    let names = ["a".to_owned(), " b ".to_owned(), String::new(), "a".to_owned()];
    assert_eq!(distinct(&names), ["a", "b"]);
    assert_eq!(summary(&RuleSettings { require_status_checks: true, ..none }), "status checks");
}

#[test]
fn the_gate_loads_teams_only_when_a_rule_names_them() {
    let store = Store::default();
    let gate = ProtectionGate::new(&store);
    let principal = |user, access| Principal { user: Some(user), deploy_key: false, access };
    let update = [BranchChange::update("main")];
    let member = ready(gate.check_push(1, principal(5, RepoAccess::Write), &update)).unwrap();
    assert!(!member.verdict.is_refused(), "team 7 may push");
    let outsider = ready(gate.check_push(1, principal(6, RepoAccess::Write), &update)).unwrap();
    assert_eq!(outsider.verdict.refusals[0].reason, reasons::RESTRICTED);
    let key = Principal { deploy_key: true, ..principal(5, RepoAccess::Write) };
    assert_eq!(ready(gate.check_push(1, key, &update)).unwrap().verdict.refusals[0].reason, reasons::RESTRICTED);
    assert_eq!(ready(gate.branch_update_refusal(1, 6, RepoAccess::Write, "main")).unwrap().as_deref(), Some(reasons::RESTRICTED));
    assert!(!ready(gate.may_delete(1, 5, RepoAccess::Write, "main")).unwrap());
    assert!(ready(gate.denies_force_push(1, 5, RepoAccess::Write, "main")).unwrap(), "a rule keeps `main` from being rewritten");
    assert!(!ready(gate.denies_force_push(1, 5, RepoAccess::Write, "dev")).unwrap(), "no rule covers `dev`");
    assert!(ready(gate.merge(1, 5, RepoAccess::Write, "main")).unwrap().merger_may_push);
    assert!(!ready(gate.merge(1, 5, RepoAccess::Write, "dev")).unwrap().is_protected());
}
