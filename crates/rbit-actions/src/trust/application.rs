//! Pull request trust from the facts read now; no storage, identity or HTTP here.
use super::{AuthorStanding, RunTrust, TriggeringActor, TrustFacts, classify_pull_request};

/// Classifies `pull_request` runs, at creation and again when an approved run starts over.
#[derive(Debug)]
pub struct PullRequestTrust;

impl PullRequestTrust {
    /// How a `pull_request` run may run: `actor` caused the event (`None` when unknown),
    /// `author` opened the pull request (`None` once their account is gone) and `is_fork`
    /// says its head is in another repository, which no push can make trusted.
    pub async fn classify<P: TrustFacts>(
        facts: &P,
        actor: Option<P::Account>,
        author: Option<P::Account>,
        is_fork: bool,
    ) -> Result<RunTrust, P::Error> {
        let actor = match actor {
            Some(id) => match facts.actor(id).await? {
                Some(known) => TriggeringActor::Known { access: known.access, suspended: known.suspended, is_author: author == Some(id) },
                None => TriggeringActor::Unknown,
            },
            None => TriggeringActor::Unknown,
        };
        let standing = match author {
            Some(id) => Self::standing(facts, id).await?,
            None => AuthorStanding::Unknown,
        };
        Ok(match classify_pull_request(actor, standing) {
            RunTrust::Trusted if is_fork => RunTrust::Untrusted { awaits_approval: false },
            trust => trust,
        })
    }

    /// Suspension first, then the trusted list, then (when the policy lets it) a merged
    /// pull request; each fact is read only when the ones before did not decide.
    async fn standing<P: TrustFacts>(facts: &P, author: P::Account) -> Result<AuthorStanding, P::Error> {
        if facts.is_suspended(author).await? {
            return Ok(AuthorStanding::Suspended);
        }
        if facts.is_trusted_user(author).await? {
            return Ok(AuthorStanding::Vouched);
        }
        if facts.merged_pull_vouches().await? && facts.has_merged_pull(author).await? {
            return Ok(AuthorStanding::Vouched);
        }
        Ok(AuthorStanding::Unknown)
    }
}
