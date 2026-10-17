//! Classification of IP addresses for outbound requests made on a user's behalf.
//!
//! Webhook deliveries and pull-mirror fetches go to URLs that users choose; unless the
//! instance allows private addresses, every address they resolve to is checked with
//! [`is_internal_address`] so neither can be used to reach the server's internal network.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Whether `ip` belongs to a range outbound requests (webhook deliveries, mirror fetches)
/// must never reach unless the instance allows private addresses: loopback, private (RFC 1918), shared/CGNAT (`100.64.0.0/10`),
/// link-local, unique-local (`fc00::/7`), multicast, unspecified, broadcast, reserved and
/// documentation or benchmarking ranges. IPv6 forms that embed an IPv4 address
/// (IPv4-mapped, IPv4-compatible, NAT64, 6to4) are judged by that address.
pub fn is_internal_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_internal_v4(v4),
        IpAddr::V6(v6) => is_internal_v6(v6),
    }
}

fn is_internal_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    a == 0                                    // "this network", including 0.0.0.0
        || a == 10                            // private
        || a == 127                           // loopback
        || (a == 100 && (64..128).contains(&b)) // shared address space (CGNAT)
        || (a == 169 && b == 254)             // link-local
        || (a == 172 && (16..32).contains(&b)) // private
        || (a == 192 && b == 0 && c == 0)     // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)     // documentation (TEST-NET-1)
        || (a == 192 && b == 88 && c == 99)   // 6to4 relay anycast
        || (a == 192 && b == 168)             // private
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || (a == 198 && b == 51 && c == 100)  // documentation (TEST-NET-2)
        || (a == 203 && b == 0 && c == 113)   // documentation (TEST-NET-3)
        || a >= 224 // multicast, reserved and broadcast
}

fn is_internal_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(v4) = embedded_v4(ip) {
        return is_internal_v4(v4);
    }
    ip.is_unspecified()
        || ip.is_loopback()
        || (segments[0] & 0xfe00) == 0xfc00 // unique-local
        || (segments[0] & 0xffc0) == 0xfe80 // link-local
        || (segments[0] & 0xffc0) == 0xfec0 // site-local (deprecated)
        || (segments[0] & 0xff00) == 0xff00 // multicast
        || segments[..3] == [0x0064, 0xff9b, 1] // local-use IPv4/IPv6 translation (RFC 8215)
        || (segments[0] == 0x2001 && segments[1] == 0x0db8) // documentation
        || (segments[0] & 0xfff0) == 0x3ff0 // documentation (3fff::/20)
        || (segments[0] == 0x0100 && segments[1..4] == [0, 0, 0]) // discard-only
}

/// The IPv4 address inside IPv4-mapped (`::ffff:a.b.c.d`), IPv4-compatible (`::a.b.c.d`),
/// NAT64 (`64:ff9b::a.b.c.d`) and 6to4 (`2002:aabb:ccdd::`) addresses.
fn embedded_v4(ip: Ipv6Addr) -> Option<Ipv4Addr> {
    let v4 = |high: u16, low: u16| {
        let ([a, b], [c, d]) = (high.to_be_bytes(), low.to_be_bytes());
        Ipv4Addr::new(a, b, c, d)
    };
    match ip.segments() {
        [0, 0, 0, 0, 0, 0xffff, high, low] | [0x0064, 0xff9b, 0, 0, 0, 0, high, low] | [0x2002, high, low, ..] => Some(v4(high, low)),
        // `::` and `::1` are handled as IPv6 themselves.
        [0, 0, 0, 0, 0, 0, high, low] if high != 0 || low > 1 => Some(v4(high, low)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn internal(address: &str) -> bool {
        is_internal_address(address.parse().unwrap())
    }

    #[test]
    fn classifies_ipv4_ranges() {
        for address in [
            "0.0.0.0",
            "0.1.2.3",
            "10.0.0.1",
            "10.255.255.255",
            "100.64.0.1",
            "100.127.255.255",
            "127.0.0.1",
            "127.255.255.254",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "198.51.100.7",
            "203.0.113.9",
            "224.0.0.1",
            "239.255.255.250",
            "240.0.0.1",
            "255.255.255.255",
        ] {
            assert!(internal(address), "{address} is internal");
        }
        for address in [
            "1.1.1.1",
            "8.8.8.8",
            "100.63.255.255",
            "100.128.0.0",
            "172.15.255.255",
            "172.32.0.0",
            "192.169.0.1",
            "198.20.0.1",
            "203.0.114.1",
            "223.255.255.255",
        ] {
            assert!(!internal(address), "{address} is public");
        }
    }

    #[test]
    fn classifies_ipv6_ranges() {
        for address in [
            "::",
            "::1",
            "fc00::1",
            "fd12:3456:789a::1",
            "fe80::1",
            "febf::1",
            "fec0::1",
            "ff02::1",
            "ff05::1:3",
            "2001:db8::1",
            "3fff::1",
            "100::1",
            "64:ff9b:1::a00:1",
            "64:ff9b:1:ffff:ffff:ffff:ffff:ffff",
        ] {
            assert!(internal(address), "{address} is internal");
        }
        for address in ["2606:4700:4700::1111", "2001:4860:4860::8888", "2a00:1450:4001::200e", "fe00::1"] {
            assert!(!internal(address), "{address} is public");
        }
    }

    #[test]
    fn judges_embedded_ipv4_by_the_embedded_address() {
        for address in [
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "::0.0.0.2",
            "::0.0.255.255",
            "64:ff9b::10.0.0.1",
            "2002:c0a8:0101::1",
            "2002:7f00:0001::",
        ] {
            assert!(internal(address), "{address} embeds an internal address");
        }
        for address in ["::ffff:8.8.8.8", "64:ff9b::1.1.1.1", "2002:0808:0808::1"] {
            assert!(!internal(address), "{address} embeds a public address");
        }
    }

    #[test]
    fn mirror_upstream_cases_are_classified() {
        for public in ["93.184.216.34", "1.1.1.1", "2606:4700::1111", "::ffff:8.8.8.8", "64:ff9b::808:808"] {
            assert!(!internal(public), "{public} is public");
        }
        for private in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "100.64.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::127.0.0.1",
            "64:ff9b::a00:1",
        ] {
            assert!(internal(private), "{private} is internal");
        }
    }
}
