//! The address of the client making a request, and whether it arrived over HTTPS.
//!
//! Milestone 1 workstream S owns this file.
//!
//! Forwarding headers are trivially forged, so they are believed only when the TCP peer
//! is one of the reverse proxies listed in `[security] trusted_proxies`. The chain in
//! `X-Forwarded-For` (or RFC 7239 `Forwarded`) is walked from the nearest hop outwards,
//! skipping trusted proxies; the first address that is not a trusted proxy is the client.
//! An unparsable hop stops the walk, so a client can never inject an address past a proxy.
//! A proxy that reports the client in a header of its own (`CF-Connecting-IP` from
//! Cloudflare) is read through `[security] client_ip_header`, again only from a trusted peer.
//!
//! A loopback or private peer that sends forwarding headers without being trusted is
//! almost always a reverse proxy or tunnel on the same host or network that nobody listed;
//! every client then appears as that proxy, so the first such request is logged once.

use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::atomic::{AtomicBool, Ordering},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{Extensions, HeaderMap, header, request::Parts},
};
use rbit_core::config::SecurityConfig;
use rbit_identity_models::sessions::ClientInfo;

use crate::AppState;

/// Reported when the server runs without connection info (e.g. in router unit tests).
const UNKNOWN_PEER: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

/// The client's IP address, used for rate limiting and audit records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    /// Resolves the client from the connection and, behind a trusted proxy, its headers.
    pub fn resolve(extensions: &Extensions, headers: &HeaderMap, security: &SecurityConfig) -> Self {
        let peer = peer(extensions);
        if untrusted_local_proxy(peer, headers, security) {
            warn_untrusted_proxy(peer);
        }
        Self(resolve_client(peer, headers, security))
    }
}

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(Self::resolve(&parts.extensions, &parts.headers, &state.config.security))
    }
}

/// The client's address and browser, recorded with new sessions and audit entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client(pub ClientInfo);

impl Client {
    pub fn ip(&self) -> Option<IpAddr> {
        self.0.ip
    }
}

impl FromRequestParts<AppState> for Client {
    type Rejection = Infallible;

    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let ClientIp(ip) = ClientIp::resolve(&parts.extensions, &parts.headers, &state.config.security);
        let user_agent = parts.headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();
        Ok(Self(ClientInfo { ip: Some(ip), user_agent }))
    }
}

/// Whether the request reached the site over HTTPS: the public URL is `https://`, or a
/// trusted proxy says so with `X-Forwarded-Proto: https` (or `Forwarded: proto=https`).
/// Decides the cookies' `Secure` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Https(pub bool);

impl FromRequestParts<AppState> for Https {
    type Rejection = Infallible;

    #[allow(unknown_lints, clippy::unused_async_trait_impl)]
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(Self(is_https(&parts.extensions, &parts.headers, &state.config)))
    }
}

/// Whether a request reached the site over HTTPS (see [`Https`]).
pub fn is_https(extensions: &Extensions, headers: &HeaderMap, config: &rbit_core::Config) -> bool {
    config.server.is_https() || forwarded_https(peer(extensions), headers, &config.security)
}

fn peer(extensions: &Extensions) -> IpAddr {
    extensions.get::<ConnectInfo<SocketAddr>>().map_or(UNKNOWN_PEER, |info| info.0.ip())
}

/// The client address for a connection from `peer` carrying `headers`.
pub fn resolve_client(peer: IpAddr, headers: &HeaderMap, security: &SecurityConfig) -> IpAddr {
    if !security.is_trusted_proxy(peer) {
        return peer;
    }
    if let Some(address) = security
        .client_ip_header
        .as_deref()
        .and_then(|name| header_values(headers, name).into_iter().next())
        .and_then(|value| parse_hop(&value))
    {
        return address;
    }
    let hops = forwarded_for_hops(headers);
    let mut client = peer;
    // Nearest hop first: each trusted address vouches for the one before it.
    for hop in hops.iter().rev() {
        if !security.is_trusted_proxy(client) {
            break;
        }
        match parse_hop(hop) {
            Some(address) => client = address,
            None => break,
        }
    }
    client
}

/// Headers that only a proxy or tunnel adds to a request.
const PROXY_HEADERS: &[&str] = &["x-forwarded-for", "forwarded", "x-real-ip", "cf-connecting-ip", "true-client-ip"];

/// Whether `peer` looks like an unlisted reverse proxy: a loopback or private address
/// that is not trusted but sends forwarding headers.
fn untrusted_local_proxy(peer: IpAddr, headers: &HeaderMap, security: &SecurityConfig) -> bool {
    let local = match peer {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(v6.is_loopback() || v6.is_unique_local(), |v4| v4.is_loopback() || v4.is_private()),
    };
    local && !security.is_trusted_proxy(peer) && PROXY_HEADERS.iter().any(|name| headers.contains_key(*name))
}

fn warn_untrusted_proxy(peer: IpAddr) {
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            %peer,
            "a request from {peer} carries forwarding headers, but {peer} is not in [security] trusted_proxies, so every client              is recorded as {peer}. List the reverse proxy or tunnel there (and set [security] client_ip_header = \"CF-Connecting-IP\"              behind Cloudflare)."
        );
    }
}

/// `true` when a trusted proxy reports that the original request used HTTPS.
pub fn forwarded_https(peer: IpAddr, headers: &HeaderMap, security: &SecurityConfig) -> bool {
    if !security.is_trusted_proxy(peer) {
        return false;
    }
    if let Some(proto) = header_values(headers, "x-forwarded-proto").into_iter().next() {
        return proto.eq_ignore_ascii_case("https");
    }
    forwarded_params(headers, "proto").into_iter().next().is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

/// The `X-Forwarded-For` chain (client first), or the `for=` values of `Forwarded`.
fn forwarded_for_hops(headers: &HeaderMap) -> Vec<String> {
    let hops = header_values(headers, "x-forwarded-for");
    if hops.is_empty() { forwarded_params(headers, "for") } else { hops }
}

/// Every comma-separated element of every occurrence of `name`, in order.
fn header_values(headers: &HeaderMap, name: &str) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

/// The values of parameter `key` in each element of RFC 7239 `Forwarded` headers.
fn forwarded_params(headers: &HeaderMap, key: &str) -> Vec<String> {
    header_values(headers, "forwarded")
        .iter()
        .filter_map(|element| {
            element.split(';').find_map(|pair| {
                let (name, value) = pair.trim().split_once('=')?;
                name.trim().eq_ignore_ascii_case(key).then(|| value.trim().trim_matches('"').to_owned())
            })
        })
        .collect()
}

/// An address as proxies write it: `192.0.2.1`, `192.0.2.1:4711`, `2001:db8::1` or
/// `[2001:db8::1]:4711`. Obfuscated identifiers (`unknown`, `_hidden`) are `None`.
fn parse_hop(hop: &str) -> Option<IpAddr> {
    if let Ok(address) = hop.parse::<IpAddr>() {
        return Some(address);
    }
    if let Ok(socket) = hop.parse::<SocketAddr>() {
        return Some(socket.ip());
    }
    hop.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).and_then(|inner| inner.parse().ok())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    fn security(trusted: &[&str]) -> SecurityConfig {
        SecurityConfig { trusted_proxies: trusted.iter().map(|n| n.parse().unwrap()).collect(), ..SecurityConfig::default() }
    }

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(*name, HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn untrusted_peers_cannot_spoof_their_address() {
        let forged = headers(&[("x-forwarded-for", "1.2.3.4"), ("forwarded", "for=5.6.7.8")]);
        assert_eq!(resolve_client(ip("203.0.113.9"), &forged, &security(&[])), ip("203.0.113.9"));
        assert_eq!(resolve_client(ip("203.0.113.9"), &forged, &security(&["10.0.0.0/8"])), ip("203.0.113.9"));
    }

    #[test]
    fn trusted_proxy_reports_the_client() {
        let config = security(&["10.0.0.0/8"]);
        let single = headers(&[("x-forwarded-for", "198.51.100.4")]);
        assert_eq!(resolve_client(ip("10.0.0.2"), &single, &config), ip("198.51.100.4"));
        // Without any header the proxy itself is the client.
        assert_eq!(resolve_client(ip("10.0.0.2"), &HeaderMap::new(), &config), ip("10.0.0.2"));
    }

    #[test]
    fn walks_the_chain_from_the_nearest_hop() {
        let config = security(&["10.0.0.0/8"]);
        // The client prepended a forged address; only the hop our proxy appended counts.
        let chain = headers(&[("x-forwarded-for", "6.6.6.6, 198.51.100.4, 10.0.0.7")]);
        assert_eq!(resolve_client(ip("10.0.0.2"), &chain, &config), ip("198.51.100.4"));
        // Repeated headers are one list.
        let repeated = headers(&[("x-forwarded-for", "6.6.6.6"), ("x-forwarded-for", "198.51.100.4")]);
        assert_eq!(resolve_client(ip("10.0.0.2"), &repeated, &config), ip("198.51.100.4"));
        // An unparsable hop stops the walk at the last address we can vouch for.
        let garbage = headers(&[("x-forwarded-for", "1.1.1.1, not-an-ip, 10.0.0.7")]);
        assert_eq!(resolve_client(ip("10.0.0.2"), &garbage, &config), ip("10.0.0.7"));
        // Every hop trusted: the outermost one is the best answer.
        let internal = headers(&[("x-forwarded-for", "10.1.1.1, 10.0.0.7")]);
        assert_eq!(resolve_client(ip("10.0.0.2"), &internal, &config), ip("10.1.1.1"));
    }

    #[test]
    fn understands_rfc_7239_and_ports() {
        let config = security(&["127.0.0.1"]);
        let forwarded = headers(&[("forwarded", r#"for=192.0.2.60;proto=https;by=203.0.113.43, for="[2001:db8:cafe::17]:4711""#)]);
        assert_eq!(resolve_client(ip("127.0.0.1"), &forwarded, &config), ip("2001:db8:cafe::17"));
        let with_port = headers(&[("x-forwarded-for", "192.0.2.60:51234")]);
        assert_eq!(resolve_client(ip("127.0.0.1"), &with_port, &config), ip("192.0.2.60"));
        let hidden = headers(&[("forwarded", "for=unknown")]);
        assert_eq!(resolve_client(ip("127.0.0.1"), &hidden, &config), ip("127.0.0.1"));
        // IPv4-mapped peers from dual-stack listeners are recognised as trusted.
        assert_eq!(resolve_client(ip("::ffff:127.0.0.1"), &with_port, &config), ip("192.0.2.60"));
    }

    #[test]
    fn client_ip_header_from_trusted_proxies_only() {
        let mut config = security(&["127.0.0.1"]);
        config.client_ip_header = Some("CF-Connecting-IP".into());
        // Cloudflare Tunnel: cloudflared connects from loopback and names the visitor.
        let tunnel = headers(&[("cf-connecting-ip", "2001:db8::42"), ("x-forwarded-for", "6.6.6.6, 198.51.100.4")]);
        assert_eq!(resolve_client(ip("127.0.0.1"), &tunnel, &config), ip("2001:db8::42"));
        // Anyone else sending it is ignored.
        assert_eq!(resolve_client(ip("203.0.113.9"), &tunnel, &config), ip("203.0.113.9"));
        // Missing or garbage: fall back to the forwarding chain.
        let chain = headers(&[("cf-connecting-ip", "nope"), ("x-forwarded-for", "198.51.100.4")]);
        assert_eq!(resolve_client(ip("127.0.0.1"), &chain, &config), ip("198.51.100.4"));
        // Without the setting the header means nothing.
        assert_eq!(resolve_client(ip("127.0.0.1"), &tunnel, &security(&["127.0.0.1"])), ip("198.51.100.4"));
    }

    #[test]
    fn notices_unlisted_local_proxies() {
        let forwarded = headers(&[("cf-connecting-ip", "198.51.100.4")]);
        assert!(untrusted_local_proxy(ip("127.0.0.1"), &forwarded, &security(&[])));
        assert!(untrusted_local_proxy(ip("::ffff:10.0.0.3"), &forwarded, &security(&[])));
        assert!(!untrusted_local_proxy(ip("127.0.0.1"), &forwarded, &security(&["127.0.0.1"])));
        assert!(!untrusted_local_proxy(ip("203.0.113.9"), &forwarded, &security(&[])));
        assert!(!untrusted_local_proxy(ip("127.0.0.1"), &HeaderMap::new(), &security(&[])));
    }

    #[test]
    fn https_only_from_trusted_proxies() {
        let config = security(&["127.0.0.1"]);
        let proto = headers(&[("x-forwarded-proto", "https")]);
        assert!(forwarded_https(ip("127.0.0.1"), &proto, &config));
        assert!(!forwarded_https(ip("192.0.2.1"), &proto, &config));
        assert!(!forwarded_https(ip("127.0.0.1"), &headers(&[("x-forwarded-proto", "http")]), &config));
        assert!(forwarded_https(ip("127.0.0.1"), &headers(&[("forwarded", "for=192.0.2.60;proto=https")]), &config));
        assert!(!forwarded_https(ip("127.0.0.1"), &HeaderMap::new(), &config));
    }
}
