#![cfg(feature = "applications")]
//! Pull request opening order and refusal text without Git or persistence.

use std::sync::Mutex;

use rbit_collaboration::{
    pulls::{BranchComparison, PullCreation, PullCreationAccess, PullCreationError, PullCreationPort, PullDraft, PullInput, PullSide},
    threads::IssueNumber,
};

#[expect(clippy::struct_excessive_bools, reason = "independent scripted outcomes")]
struct Script {
    calls: Mutex<Vec<String>>,
    network: bool,
    mirror: bool,
    tips: bool,
    comparison: BranchComparison,
    duplicate: bool,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            calls: Mutex::default(),
            network: true,
            mirror: false,
            tips: true,
            comparison: BranchComparison { related: true, ahead: 1, newest_summary: Some("Fix the crash".into()) },
            duplicate: false,
        }
    }
}

impl Script {
    fn call(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[allow(clippy::unused_async_trait_impl, reason = "scripted effects stay lazy until poll like the real adapter")]
impl PullCreationPort for Script {
    type Error = &'static str;
    type Created = PullDraft;

    async fn head_in_network(&self) -> Result<bool, Self::Error> {
        self.call("network".into());
        Ok(self.network)
    }
    async fn base_is_mirror(&self) -> Result<bool, Self::Error> {
        self.call("mirror".into());
        Ok(self.mirror)
    }
    async fn tip(&self, side: PullSide, reference: String) -> Option<String> {
        self.call(format!("tip {side:?} {reference}"));
        self.tips.then(|| format!("{side:?}").to_lowercase())
    }
    async fn compare(&self, base: &str, head: &str) -> Result<BranchComparison, Self::Error> {
        self.call(format!("compare {base} {head}"));
        Ok(self.comparison.clone())
    }
    async fn create(&self, draft: PullDraft) -> Result<Result<PullDraft, IssueNumber>, Self::Error> {
        self.call("create".into());
        Ok(if self.duplicate { Err(IssueNumber::new(7).unwrap()) } else { Ok(draft) })
    }
}

const ACCESS: PullCreationAccess = PullCreationAccess { same_repo: true, base_readable: true, head_readable: true, base_archived: false };

fn input<'a>(base: &'a str, head: &'a str, title: &'a str) -> PullInput<'a> {
    PullInput { base_branch: base, head_branch: head, title, body: "", draft: false }
}

async fn open(port: &Script, access: PullCreationAccess, input: PullInput<'_>) -> Result<PullDraft, PullCreationError<&'static str>> {
    PullCreation::new(port, access)
        .create(input, || {
            port.call("spend".into());
            Ok(())
        })
        .await
}

fn invalid(result: Result<PullDraft, PullCreationError<&'static str>>) -> String {
    match result {
        Err(PullCreationError::Invalid(message)) => message,
        other => panic!("expected an invalid request, got {other:?}"),
    }
}

#[tokio::test]
async fn refusals_follow_the_legacy_order() {
    let hidden = PullCreationAccess { head_readable: false, ..ACCESS };
    assert!(matches!(open(&Script::default(), hidden, input("main", "x", "")).await, Err(PullCreationError::NotFound)));
    let outside = Script { network: false, ..Script::default() };
    let fork = PullCreationAccess { same_repo: false, base_archived: true, ..ACCESS };
    assert!(matches!(open(&outside, fork, input("main", "x", "")).await, Err(PullCreationError::NotFound)), "network before archive");
    assert!(matches!(open(&Script::default(), fork, input("main", "x", "")).await, Err(PullCreationError::Archived)));
    let mirror = Script { mirror: true, ..Script::default() };
    assert_eq!(invalid(open(&mirror, ACCESS, input("main", "x", "")).await), "pull requests cannot be opened against a mirror");
    assert_eq!(invalid(open(&Script::default(), ACCESS, input("main", "a..b", "")).await), "choose existing branches to compare");
    assert_eq!(invalid(open(&Script::default(), ACCESS, input(" main ", "main", "")).await), "the base and the head branch must differ");
    let missing = Script { tips: false, ..Script::default() };
    assert_eq!(invalid(open(&missing, ACCESS, input("main", "x", "")).await), "the base branch does not exist");
    let unrelated = Script { comparison: BranchComparison { related: false, ahead: 3, newest_summary: None }, ..Script::default() };
    assert_eq!(invalid(open(&unrelated, ACCESS, input("main", "x", "")).await), "the branches have entirely different histories");
    let merged = Script { comparison: BranchComparison { related: true, ahead: 0, newest_summary: None }, ..Script::default() };
    assert_eq!(invalid(open(&merged, ACCESS, input("main", "x", "")).await), "main already contains every commit of x");
    let duplicate = Script { duplicate: true, ..Script::default() };
    assert_eq!(invalid(open(&duplicate, ACCESS, input("main", "x", "")).await), "pull request #7 already proposes these branches");
    assert_eq!(duplicate.calls(), ["mirror", "tip Base refs/heads/main", "tip Head refs/heads/x", "compare base head", "spend", "create"]);
    assert_eq!(missing.calls(), ["mirror", "tip Base refs/heads/main"], "nothing is spent on refusals");
}

#[tokio::test]
async fn titles_default_to_the_single_commit_or_the_branch() {
    let one = open(&Script::default(), ACCESS, input("main", " fix ", "")).await.unwrap();
    assert_eq!((one.title.as_str(), one.head_branch.as_str(), one.head_sha.as_str()), ("Fix the crash", "fix", "head"));
    let two =
        Script { comparison: BranchComparison { related: true, ahead: 2, newest_summary: Some("Newest".into()) }, ..Script::default() };
    assert_eq!(open(&two, ACCESS, input("main", "fix", "")).await.unwrap().title.as_str(), "fix");
    assert_eq!(open(&two, ACCESS, input("main", "fix", " Mine ")).await.unwrap().title.as_str(), "Mine");
}
