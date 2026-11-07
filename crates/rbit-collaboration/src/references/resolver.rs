//! Rendering resolution preserves first occurrence order and one batch per reference kind.
use super::{MAX_MENTIONS, ReferenceReadPort, ReferenceSet, ReferencedIssue, Scan};
use crate::threads::IssueNumber;
use rbit_contracts::RepoId;
#[derive(Debug)]
pub struct ReferenceResolver<'a, P> {
    port: &'a P,
}
impl<'a, P: ReferenceReadPort> ReferenceResolver<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn resolve(&self, repo: RepoId, texts: &[&str], scan: impl Fn(&str) -> Scan) -> Result<ReferenceSet, P::Error> {
        let mut numbers: Vec<IssueNumber> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for text in texts {
            let scan = scan(text);
            for number in scan.local {
                if !numbers.contains(&number) {
                    numbers.push(number);
                }
            }
            for name in scan.mentions {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names.truncate(MAX_MENTIONS.saturating_mul(texts.len().max(1)));
        let (issues, accounts) = self.port.resolve_batch(repo, &numbers, &names).await?;
        Ok(ReferenceSet {
            issues: issues
                .into_iter()
                .map(|issue| {
                    let referenced = ReferencedIssue { number: issue.number, kind: issue.kind, state: issue.state, title: issue.title };
                    (issue.number, referenced)
                })
                .collect(),
            accounts: accounts.into_iter().map(|account| (account.name.as_str().to_ascii_lowercase(), account.name.to_string())).collect(),
        })
    }
}
