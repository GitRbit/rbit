//! `[security]`: sessions, passwords and abuse protection.
//!
//! Milestone 1 workstream S (security) owns this file.

use std::{fmt, net::IpAddr, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[expect(clippy::struct_excessive_bools, reason = "independent configuration switches")]
pub struct SecurityConfig {
    /// How long a session stays valid without activity, in days.
    pub session_lifetime_days: u32,
    /// How long a session stays valid at most, however active, in days. `0` means no limit.
    pub session_max_age_days: u32,
    /// Minimum password length accepted at sign-up.
    pub min_password_length: usize,
    /// Reverse proxies (CIDR networks, e.g. `127.0.0.1/32`, `10.0.0.0/8`, `::1/128`) whose
    /// `X-Forwarded-For`, `Forwarded` and `X-Forwarded-Proto` headers are believed.
    /// Empty (the default) means the TCP peer is always the client.
    pub trusted_proxies: Vec<ProxyNetwork>,
    /// Throttle sign-in, sign-up, two-factor codes, Git credentials and form posts per
    /// client. Only worth turning off for load tests.
    pub rate_limit: bool,
    /// The token that the first account's sign-up form asks for. Unset (the default), a
    /// random one is generated and logged when the server starts without any user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_token: Option<String>,
    /// Send `Strict-Transport-Security: max-age=31536000` when `server.public_url` is
    /// `https://`, so browsers refuse plain HTTP to this host for a year. Turn off only
    /// while moving the instance away from HTTPS.
    pub hsts: bool,
    /// Add `includeSubDomains` to that header. Off by default: it binds every subdomain
    /// of the host to HTTPS as well.
    pub hsts_include_subdomains: bool,
    /// Let pages show images from other sites (Markdown `![](https://...)`), which also
    /// tells those sites who viewed the page and when. Off restricts images to this
    /// instance and inline `data:` images; external ones (including a branding logo on
    /// another host) then do not load.
    pub external_images: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            session_lifetime_days: 30,
            session_max_age_days: 90,
            min_password_length: 10,
            trusted_proxies: Vec::new(),
            rate_limit: true,
            hsts: true,
            hsts_include_subdomains: false,
            external_images: true,
            setup_token: None,
        }
    }
}

impl SecurityConfig {
    /// Whether `peer` is one of the configured reverse proxies.
    pub fn is_trusted_proxy(&self, peer: IpAddr) -> bool {
        self.trusted_proxies.iter().any(|network| network.contains(peer))
    }
}

/// An IP network in CIDR notation. A bare address means a single host (`/32` or `/128`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProxyNetwork {
    address: IpAddr,
    prefix_len: u8,
}

/// Why a `trusted_proxies` entry was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid trusted proxy {0:?}: expected an address or CIDR network such as 10.0.0.0/8 or ::1/128")]
pub struct ProxyNetworkError(String);

impl ProxyNetwork {
    /// Whether `ip` lies inside the network. IPv4-mapped IPv6 addresses (`::ffff:a.b.c.d`)
    /// match IPv4 networks, since dual-stack listeners report IPv4 peers that way.
    pub fn contains(self, ip: IpAddr) -> bool {
        let ip = canonical(ip);
        match (self.address, ip) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => prefix_matches(&network.octets(), &ip.octets(), self.prefix_len),
            (IpAddr::V6(network), IpAddr::V6(ip)) => prefix_matches(&network.octets(), &ip.octets(), self.prefix_len),
            _ => false,
        }
    }
}

/// Unwraps IPv4-mapped IPv6 addresses.
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        IpAddr::V4(_) => ip,
    }
}

fn prefix_matches(network: &[u8], ip: &[u8], prefix_len: u8) -> bool {
    let full_bytes = usize::from(prefix_len / 8);
    let remaining_bits = prefix_len % 8;
    if network[..full_bytes] != ip[..full_bytes] {
        return false;
    }
    if remaining_bits == 0 {
        return true;
    }
    let mask = 0xff_u8 << (8 - remaining_bits);
    network[full_bytes] & mask == ip[full_bytes] & mask
}

impl FromStr for ProxyNetwork {
    type Err = ProxyNetworkError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let invalid = || ProxyNetworkError(value.to_owned());
        let (address, prefix) = match value.trim().split_once('/') {
            Some((address, prefix)) => (address, Some(prefix)),
            None => (value.trim(), None),
        };
        let address = canonical(address.parse::<IpAddr>().map_err(|_| invalid())?);
        let max = if address.is_ipv4() { 32 } else { 128 };
        let prefix_len = match prefix {
            None => max,
            Some(prefix) => prefix.parse::<u8>().ok().filter(|&len| len <= max).ok_or_else(invalid)?,
        };
        Ok(Self { address, prefix_len })
    }
}

impl TryFrom<String> for ProxyNetwork {
    type Error = ProxyNetworkError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ProxyNetwork> for String {
    fn from(network: ProxyNetwork) -> Self {
        network.to_string()
    }
}

impl fmt::Display for ProxyNetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    #[test]
    fn parses_networks_and_hosts() {
        assert_eq!("10.0.0.0/8".parse::<ProxyNetwork>().unwrap().to_string(), "10.0.0.0/8");
        assert_eq!("127.0.0.1".parse::<ProxyNetwork>().unwrap().to_string(), "127.0.0.1/32");
        assert_eq!("::1".parse::<ProxyNetwork>().unwrap().to_string(), "::1/128");
        assert_eq!(" fd00::/8 ".parse::<ProxyNetwork>().unwrap().to_string(), "fd00::/8");
        for bad in ["", "10.0.0.0/33", "::/129", "localhost", "10.0.0.0/-1", "10.0.0/8"] {
            assert!(bad.parse::<ProxyNetwork>().is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn matches_addresses_by_prefix() {
        let private: ProxyNetwork = "172.16.0.0/12".parse().unwrap();
        assert!(private.contains(ip("172.16.0.1")));
        assert!(private.contains(ip("172.31.255.255")));
        assert!(!private.contains(ip("172.32.0.1")));
        assert!(!private.contains(ip("::1")));

        let host: ProxyNetwork = "192.168.1.10".parse().unwrap();
        assert!(host.contains(ip("192.168.1.10")));
        assert!(host.contains(ip("::ffff:192.168.1.10")), "IPv4-mapped peers match");
        assert!(!host.contains(ip("192.168.1.11")));

        let v6: ProxyNetwork = "2001:db8::/32".parse().unwrap();
        assert!(v6.contains(ip("2001:db8:1::5")));
        assert!(!v6.contains(ip("2001:db9::1")));

        assert!("0.0.0.0/0".parse::<ProxyNetwork>().unwrap().contains(ip("8.8.8.8")));
    }

    #[test]
    fn config_deserialises_and_checks_peers() {
        #[derive(Deserialize)]
        struct Wrapper {
            security: SecurityConfig,
        }
        let json = r#"{"security": {"trusted_proxies": ["127.0.0.1", "10.0.0.0/8"]}}"#;
        let config: Wrapper = serde_json::from_str(json).unwrap();
        assert!(config.security.is_trusted_proxy(ip("10.1.2.3")));
        assert!(!config.security.is_trusted_proxy(ip("11.0.0.1")));
        assert!(config.security.rate_limit);
        assert!(config.security.hsts && !config.security.hsts_include_subdomains && config.security.external_images);
        let json = r#"{"security": {"hsts": false, "external_images": false}}"#;
        let config: Wrapper = serde_json::from_str(json).unwrap();
        assert!(!config.security.hsts && !config.security.external_images);
        assert!(serde_json::from_str::<Wrapper>(r#"{"security": {"trusted_proxies": ["nope"]}}"#).is_err());
        assert!(!SecurityConfig::default().is_trusted_proxy(ip("127.0.0.1")));
    }
}
