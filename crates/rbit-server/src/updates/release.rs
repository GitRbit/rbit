//! Release eligibility and check scheduling. Pure decisions; no I/O.
//!
//! Only published stable releases participate in `SemVer` precedence, and notice links
//! are always constructed under the fixed upstream repository.

use std::time::Duration;

use rbit_services::updates::AvailableRelease;
use semver::Version;
use serde::Deserialize;

const RELEASES: &str = "https://github.com/GitRbit/rbit/releases/tag/";
/// How long a successful check is trusted, and the ceiling of every retry delay.
pub(super) const CACHE_TIME: Duration = Duration::from_hours(24);

/// The fields of an upstream release record the decision reads; others are ignored.
#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(super) tag_name: String,
    pub(super) draft: bool,
    pub(super) prerelease: bool,
}

pub(super) fn newer_release(release: Release, running: &str) -> Result<Option<AvailableRelease>, &'static str> {
    if release.draft || release.prerelease {
        return Ok(None);
    }
    let tag = release.tag_name;
    if tag.len() > 128 || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+')) {
        return Err("invalid release tag");
    }
    let candidate = Version::parse(tag.strip_prefix('v').unwrap_or(&tag)).map_err(|_| "invalid release version")?;
    let current = Version::parse(running).map_err(|_| "invalid running version")?;
    if !candidate.pre.is_empty() || !candidate.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    Ok(Some(AvailableRelease { version: candidate.to_string(), url: format!("{RELEASES}{tag}") }))
}

/// Exponential backoff after consecutive failures, honoring a longer upstream hint.
pub(super) fn retry_delay(failures: u32, advised: Option<Duration>) -> Duration {
    failure_delay(failures).max(advised.unwrap_or_default()).min(CACHE_TIME)
}

fn failure_delay(failures: u32) -> Duration {
    Duration::from_secs(15 * 60 * 2_u64.saturating_pow(failures.saturating_sub(1).min(7))).min(CACHE_TIME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release { tag_name: tag.into(), draft: false, prerelease: false }
    }

    #[test]
    fn release_precedence_ignores_build_metadata_and_rejects_unsafe_tags() {
        for (tag, current, newer) in [
            ("v1.10.0", "1.9.0", true),
            ("1.9.0", "1.10.0", false),
            ("v1.0.0", "1.0.0-rc.1", true),
            ("v1.0.0", "1.1.0-rc.1", false),
            ("v1.0.0+upstream", "1.0.0+local", false),
            ("v2.0.0-rc.1", "1.0.0", false),
        ] {
            assert_eq!(newer_release(release(tag), current).unwrap().is_some(), newer, "{tag} vs {current}");
        }
        for tag in ["../../evil", "https://evil.example", "v1.0", "v1.0.0\n"] {
            assert!(newer_release(release(tag), "0.1.0").is_err());
        }
        let mut draft = release("v99.0.0");
        draft.draft = true;
        assert!(newer_release(draft, "0.1.0").unwrap().is_none());
        let mut preview = release("v99.0.0");
        preview.prerelease = true;
        assert!(newer_release(preview, "0.1.0").unwrap().is_none());
        assert_eq!(failure_delay(1), Duration::from_mins(15));
        assert_eq!(failure_delay(2), Duration::from_mins(30));
        assert_eq!(failure_delay(u32::MAX), CACHE_TIME);
        assert_eq!(retry_delay(1, Some(Duration::from_hours(1))), Duration::from_hours(1));
        assert_eq!(retry_delay(2, None), Duration::from_mins(30));
        assert_eq!(retry_delay(1, Some(Duration::from_hours(48))), CACHE_TIME);
    }
}
