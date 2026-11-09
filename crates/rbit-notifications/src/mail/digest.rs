//! Daily and weekly digests: periods, claims, re-checks and grouping.
//!
//! For every user with queued notifications a run works out the period that started
//! last ([`period_start`]: each day, or each Monday, at the user's digest hour in UTC, or
//! the instance's) and claims the user's digest for it. The claim is one row per user and
//! period, so each period gets at most one digest however often runs happen, and a
//! restart mid-run does not send it again. Before sending, everything is checked again:
//! the recipient must still be active, verified and want email, and still read each
//! repository. A failed digest is not retried: sending is at most once.
use super::{
    policy::{Cadence, MAX_DIGEST_HOUR, MailPreference, thread_link, thread_path, wants},
    port::DigestOutbox,
    records::{Claim, ClaimedDigest, DigestItem, DigestState},
};
use rbit_collaboration::threads::{IssueId, IssueKind};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use std::collections::{HashMap, hash_map::Entry};

const HOUR: i64 = 60 * 60;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;
/// 1970-01-05, the first Monday after the Unix epoch (a Thursday).
const FIRST_MONDAY: i64 = 4 * DAY;
/// Notifications waiting for a digest are never pruned sooner than this.
const MIN_PENDING_RETENTION: i64 = 2 * WEEK;
/// Users with queued notifications read per page.
pub const CANDIDATES_PER_PAGE: u32 = 200;

/// What one digest run did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DigestRun {
    /// Digests the mail transport accepted.
    pub sent: u64,
    /// Digests the mail transport refused (or that could not be built).
    pub failed: u64,
    /// Digests claimed with nothing left to send (access or preferences changed).
    pub empty: u64,
    /// Queued notifications left out because their thread was read in the app.
    pub skipped_read: u64,
}

/// One repository's part of a digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestSection {
    pub name: String,
    pub link: String,
    pub threads: Vec<DigestSectionThread>,
}

/// One thread of a digest section and what happened on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestSectionThread {
    pub number: u64,
    pub title: String,
    pub is_pull: bool,
    pub link: String,
    /// `(headline, excerpt, link)` in order.
    pub entries: Vec<(String, String, String)>,
}

/// The start of the digest period `now` falls in: the latest day (daily) or Monday
/// (weekly) at `hour`:00 UTC that is not after `now`. Immediate delivery uses daily
/// periods for what was queued before the user switched back.
pub fn period_start(cadence: Cadence, hour: u8, now: Timestamp) -> Timestamp {
    let hour = i64::from(hour.min(MAX_DIGEST_HOUR)) * HOUR;
    let (length, origin) = match cadence {
        Cadence::Weekly => (WEEK, FIRST_MONDAY + hour),
        Cadence::Daily | Cadence::Immediate => (DAY, hour),
    };
    Timestamp(now.0.saturating_sub(origin).div_euclid(length).saturating_mul(length).saturating_add(origin))
}

/// Where pruning stops for digests: the inbox retention, but never less than two weeks.
pub fn prune_cutoff(retention_cutoff: Timestamp, now: Timestamp) -> Timestamp {
    Timestamp(retention_cutoff.0.min(now.0.saturating_sub(MIN_PENDING_RETENTION)))
}

/// `items` (sorted by repository, thread and time) as the digest's sections.
pub fn group(base_url: &str, items: &[DigestItem]) -> Vec<DigestSection> {
    let mut sections: Vec<DigestSection> = Vec::new();
    let mut last: Option<(RepoId, IssueId)> = None;
    for item in items {
        let full_name = format!("{}/{}", item.owner, item.repo_name);
        let path = thread_path(&full_name, item.kind, item.number.get());
        if last.map(|(repo, _)| repo) != Some(item.repo) {
            sections.push(DigestSection { name: full_name.clone(), link: format!("{base_url}/{full_name}"), threads: Vec::new() });
        }
        let Some(section) = sections.last_mut() else { continue };
        if last != Some((item.repo, item.issue)) {
            section.threads.push(DigestSectionThread {
                number: u64::from(item.number.get()),
                title: item.title.clone(),
                is_pull: item.kind == IssueKind::Pull,
                link: format!("{base_url}/{path}"),
                entries: Vec::new(),
            });
        }
        last = Some((item.repo, item.issue));
        let Some(thread) = section.threads.last_mut() else { continue };
        let link = thread_link(base_url, &path, item.anchor.as_deref());
        thread.entries.push((item.headline.clone(), item.excerpt.clone(), link));
    }
    sections
}

/// The settings a run applies to users who never chose.
#[derive(Debug, Clone, Copy)]
pub struct DigestDefaults<'a> {
    pub preference: MailPreference,
    pub hour: u8,
    pub base_url: &'a str,
}

/// Sends every digest due at `clock()`; finishing stamps read the clock again.
pub async fn send_digests<O: DigestOutbox>(
    outbox: &O,
    defaults: DigestDefaults<'_>,
    clock: impl Fn() -> Timestamp,
) -> Result<DigestRun, O::Error> {
    let mut run = DigestRun::default();
    let now = clock();
    let mut after = AccountId(0);
    loop {
        let page = outbox.candidates(after, CANDIDATES_PER_PAGE).await?;
        let Some(last) = page.last() else { break };
        after = last.account;
        for candidate in &page {
            let period = period_start(candidate.cadence, candidate.digest_hour.unwrap_or(defaults.hour), now);
            let claimed = match outbox.claim(candidate.account, period, now).await? {
                Claim::Taken => continue,
                Claim::Nothing { skipped_read } => {
                    run.skipped_read += skipped_read;
                    continue;
                }
                Claim::Claimed { digest, skipped_read } => {
                    run.skipped_read += skipped_read;
                    digest
                }
            };
            let weekly = candidate.cadence == Cadence::Weekly;
            let state = match send_digest(outbox, defaults, &claimed, weekly).await {
                Ok(state) => state,
                Err(error) => {
                    outbox.build_failed(claimed.account, &error);
                    DigestState::Failed
                }
            };
            match state {
                DigestState::Sent => run.sent += 1,
                DigestState::Empty => run.empty += 1,
                DigestState::Failed | DigestState::Claimed => run.failed += 1,
            }
            if matches!(state, DigestState::Sent | DigestState::Failed) {
                outbox.record(state == DigestState::Sent);
            }
            outbox.finish(claimed.id, state, clock()).await?;
        }
        if page.len() < usize::try_from(CANDIDATES_PER_PAGE).unwrap_or(usize::MAX) {
            break;
        }
    }
    Ok(run)
}

/// Checks, renders and delivers one claimed digest; returns what became of it.
async fn send_digest<O: DigestOutbox>(
    outbox: &O,
    defaults: DigestDefaults<'_>,
    claimed: &ClaimedDigest,
    weekly: bool,
) -> Result<DigestState, O::Error> {
    // Suspended, deleted or no longer verified.
    let Some(recipient) = outbox.recipient(claimed.account).await? else { return Ok(DigestState::Empty) };
    let preference = recipient.preference.unwrap_or(defaults.preference);
    if preference == MailPreference::Off {
        return Ok(DigestState::Empty);
    }
    let items: Vec<_> = outbox.items(claimed.id).await?.into_iter().filter(|item| wants(preference, item.reason)).collect();
    // Read access is checked again now, once per repository.
    let mut readable = HashMap::<RepoId, bool>::new();
    for repo in items.iter().map(|item| item.repo) {
        if let Entry::Vacant(entry) = readable.entry(repo) {
            entry.insert(outbox.can_read(&recipient, repo).await?);
        }
    }
    let items: Vec<_> = items.into_iter().filter(|item| readable.get(&item.repo).copied().unwrap_or(false)).collect();
    if items.is_empty() {
        return Ok(DigestState::Empty);
    }
    let mail = outbox.render(&recipient, claimed, weekly, group(defaults.base_url, &items)).await?;
    Ok(if outbox.deliver(&recipient, mail, items.len()).await { DigestState::Sent } else { DigestState::Failed })
}

#[cfg(test)]
#[path = "digest_tests.rs"]
mod tests;
