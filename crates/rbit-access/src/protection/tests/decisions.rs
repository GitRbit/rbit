use super::super::*;
use crate::RepoAccess;

type Rule = ProtectionRule<u32, u32, String>;

fn pattern(value: &str) -> BranchPattern {
    value.parse().unwrap()
}

fn context(value: &str) -> String {
    value.to_owned()
}

/// A push command as the transports parse it.
#[derive(Debug, Clone)]
struct RefUpdate {
    name: String,
    before: Option<String>,
    after: Option<String>,
}

impl RefChange for RefUpdate {
    fn name(&self) -> &str {
        &self.name
    }
    fn is_created(&self) -> bool {
        self.before.is_none()
    }
    fn is_deleted(&self) -> bool {
        self.after.is_none()
    }
}

// ---------------------------------------------------------- the decision matrix

const ALICE: u32 = 1;
const LISTED: u32 = 2;
const TEAM: u32 = 7;
const OTHER_TEAM: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Actor {
    DeployKey,
    Reader,
    Writer,
    Admin,
    ListedUser,
    TeamMember,
}

const ACTORS: [Actor; 6] = [Actor::DeployKey, Actor::Reader, Actor::Writer, Actor::Admin, Actor::ListedUser, Actor::TeamMember];

impl Actor {
    fn pusher(self) -> Pusher<u32, u32> {
        match self {
            Self::DeployKey => Pusher::DeployKey { access: RepoAccess::Write },
            Self::Reader => Pusher::User { id: ALICE, access: RepoAccess::Read, teams: vec![] },
            Self::Writer => Pusher::User { id: ALICE, access: RepoAccess::Write, teams: vec![OTHER_TEAM] },
            Self::Admin => Pusher::User { id: ALICE, access: RepoAccess::Admin, teams: vec![] },
            Self::ListedUser => Pusher::User { id: LISTED, access: RepoAccess::Write, teams: vec![] },
            Self::TeamMember => Pusher::User { id: ALICE, access: RepoAccess::Write, teams: vec![OTHER_TEAM, TEAM] },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Create,
    Update,
    Delete,
}

fn update(name: &str, kind: Kind) -> RefUpdate {
    let id = "1".repeat(40);
    let (before, after) = match kind {
        Kind::Create => (None, Some(id)),
        Kind::Update => (Some(id.clone()), Some(id)),
        Kind::Delete => (Some(id), None),
    };
    RefUpdate { name: name.to_owned(), before, after }
}

fn rule(glob: &str, settings: RuleSettings) -> Rule {
    ProtectionRule {
        pattern: pattern(glob),
        settings,
        allowed_users: vec![LISTED],
        allowed_teams: vec![TEAM],
        required_contexts: vec![context("ci")],
    }
}

/// The reference table, written independently of [`decide_push`]: what one rule with
/// these flags does to one update of a branch it matches. `None`: allowed.
fn expected(flags: [bool; 5], actor: Actor, kind: Kind) -> (Option<&'static str>, bool) {
    let [force, deletion, pull_request, restrict, bypass] = flags;
    if actor == Actor::Reader {
        return (Some(reasons::NO_WRITE), false);
    }
    if bypass && actor == Actor::Admin {
        return (None, false);
    }
    let allowed = matches!(actor, Actor::ListedUser | Actor::TeamMember);
    match kind {
        Kind::Delete if deletion => (Some(reasons::DELETION), false),
        _ if restrict && !allowed => (Some(reasons::RESTRICTED), false),
        Kind::Update if pull_request => (Some(reasons::PULL_REQUEST), false),
        Kind::Update => (None, force),
        Kind::Create | Kind::Delete => (None, false),
    }
}

#[test]
fn every_flag_actor_and_update_kind() {
    let mut cases = 0;
    for bits in 0..32_u8 {
        let flags = [0, 1, 2, 3, 4].map(|bit| bits & (1 << bit) != 0);
        let settings = RuleSettings {
            deny_force_push: flags[0],
            deny_deletion: flags[1],
            require_pull_request: flags[2],
            restrict_pushers: flags[3],
            allow_admin_bypass: flags[4],
            ..RuleSettings::default()
        };
        let rules = [rule("main", settings)];
        for actor in ACTORS {
            for kind in [Kind::Create, Kind::Update, Kind::Delete] {
                let decision = decide_push(&rules, &actor.pusher(), &[update("refs/heads/main", kind)]);
                let (reason, deny_nff) = expected(flags, actor, kind);
                let got = decision.verdict.refusals.first().map(|refusal| refusal.reason.as_str());
                assert_eq!((got, decision.verdict.deny_non_fast_forwards), (reason, deny_nff), "{flags:?} {actor:?} {kind:?}");

                // A bypass is reported exactly when it changed the outcome.
                let strict = expected([flags[0], flags[1], flags[2], flags[3], false], actor, kind);
                assert_eq!(decision.bypasses.len(), usize::from(strict != (reason, deny_nff)), "{flags:?} {actor:?} {kind:?}");

                // Unprotected branches and tags are never touched by the rule.
                for name in ["refs/heads/other", "refs/tags/main"] {
                    let free = decide_push(&rules, &actor.pusher(), &[update(name, kind)]);
                    let expected_free = if actor == Actor::Reader { Some(reasons::NO_WRITE) } else { None };
                    assert_eq!(free.verdict.refusals.first().map(|r| r.reason.as_str()), expected_free);
                    assert!(!free.verdict.deny_non_fast_forwards && free.bypasses.is_empty());
                }
                // The reserved namespace is refused for everyone, whatever the rules.
                let pull = decide_push(&rules, &actor.pusher(), &[update("refs/pull/1/head", kind)]);
                assert_eq!(pull.verdict.refusals[0].reason, reasons::PULL_REFS);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 32 * ACTORS.len() * 3);
}

#[test]
fn matching_rules_combine_strictly() {
    let admin = Actor::Admin.pusher();
    let writer = Actor::Writer.pusher();
    let lenient = RuleSettings { deny_force_push: false, deny_deletion: false, ..RuleSettings::default() };
    let rules = [rule("release/**", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() }), rule("release/1.*", lenient)];
    // The force-push protection of the first rule applies to writers.
    let decision = decide_push(&rules, &writer, &[update("refs/heads/release/1.0", Kind::Update)]);
    assert!(decision.verdict.deny_non_fast_forwards && !decision.verdict.is_refused());
    // The admin bypasses the first rule; the second protects nothing.
    let decision = decide_push(&rules, &admin, &[update("refs/heads/release/1.0", Kind::Delete)]);
    assert!(!decision.verdict.is_refused());
    assert_eq!(
        decision.bypasses,
        [Bypass { name: "refs/heads/release/1.0".to_owned(), instead: format!("refused: {}", reasons::DELETION) }]
    );

    // A matching rule without bypass still applies to administrators: no bypass recorded.
    let rules = [rule("main", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() }), rule("ma*", RuleSettings::default())];
    let decision = decide_push(&rules, &admin, &[update("refs/heads/main", Kind::Delete)]);
    assert_eq!(decision.verdict.refusals[0].reason, reasons::DELETION);
    assert!(decision.bypasses.is_empty(), "the outcome did not change");

    // Restrictions intersect: a pusher must be allowed by every restricting rule.
    let restricted = RuleSettings { restrict_pushers: true, ..lenient };
    let only_listed = ProtectionRule { allowed_teams: vec![], ..rule("main", restricted) };
    let only_team = ProtectionRule { allowed_users: vec![], ..rule("m*", restricted) };
    for (actor, allowed) in [(Actor::ListedUser, false), (Actor::TeamMember, false)] {
        let decision = decide_push(&[only_listed.clone(), only_team.clone()], &actor.pusher(), &[update("refs/heads/main", Kind::Create)]);
        assert_eq!(!decision.verdict.is_refused(), allowed, "{actor:?}");
    }
    let both = Pusher::User { id: LISTED, access: RepoAccess::Write, teams: vec![TEAM] };
    assert!(!decide_push(&[only_listed, only_team], &both, &[update("refs/heads/main", Kind::Create)]).verdict.is_refused());
}

#[test]
fn every_ref_of_a_push_is_decided() {
    let rules = [rule("main", RuleSettings::default())];
    let updates = [
        update("refs/heads/main", Kind::Delete),
        update("refs/heads/topic", Kind::Update),
        update("refs/pull/3/head", Kind::Create),
        update("refs/heads/main", Kind::Update),
    ];
    let decision = decide_push(&rules, &Actor::Writer.pusher(), &updates);
    let refused: Vec<_> = decision.verdict.refusals.iter().map(|r| (r.name.as_str(), r.reason.as_str())).collect();
    assert_eq!(refused, [("refs/heads/main", reasons::DELETION), ("refs/pull/3/head", reasons::PULL_REFS)]);
    assert!(decision.verdict.deny_non_fast_forwards, "the update of main is force-push protected");
    assert_eq!(decide_push(&rules, &Actor::Writer.pusher(), &[] as &[RefUpdate]).verdict, PushVerdict::allow());
}

#[test]
fn merge_requirements() {
    let writer = Actor::Writer.pusher();
    assert_eq!(merge_rules::<_, _, String>(&[], &writer, "main"), MergeRules::unprotected());
    assert!(!MergeRules::<String>::unprotected().is_protected());

    let reviewed = RuleSettings {
        require_pull_request: true,
        required_approvals: 2,
        dismiss_stale_approvals: true,
        block_on_changes_requested: false,
        ..RuleSettings::default()
    };
    let rules = [
        rule("main", reviewed),
        rule("ma*", RuleSettings { required_approvals: 0, ..RuleSettings::default() }),
        rule("other", RuleSettings { restrict_pushers: true, ..RuleSettings::default() }),
    ];
    let merge = merge_rules(&rules, &writer, "main");
    assert_eq!(merge.patterns, ["main", "ma*"]);
    assert!(merge.is_protected() && merge.require_pull_request && merge.dismiss_stale_approvals);
    assert_eq!(merge.required_approvals, 2);
    assert!(!merge.block_on_changes_requested, "only rules requiring pull requests set review options");
    assert!(merge.merger_may_push && !merge.merger_may_bypass);

    let restricted = [rule("main", RuleSettings { restrict_pushers: true, allow_admin_bypass: true, ..reviewed })];
    let merge = merge_rules(&restricted, &writer, "main");
    assert!(!merge.merger_may_push && !merge.merger_may_bypass && merge.block_on_changes_requested == reviewed.block_on_changes_requested);
    assert!(merge_rules(&restricted, &Actor::ListedUser.pusher(), "main").merger_may_push);
    assert!(merge_rules(&restricted, &Actor::TeamMember.pusher(), "main").merger_may_push);
    let admin = merge_rules(&restricted, &Actor::Admin.pusher(), "main");
    assert!(!admin.merger_may_push && admin.merger_may_bypass);
    // Bypass needs every matching rule to allow it.
    let mixed = [restricted[0].clone(), rule("*", RuleSettings::default())];
    assert!(!merge_rules(&mixed, &Actor::Admin.pusher(), "main").merger_may_bypass);
    assert!(!merge_rules(&restricted, &Actor::DeployKey.pusher(), "main").merger_may_push);
    assert!(!merge.require_status_checks && merge.required_contexts.is_empty(), "contexts count only with the flag");
}

#[test]
fn required_checks_are_the_union_of_the_matching_rules() {
    let checked = RuleSettings { require_status_checks: true, ..RuleSettings::default() };
    let rules = [
        ProtectionRule { required_contexts: vec![context("lint"), context("ci")], ..rule("main", checked) },
        ProtectionRule { required_contexts: vec![context("ci"), context("build")], ..rule("ma*", checked) },
        // Without the flag its contexts are not required.
        ProtectionRule { required_contexts: vec![context("docs")], ..rule("*", RuleSettings::default()) },
        ProtectionRule { required_contexts: vec![context("deploy")], ..rule("release/*", checked) },
    ];
    let merge = merge_rules(&rules, &Actor::Writer.pusher(), "main");
    assert!(merge.require_status_checks);
    assert_eq!(merge.required_contexts, [context("build"), context("ci"), context("lint")]);
    let other = merge_rules(&rules, &Actor::Writer.pusher(), "feature");
    assert!(other.is_protected() && !other.require_status_checks && other.required_contexts.is_empty());
    // Status checks never change what a push may do.
    let decision = decide_push(&rules, &Actor::Writer.pusher(), &[update("refs/heads/main", Kind::Update)]);
    assert!(!decision.verdict.is_refused());
}

#[test]
fn deleting_and_listing_protected_branches() {
    let rules = [rule("main", RuleSettings::default()), rule("tmp/*", RuleSettings { deny_deletion: false, ..RuleSettings::default() })];
    let writer = Actor::Writer.pusher();
    assert!(!may_delete(&rules, &writer, "main"));
    assert!(may_delete(&rules, &writer, "tmp/x") && may_delete(&rules, &writer, "feature"));
    let bypassing = [rule("main", RuleSettings { allow_admin_bypass: true, ..RuleSettings::default() })];
    assert!(may_delete(&bypassing, &Actor::Admin.pusher(), "main") && !may_delete(&bypassing, &writer, "main"));
    assert!(is_protected(&rules, "tmp/x") && is_protected(&rules, "main") && !is_protected(&rules, "feature"));
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn filesystem_case_aliases_share_every_protection_check() {
    let settings = RuleSettings { require_pull_request: true, required_approvals: 2, ..RuleSettings::default() };
    let rules = [rule("main", settings), rule("résumé/*", settings)];
    let writer = Actor::Writer.pusher();
    for branch in ["MAIN", "Main", "RÉSUMÉ/Release"] {
        assert!(is_protected(&rules, branch), "{branch}");
        assert!(!may_delete(&rules, &writer, branch), "{branch}");
        let merge = merge_rules(&rules, &writer, branch);
        assert!(merge.require_pull_request && merge.required_approvals == 2, "{branch}");
        let name = format!("refs/heads/{branch}");
        let decision = decide_push(&rules, &writer, &[update(&name, Kind::Update)]);
        assert_eq!(decision.verdict.refusals[0].reason, reasons::PULL_REQUEST);
    }
    assert!(pattern("résumé").matches("RÉSUMÉ"), "exact patterns use Unicode case mappings too");
    assert!(pattern("RÉSUMÉ/*").matches("résumé/feature"), "case matching is symmetric");
    assert!(!pattern("main").matches("main/other"));
    assert!(!pattern("main").matches("maintain"));
    assert!(!decide_push(&[] as &[Rule], &writer, &[update("refs/heads/MixedCase", Kind::Create)]).verdict.is_refused());
    let force = decide_push(&[rule("main", RuleSettings::default())], &writer, &[update("refs/heads/MAIN", Kind::Update)]);
    assert!(force.verdict.deny_non_fast_forwards);
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn filesystem_namespace_aliases_are_refused_before_git() {
    for actor in ACTORS {
        for kind in [Kind::Create, Kind::Update, Kind::Delete] {
            for name in ["refs/HEADS/main", "REFS/heads/main", "refs/Headſ/main"] {
                let decision = decide_push(&[] as &[Rule], &actor.pusher(), &[update(name, kind)]);
                assert_eq!(decision.verdict.refusals[0].reason, reasons::NAMESPACE_CASE, "{name}");
            }
            for name in ["refs/PULL/1/head", "REFS/pull/1/head", "refs/Pull/1/merge"] {
                let decision = decide_push(&[] as &[Rule], &actor.pusher(), &[update(name, kind)]);
                assert_eq!(decision.verdict.refusals[0].reason, reasons::PULL_REFS, "{name}");
            }
        }
    }
}
