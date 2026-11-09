//! Shared-cache contract (§12.5.6): the key, `restore-keys` and version rules, the order a
//! restore probes scopes in, and how a scope is named as a ref. `rbit-runner` applies them
//! to its local cache and its requests; the server applies them to what runners send and to
//! the cache REST API. Pure: the stores that answer each probe are the callers'.

/// Longest key, as on GitHub, in characters.
pub const MAX_CACHE_KEY_CHARS: usize = 512;

/// Most `restore-keys` one lookup may carry.
pub const MAX_RESTORE_KEYS: usize = 10;

/// Whether `key` is within the key limits: at most [`MAX_CACHE_KEY_CHARS`] characters and
/// no comma. A runner checks a step's (possibly empty) `key` input with this.
pub fn cache_key_fits(key: &str) -> bool {
    key.chars().count() <= MAX_CACHE_KEY_CHARS && !key.contains(',')
}

/// Whether `key` may name or prefix an entry: not empty, and [`cache_key_fits`].
pub fn valid_cache_key(key: &str) -> bool {
    !key.is_empty() && cache_key_fits(key)
}

/// Whether `version` is 64 lowercase hex digits, as the runner derives versions.
pub fn valid_cache_version(version: &str) -> bool {
    version.len() == 64 && version.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// One step of a restore: the exact key in one scope, or the newest entry whose key starts
/// with a `restore-keys` prefix in one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheProbe<'a> {
    Exact { scope: &'a str, key: &'a str },
    Prefix { scope: &'a str, prefix: &'a str },
}

/// The probes of a restore, in the order the first hit wins: `key` in every scope of
/// `read`, then for each prefix of `restore_keys` every scope of `read`.
pub fn restore_probes<'a>(read: &'a [String], key: &'a str, restore_keys: &'a [String]) -> impl Iterator<Item = CacheProbe<'a>> + 'a {
    let exact = read.iter().map(move |scope| CacheProbe::Exact { scope, key });
    let prefixes =
        restore_keys.iter().flat_map(move |prefix| read.iter().map(move |scope| CacheProbe::Prefix { scope, prefix: prefix.as_str() }));
    exact.chain(prefixes)
}

/// The ref GitHub names an entry's scope by: `refs/heads/main` for `heads/main`, and an
/// untrusted pull request's `pull/3` as `refs/pull/3/merge`.
pub fn scope_ref(scope: &str) -> String {
    match scope.strip_prefix("pull/").filter(|number| number.bytes().all(|b| b.is_ascii_digit())) {
        Some(number) => format!("refs/pull/{number}/merge"),
        None => format!("refs/{scope}"),
    }
}

/// The scope a `ref` filter names, the inverse of [`scope_ref`].
pub fn ref_to_scope(reference: &str) -> String {
    let scope = reference.strip_prefix("refs/").unwrap_or(reference);
    match scope.strip_prefix("pull/").and_then(|rest| rest.strip_suffix("/merge")) {
        Some(number) if number.bytes().all(|b| b.is_ascii_digit()) => format!("pull/{number}"),
        _ => scope.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_versions_are_checked() {
        assert!(valid_cache_key("cargo-Linux-abc"));
        assert!(!valid_cache_key(""));
        assert!(cache_key_fits(""), "an empty step input is left to the lookup");
        assert!(!valid_cache_key("a,b") && !cache_key_fits("a,b"));
        assert!(valid_cache_key(&"k".repeat(MAX_CACHE_KEY_CHARS)));
        assert!(!valid_cache_key(&"k".repeat(MAX_CACHE_KEY_CHARS + 1)));
        assert!(valid_cache_key(&"é".repeat(MAX_CACHE_KEY_CHARS)), "characters, not bytes");
        assert!(valid_cache_version(&"0a".repeat(32)));
        assert!(!valid_cache_version(&"0A".repeat(32)));
        assert!(!valid_cache_version("abc"));
    }

    #[test]
    fn restores_probe_exact_keys_first_then_prefixes_scope_by_scope() {
        let read = ["heads/f".to_owned(), "heads/main".to_owned()];
        let prefixes = ["a-".to_owned(), "b-".to_owned()];
        let probes: Vec<_> = restore_probes(&read, "k", &prefixes).collect();
        assert_eq!(
            probes,
            [
                CacheProbe::Exact { scope: "heads/f", key: "k" },
                CacheProbe::Exact { scope: "heads/main", key: "k" },
                CacheProbe::Prefix { scope: "heads/f", prefix: "a-" },
                CacheProbe::Prefix { scope: "heads/main", prefix: "a-" },
                CacheProbe::Prefix { scope: "heads/f", prefix: "b-" },
                CacheProbe::Prefix { scope: "heads/main", prefix: "b-" },
            ]
        );
        assert_eq!(restore_probes(&read, "k", &[]).count(), 2);
    }

    #[test]
    fn content_digests_are_64_hex_digits() {
        use crate::protocol::sha256_from_hex;
        let hex = "2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881";
        let digest = sha256_from_hex(hex).unwrap();
        assert_eq!((digest[0], digest[31]), (0x2d, 0x81));
        assert_eq!(sha256_from_hex(&hex.to_uppercase()), Some(digest));
        assert_eq!(sha256_from_hex(&hex[..62]), None);
        assert_eq!(sha256_from_hex(&"zz".repeat(32)), None);
    }

    #[test]
    fn scopes_and_refs_map_both_ways() {
        for (scope, reference) in [("heads/main", "refs/heads/main"), ("pull/3", "refs/pull/3/merge"), ("pull/3/head", "refs/pull/3/head")]
        {
            assert_eq!(scope_ref(scope), reference);
            assert_eq!(ref_to_scope(reference), scope);
        }
        assert_eq!(ref_to_scope("heads/main"), "heads/main");
    }
}
