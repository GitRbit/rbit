//! The outbound address policy shared by every request rbit makes to a URL that a user
//! or a provider chose: webhook deliveries, imports, pull and push mirrors and sign-in
//! providers.
//!
//! * [`lookup`] is the only name resolver those paths use. HTTP clients reach it through
//!   the webhooks' `GuardedResolver`, which drops internal answers at connection time;
//!   Git reaches it through [`check_public_host`], whose approved answer is then pinned
//!   for `git` (see [`rbit_git::remote::ResolvedHost`]), so Git never resolves again.
//! * [`refuses_literal`] classifies address literals, which never reach a resolver.

use std::{
    io,
    net::{IpAddr, SocketAddr},
};

use rbit_core::network::is_internal_address;
use rbit_git::remote::ResolvedHost;

/// Resolves `host` (a name, never an address literal) to socket addresses on `port`.
pub(crate) async fn lookup(host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
    Ok(tokio::net::lookup_host((host, port)).await?.collect())
}

/// Whether `url` names an internal address literal that `allow_private` does not permit.
/// `url` is already canonical: numeric IPv4 spellings such as `127.1`, `0x7f000001` or
/// `0177.0.0.1`, a trailing dot and full-width digits have become the dotted address.
pub(crate) fn refuses_literal(url: &reqwest::Url, allow_private: bool) -> bool {
    let literal = url.host_str().map(|host| host.trim_start_matches('[').trim_end_matches(']'));
    !allow_private && literal.and_then(|host| host.parse::<IpAddr>().ok()).is_some_and(is_internal_address)
}

/// Checks that every address `host` resolves to is public (unless `allow_private`), and
/// returns that answer for Git to pin. The error is shown to the user.
pub(crate) async fn check_public_host(host: &str, port: u16, allow_private: bool) -> Result<ResolvedHost, String> {
    // URL clients recognize shortened, hexadecimal and octal IPv4 spellings before
    // DNS. Classify their canonical address rather than relying on platform-specific
    // getaddrinfo behavior, and use the canonical IDNA hostname for DNS pinning.
    let canonical = if host.parse::<IpAddr>().is_ok() {
        host.to_owned()
    } else {
        let parsed = reqwest::Url::parse(&format!("http://{host}:{port}/")).map_err(|_| format!("cannot resolve {host}"))?;
        parsed.host_str().ok_or_else(|| format!("cannot resolve {host}"))?.to_owned()
    };
    let host = canonical.trim_start_matches('[').trim_end_matches(']');
    let mut addresses: Vec<IpAddr> = match host.parse::<IpAddr>() {
        Ok(address) => vec![address],
        Err(_) => lookup(host, port).await.map_err(|_| format!("cannot resolve {host}"))?.into_iter().map(|socket| socket.ip()).collect(),
    };
    if addresses.is_empty() {
        return Err(format!("cannot resolve {host}"));
    }
    if !allow_private && addresses.iter().any(|address| is_internal_address(*address)) {
        return Err(format!("{host} is not a public address; only servers on the internet can be reached"));
    }
    addresses.sort_unstable();
    addresses.dedup();
    ResolvedHost::new(host, port, addresses).map_err(|_| format!("cannot resolve {host}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn internal_hosts_are_refused_unless_allowed() {
        assert!(check_public_host("127.0.0.1", 443, false).await.is_err());
        assert!(check_public_host("::1", 443, false).await.is_err());
        assert!(check_public_host("10.1.2.3", 443, false).await.is_err());
        assert!(check_public_host("127.0.0.1", 443, true).await.is_ok());
        assert!(check_public_host("93.184.216.34", 443, false).await.is_ok());
        for host in ["2130706433", "0x7f000001", "127.1", "0177.0.0.1"] {
            assert!(check_public_host(host, 443, false).await.is_err(), "{host}");
            assert!(check_public_host(host, 443, true).await.is_ok(), "{host}");
        }
    }
}
