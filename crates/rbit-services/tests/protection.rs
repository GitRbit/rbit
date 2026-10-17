//! Branch protection rules: who may manage them, validation, the audit trail, and the
//! decisions the push gate and pull request merges get, against real stores and a bare
//! repository filled with the `git` CLI.

use std::{path::Path, process::Command, sync::Arc};

use rbit_core::{
    Config, OrgRole, RefUpdate, RepoAccess, User, Visibility,
    membership::{BasePermission, Permission},
    protection::{ProtectionId, RuleSettings, reasons},
};
use rbit_services::{
    CreateRepository, RepoContext, ServiceError, Services,
    audit::{AuditAction, AuditEvent},
    orgs::CreateOrganization,
    protection::{MergeRules, ProtectionInput, PushActor, PushVerdict},
    teams::TeamInput,
};
use rbit_storage::{Database, KvStore, NewUser};

const ID: &str = "1111111111111111111111111111111111111111";

struct Fixture {
    dir: tempfile::TempDir,
    services: Services,
    /// Owns `acme`.
    olga: User,
    /// On team `core`, which may write to `acme/rocket`.
    bob: User,
    /// A collaborator with write access.
    carol: User,
    /// A collaborator with read access.
    rita: User,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let database = Database::open(&dir.path().join("rbit.db")).await.unwrap();
        let kv = KvStore::open(&dir.path().join("rbit.redb")).unwrap();
        let mut config = Config::default();
        config.storage.data_dir = dir.path().to_path_buf();
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
        let user = |name: &'static str| {
            let database = database.clone();
            async move {
                let new = NewUser {
                    name: name.parse().unwrap(),
                    email: format!("{name}@example.com").parse().unwrap(),
                    display_name: None,
                    password_hash: "not-a-real-hash".to_owned(),
                    is_admin: false,
                };
                database.accounts().create_user(new).await.unwrap()
            }
        };
        let (olga, bob, carol, rita) = (user("olga").await, user("bob").await, user("carol").await, user("rita").await);
        let acme = services.orgs.create(&olga, CreateOrganization { name: "acme".into(), ..CreateOrganization::default() }).await.unwrap();
        services.orgs.add_member(&olga, &acme, "bob", OrgRole::Member).await.unwrap();
        services.orgs.set_base_permission(&olga, &acme, BasePermission::None).await.unwrap();
        let input = CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Private };
        services.repositories.create_in(&olga, "acme", input).await.unwrap();
        for team in ["core", "docs"] {
            services.teams.create(&olga, &acme, TeamInput { name: team.into(), description: String::new() }).await.unwrap();
        }
        let core = services.teams.open(Some(&olga), &acme, "core").await.unwrap();
        services.teams.add_member(&olga, &core, "bob").await.unwrap();
        services.teams.set_repository(&olga, &core, "rocket", Permission::Write).await.unwrap();
        let fixture = Self { dir, services, olga, bob, carol, rita };
        let rocket = fixture.repo(&fixture.olga).await;
        fixture.services.collaborators.add(&fixture.olga, &rocket, "carol", Permission::Write).await.unwrap();
        fixture.services.collaborators.add(&fixture.olga, &rocket, "rita", Permission::Read).await.unwrap();
        fixture
    }

    async fn repo(&self, viewer: &User) -> RepoContext {
        self.services.repositories.open(Some(viewer), "acme", "rocket").await.unwrap()
    }

    /// Pushes `main`, `release/1.0` and `feature` into `acme/rocket` with the git CLI.
    async fn branches(&self) {
        let repo = self.repo(&self.olga).await;
        let work = self.dir.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(work.join("README.md"), "hello\n").unwrap();
        git(&work, &["init", "--quiet", "--initial-branch=main"]);
        git(&work, &["add", "README.md"]);
        git(&work, &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "--quiet", "-m", "initial"]);
        git(&work, &["branch", "release/1.0"]);
        git(&work, &["branch", "feature"]);
        git(&work, &["push", "--quiet", repo.disk_path.to_str().unwrap(), "main", "release/1.0", "feature"]);
    }

    /// What the push gate decides for `user` pushing `updates` to `acme/rocket`.
    async fn push(&self, user: &User, updates: Vec<RefUpdate>) -> PushVerdict {
        let repo = self.repo(user).await;
        let actor = PushActor { user: Some(user), deploy_key: false, access: repo.access };
        self.services.protection.check_push(&repo, &actor, &updates).await.unwrap()
    }

    async fn audit(&self, action: AuditAction) -> Vec<AuditEvent> {
        let events = self.services.audit.recent_for(&self.olga, 50).await.unwrap();
        events.into_iter().filter(|event| event.action == action).collect()
    }
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn input(pattern: &str, settings: RuleSettings) -> ProtectionInput {
    ProtectionInput { pattern: pattern.to_owned(), settings, users: Vec::new(), teams: Vec::new(), contexts: Vec::new() }
}

fn restricted() -> RuleSettings {
    RuleSettings { restrict_pushers: true, ..RuleSettings::default() }
}

fn invalid(result: Result<impl std::fmt::Debug, ServiceError>) -> String {
    match result {
        Err(ServiceError::Invalid(message)) => message,
        other => panic!("expected a validation error, got {other:?}"),
    }
}

fn update(name: &str, before: Option<&str>, after: Option<&str>) -> RefUpdate {
    RefUpdate { name: name.to_owned(), before: before.map(str::to_owned), after: after.map(str::to_owned) }
}

#[tokio::test]
async fn administrators_manage_rules_and_changes_are_audited() {
    let fixture = Fixture::new().await;
    let protection = &fixture.services.protection;
    let admin = fixture.repo(&fixture.olga).await;
    let writer = fixture.repo(&fixture.bob).await;
    assert_eq!(writer.access, RepoAccess::Write);

    assert!(matches!(protection.list(&writer).await, Err(ServiceError::Forbidden)));
    assert!(matches!(
        protection.create(&fixture.bob, &writer, &input("main", RuleSettings::default())).await,
        Err(ServiceError::Forbidden)
    ));
    assert!(matches!(protection.teams(&writer).await, Err(ServiceError::Forbidden)));

    let mut rule_input = input("main", restricted());
    rule_input.users = vec!["carol".to_owned(), " carol ".to_owned(), String::new()];
    rule_input.teams = vec!["CORE".to_owned()];
    let rule = protection.create(&fixture.olga, &admin, &rule_input).await.unwrap();
    assert_eq!(rule.rule.pattern.as_str(), "main");
    assert_eq!(rule.users.iter().map(|user| user.name.as_str()).collect::<Vec<_>>(), ["carol"]);
    assert_eq!(rule.teams.iter().map(|team| team.name.as_str()).collect::<Vec<_>>(), ["core"]);
    assert_eq!(protection.list(&admin).await.unwrap().len(), 1);
    let teams: Vec<_> = protection.teams(&admin).await.unwrap().iter().map(|team| team.name.to_string()).collect();
    assert_eq!(teams, ["core", "docs"]);

    // Validation.
    let cases = [
        (input("a b", RuleSettings::default()), "the pattern may not contain spaces or control characters"),
        (input("main", RuleSettings::default()), "a rule for this pattern already exists"),
        (
            input("x", RuleSettings { required_approvals: 1, ..RuleSettings::default() }),
            "required approvals need \"require a pull request\"",
        ),
        (ProtectionInput { users: vec!["rita".into()], ..input("x", restricted()) }, "rita does not have write access to this repository"),
        (ProtectionInput { users: vec!["nobody".into()], ..input("x", restricted()) }, "there is no user called nobody"),
        (ProtectionInput { users: vec!["acme".into()], ..input("x", restricted()) }, "there is no user called acme"),
        (ProtectionInput { teams: vec!["ghosts".into()], ..input("x", restricted()) }, "acme has no team called ghosts"),
        (
            ProtectionInput { users: (0..51).map(|n| format!("user{n}")).collect(), ..input("x", restricted()) },
            "a rule can allow at most 50 users and 50 teams",
        ),
    ];
    for (case, message) in cases {
        assert_eq!(invalid(protection.create(&fixture.olga, &admin, &case).await), message, "{case:?}");
    }
    // Without restrict_pushers the lists are dropped (and not validated).
    let open = protection
        .create(&fixture.olga, &admin, &ProtectionInput { users: vec!["rita".into()], ..input("dev/*", RuleSettings::default()) })
        .await
        .unwrap();
    assert!(open.users.is_empty() && open.rule.allowed_users.is_empty());

    // Changes and deletions.
    let changed = protection
        .update(&fixture.olga, &admin, open.id, &input("develop/**", RuleSettings { deny_deletion: false, ..RuleSettings::default() }))
        .await
        .unwrap();
    assert_eq!((changed.rule.pattern.as_str(), changed.rule.settings.deny_deletion), ("develop/**", false));
    assert_eq!(
        invalid(protection.update(&fixture.olga, &admin, open.id, &input("main", RuleSettings::default())).await),
        "a rule for this pattern already exists"
    );
    assert!(matches!(
        protection.update(&fixture.olga, &admin, ProtectionId(999), &input("x", RuleSettings::default())).await,
        Err(ServiceError::NotFound)
    ));
    assert!(matches!(protection.delete(&fixture.bob, &writer, open.id).await, Err(ServiceError::Forbidden)));
    protection.delete(&fixture.olga, &admin, open.id).await.unwrap();
    assert!(matches!(protection.get(&admin, open.id).await, Err(ServiceError::NotFound)));

    let created = fixture.audit(AuditAction::BranchProtectionCreated).await;
    assert_eq!(created.len(), 2);
    assert!(created.iter().all(|event| event.target_name.as_deref() == Some("acme/rocket")));
    assert!(created.iter().any(|event| event.detail == "main (no force pushes, no deletion, restricted)"), "{created:?}");
    let changed = fixture.audit(AuditAction::BranchProtectionChanged).await;
    assert_eq!(changed[0].detail, "develop/** (no force pushes, was dev/*)");
    assert_eq!(fixture.audit(AuditAction::BranchProtectionDeleted).await[0].detail, "develop/**");
    assert!(!format!("{created:?}{changed:?}").contains("carol"), "allowed pushers are not in the log");
}

#[tokio::test]
async fn personal_repositories_cannot_allow_teams() {
    let fixture = Fixture::new().await;
    let input_repo = CreateRepository { name: "mine".into(), description: String::new(), visibility: Visibility::Public };
    fixture.services.repositories.create(&fixture.carol, input_repo).await.unwrap();
    let mine = fixture.services.repositories.open(Some(&fixture.carol), "carol", "mine").await.unwrap();
    let protection = &fixture.services.protection;
    assert!(protection.teams(&mine).await.unwrap().is_empty());
    let teams = ProtectionInput { teams: vec!["core".into()], ..input("main", restricted()) };
    assert_eq!(invalid(protection.create(&fixture.carol, &mine, &teams).await), "only repositories of organizations can allow teams");

    // Rules belong to their repository: another repository's id is not found.
    let admin = fixture.repo(&fixture.olga).await;
    let rule = protection.create(&fixture.olga, &admin, &input("main", RuleSettings::default())).await.unwrap();
    assert!(matches!(protection.get(&mine, rule.id).await, Err(ServiceError::NotFound)));
    assert!(matches!(protection.delete(&fixture.carol, &mine, rule.id).await, Err(ServiceError::NotFound)));
}

#[tokio::test]
async fn the_push_gate_applies_rules_to_each_kind_of_pusher() {
    let fixture = Fixture::new().await;
    let protection = &fixture.services.protection;
    let admin = fixture.repo(&fixture.olga).await;
    let mut rule = input("main", restricted());
    rule.users = vec!["carol".into()];
    rule.teams = vec!["core".into()];
    protection.create(&fixture.olga, &admin, &rule).await.unwrap();
    protection
        .create(&fixture.olga, &admin, &input("release/*", RuleSettings { require_pull_request: true, ..RuleSettings::default() }))
        .await
        .unwrap();

    let main = update("refs/heads/main", Some(ID), Some(ID));
    // A team member and a listed collaborator may push (fast-forwards only).
    for user in [&fixture.bob, &fixture.carol] {
        let verdict = fixture.push(user, vec![main.clone()]).await;
        assert!(!verdict.is_refused() && verdict.deny_non_fast_forwards, "{}", user.name());
    }
    // The owner is an administrator but not listed, and no rule allows bypassing.
    let verdict = fixture.push(&fixture.olga, vec![main.clone(), update("refs/heads/topic", None, Some(ID))]).await;
    assert_eq!(verdict.refusals.len(), 1);
    assert_eq!((verdict.refusals[0].name.as_str(), verdict.refusals[0].reason.as_str()), ("refs/heads/main", reasons::RESTRICTED));
    // Deploy keys are never allowed pushers; refs/pull is refused for everyone.
    let deploy = protection
        .check_push(&admin, &PushActor { user: None, deploy_key: true, access: RepoAccess::Write }, std::slice::from_ref(&main))
        .await
        .unwrap();
    assert_eq!(deploy.refusals[0].reason, reasons::RESTRICTED);
    let pull = fixture.push(&fixture.carol, vec![update("refs/pull/1/head", None, Some(ID))]).await;
    assert_eq!(pull.refusals[0].reason, reasons::PULL_REFS);
    // Pull requests are required for updates of release branches, not their creation.
    let release = fixture.push(&fixture.bob, vec![update("refs/heads/release/2.0", Some(ID), Some(ID))]).await;
    assert_eq!(release.refusals[0].reason, reasons::PULL_REQUEST);
    assert!(!fixture.push(&fixture.bob, vec![update("refs/heads/release/3.0", None, Some(ID))]).await.is_refused());
    assert_eq!(fixture.audit(AuditAction::BranchProtectionBypassed).await.len(), 0);

    // With bypass, the administrator passes the rule, and that is audited.
    let main_rule = protection.list(&admin).await.unwrap().into_iter().find(|rule| rule.rule.pattern.as_str() == "main").unwrap();
    let mut bypassing = ProtectionInput::from_rule(&main_rule);
    bypassing.settings.allow_admin_bypass = true;
    protection.update(&fixture.olga, &admin, main_rule.id, &bypassing).await.unwrap();
    let verdict = fixture.push(&fixture.olga, vec![main.clone()]).await;
    assert!(!verdict.is_refused() && !verdict.deny_non_fast_forwards, "force pushes are allowed too");
    let bypassed = fixture.audit(AuditAction::BranchProtectionBypassed).await;
    assert_eq!(bypassed.len(), 1);
    assert_eq!(bypassed[0].detail, format!("refs/heads/main (otherwise refused: {})", reasons::RESTRICTED));
    assert_eq!(bypassed[0].actor_name.as_deref(), Some("olga"));
    // Others still cannot.
    assert_eq!(fixture.push(&fixture.rita, vec![main]).await.refusals[0].reason, reasons::NO_WRITE);
}

#[tokio::test]
async fn merges_deletions_and_protected_branches() {
    let fixture = Fixture::new().await;
    fixture.branches().await;
    let protection = &fixture.services.protection;
    let admin = fixture.repo(&fixture.olga).await;
    let writer = fixture.repo(&fixture.bob).await;
    assert_eq!(protection.protected_branches(&admin).await.unwrap(), Vec::<String>::new());
    assert_eq!(protection.check_merge(&writer, &fixture.bob, "main").await.unwrap(), MergeRules::unprotected());
    assert!(protection.may_delete_branch(&writer, &fixture.bob, "main").await.unwrap());

    let reviewed = RuleSettings { require_pull_request: true, required_approvals: 2, ..restricted() };
    protection.create(&fixture.olga, &admin, &ProtectionInput { teams: vec!["core".into()], ..input("main", reviewed) }).await.unwrap();
    protection
        .create(&fixture.olga, &admin, &input("release/**", RuleSettings { deny_deletion: false, ..RuleSettings::default() }))
        .await
        .unwrap();
    protection.create(&fixture.olga, &admin, &input("hotfix/*", RuleSettings::default())).await.unwrap();

    let mut protected = protection.protected_branches(&admin).await.unwrap();
    protected.sort();
    assert_eq!(protected, ["main", "release/1.0"], "existing branches only");

    let merge = protection.check_merge(&writer, &fixture.bob, "main").await.unwrap();
    assert_eq!(merge.patterns, ["main"]);
    assert!(merge.require_pull_request && merge.required_approvals == 2 && merge.block_on_changes_requested);
    assert!(merge.merger_may_push && !merge.merger_may_bypass, "bob is on core");
    let carol = fixture.repo(&fixture.carol).await;
    assert!(!protection.check_merge(&carol, &fixture.carol, "main").await.unwrap().merger_may_push);
    assert!(!protection.check_merge(&writer, &fixture.bob, "feature").await.unwrap().is_protected());

    assert!(!protection.may_delete_branch(&writer, &fixture.bob, "main").await.unwrap());
    assert!(protection.may_delete_branch(&writer, &fixture.bob, "release/1.0").await.unwrap());
    assert!(protection.may_delete_branch(&writer, &fixture.bob, "feature").await.unwrap());
    let names = protection.branch_names(&admin).await.unwrap();
    assert_eq!(names.len(), 3);
}

#[cfg(any(windows, target_os = "macos"))]
#[tokio::test]
async fn case_aliases_cannot_bypass_push_merge_or_deletion_rules() {
    let fixture = Fixture::new().await;
    let protection = &fixture.services.protection;
    let admin = fixture.repo(&fixture.olga).await;
    let writer = fixture.repo(&fixture.bob).await;
    let reviewed = RuleSettings { require_pull_request: true, required_approvals: 2, ..RuleSettings::default() };
    for pattern in ["main", "résumé/*"] {
        protection.create(&fixture.olga, &admin, &input(pattern, reviewed)).await.unwrap();
    }
    for branch in ["MAIN", "RÉSUMÉ/Release"] {
        let name = format!("refs/heads/{branch}");
        let verdict = fixture.push(&fixture.bob, vec![update(&name, Some(ID), Some(ID))]).await;
        assert_eq!(verdict.refusals[0].reason, reasons::PULL_REQUEST, "{branch}");
        let deletion = fixture.push(&fixture.bob, vec![update(&name, Some(ID), None)]).await;
        assert_eq!(deletion.refusals[0].reason, reasons::DELETION, "{branch}");
        assert!(!protection.may_delete_branch(&writer, &fixture.bob, branch).await.unwrap(), "{branch}");
        let merge = protection.check_merge(&writer, &fixture.bob, branch).await.unwrap();
        assert!(merge.require_pull_request && merge.required_approvals == 2, "{branch}");
    }
    let verdict = fixture.push(&fixture.bob, vec![update("refs/HEADS/main", Some(ID), Some(ID))]).await;
    assert_eq!(verdict.refusals[0].reason, reasons::NAMESPACE_CASE);
    let verdict = fixture.push(&fixture.olga, vec![update("refs/PULL/1/head", Some(ID), None)]).await;
    assert_eq!(verdict.refusals[0].reason, reasons::PULL_REFS);
    assert!(!fixture.push(&fixture.bob, vec![update("refs/heads/MixedCase", None, Some(ID))]).await.is_refused());
}

// Milestone 3: required status checks (S).
#[tokio::test]
async fn required_status_checks_are_validated_and_reach_the_merge_rules() {
    let fixture = Fixture::new().await;
    let protection = &fixture.services.protection;
    let admin = fixture.repo(&fixture.olga).await;
    let writer = fixture.repo(&fixture.bob).await;
    let checked = RuleSettings { require_status_checks: true, ..RuleSettings::default() };
    let with = |pattern: &str, contexts: &[&str]| ProtectionInput {
        contexts: contexts.iter().map(|context| (*context).to_owned()).collect(),
        ..input(pattern, checked)
    };

    assert_eq!(
        invalid(protection.create(&fixture.olga, &admin, &with("main", &[" "])).await),
        "requiring status checks needs at least one required check"
    );
    let too_long = "x".repeat(256);
    assert!(invalid(protection.create(&fixture.olga, &admin, &with("main", &[&too_long])).await).starts_with("required check:"));

    let rule = protection.create(&fixture.olga, &admin, &with("main", &["ci/build", "lint", "ci/build"])).await.unwrap();
    let contexts: Vec<_> = rule.rule.required_contexts.iter().map(ToString::to_string).collect();
    assert_eq!(contexts, ["ci/build", "lint"]);
    assert_eq!(ProtectionInput::from_rule(&rule).contexts, ["ci/build", "lint"], "the edit form shows them");
    protection.create(&fixture.olga, &admin, &with("m*", &["deploy"])).await.unwrap();
    // Without the flag the contexts are dropped (and not validated).
    let unchecked = ProtectionInput { contexts: vec![too_long.clone()], ..input("release/*", RuleSettings::default()) };
    assert_eq!(protection.create(&fixture.olga, &admin, &unchecked).await.unwrap().rule.required_contexts.len(), 0);

    let merge = protection.check_merge(&writer, &fixture.bob, "main").await.unwrap();
    assert!(merge.require_status_checks);
    let required: Vec<_> = merge.required_contexts.iter().map(ToString::to_string).collect();
    assert_eq!(required, ["ci/build", "deploy", "lint"], "every matching rule's contexts");
    assert!(!protection.check_merge(&writer, &fixture.bob, "release/1.0").await.unwrap().require_status_checks);

    let created = fixture.audit(AuditAction::BranchProtectionCreated).await;
    assert!(created.iter().any(|event| event.detail == "main (no force pushes, no deletion, status checks)"), "{created:?}");
}
