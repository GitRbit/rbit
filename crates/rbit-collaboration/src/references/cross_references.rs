//! Cross-reference policy retains author visibility, ordered dedup and the existing effect gap.
use super::{CrossReferencePort, RemoteReferencePort, Scan};
use crate::threads::Issue;
use rbit_contracts::{AccountId, Timestamp};
#[derive(Debug)]
pub struct CrossReferences<'a, P> {
    port: &'a P,
}
impl<'a, P: CrossReferencePort> CrossReferences<'a, P> {
    pub fn new(port: &'a P) -> Self {
        Self { port }
    }
    pub async fn record(
        &self,
        source: &Issue,
        actor: Option<AccountId>,
        text: &str,
        scan: impl FnOnce(&str) -> Scan,
        mut now: impl FnMut() -> Timestamp,
    ) -> Result<(), P::Error> {
        let scan = scan(text);
        if scan.local.is_empty() && scan.remote.is_empty() {
            return Ok(());
        }
        let mut targets: Vec<Issue> = Vec::new();
        for target in self.port.numbers(source.repo_id, &scan.local).await? {
            if let Some(issue) = self.port.issue(target.id).await? {
                targets.push(issue);
            }
        }
        let actor_user = match actor {
            Some(id) => self.port.active_user(id).await?,
            None => None,
        };
        if let Some(user) = &actor_user {
            let repositories = self.port.repositories();
            for remote in &scan.remote {
                let Some(repo) = repositories.find(remote).await? else {
                    continue;
                };
                if repo.repo.id == source.repo_id {
                    if let Some(issue) = self.port.by_number(repo.repo.id, remote.number).await? {
                        targets.push(issue);
                    }
                    continue;
                }
                if !repositories.can_read(user, &repo).await? {
                    continue;
                }
                if let Some(issue) = self.port.by_number(repo.repo.id, remote.number).await? {
                    targets.push(issue);
                }
            }
        }
        let mut seen = Vec::new();
        for target in targets {
            if target.id == source.id || seen.contains(&target.id) {
                continue;
            }
            seen.push(target.id);
            if self.port.link(target.id, source.id, now()).await? {
                self.port.record(target.id, source.id, actor).await?;
            }
        }
        Ok(())
    }
}
