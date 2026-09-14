//! Addresses: the ranges of `trusted_proxies` (deployment §3.3) and the one
//! rule for whose address a request carries. Behind a trusted proxy the
//! client is the rightmost `X-Forwarded-For` hop that is not itself a
//! trusted proxy; anywhere else the header is not believed, since anyone
//! can send one.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;

/// An address range, `10.0.0.0/8` or `fd00::/8`; a bare address is itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl FromStr for Cidr {
    type Err = String;

    fn from_str(s: &str) -> Result<Cidr, String> {
        let s = s.trim();
        let (addr, prefix) = match s.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (s, None),
        };
        let ip: IpAddr = addr
            .parse()
            .map_err(|_| format!("`{s}` is not an address or a range like 10.0.0.0/8"))?;
        let ip = ip.to_canonical();
        let bits: u8 = if ip.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= bits)
                .ok_or_else(|| format!("`{s}`: the prefix must be 0 to {bits}"))?,
            None => bits,
        };
        Ok(Cidr {
            network: mask(ip, prefix),
            prefix,
        })
    }
}

impl std::fmt::Display for Cidr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.network, self.prefix)
    }
}

impl Cidr {
    /// `0.0.0.0/0` or `::/0`: every address, which no list of proxies means.
    pub fn is_everything(&self) -> bool {
        self.prefix == 0
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        ip.is_ipv4() == self.network.is_ipv4() && mask(ip, self.prefix) == self.network
    }
}

fn mask(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let m = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(prefix))
            };
            IpAddr::V4(Ipv4Addr::from(u32::from(v4) & m))
        }
        IpAddr::V6(v6) => {
            let m = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(prefix))
            };
            IpAddr::V6(Ipv6Addr::from(u128::from(v6) & m))
        }
    }
}

pub fn is_trusted(ip: IpAddr, trusted: &[Cidr]) -> bool {
    trusted.iter().any(|range| range.contains(ip))
}

/// Whose address a request carries: the connection's, unless the connection
/// is a trusted proxy's, and then the client `X-Forwarded-For` names.
pub fn client_ip(
    peer: Option<IpAddr>,
    forwarded_for: Option<&str>,
    trusted: &[Cidr],
) -> Option<IpAddr> {
    let peer = peer?;
    if !is_trusted(peer, trusted) {
        return Some(peer);
    }
    let Some(header) = forwarded_for else {
        return Some(peer);
    };
    let hops: Vec<IpAddr> = header
        .split(',')
        .filter_map(|hop| parse_hop(hop.trim()))
        .collect();
    // From the right, the first hop that is not a trusted proxy is the client.
    if let Some(client) = hops.iter().rev().find(|hop| !is_trusted(**hop, trusted)) {
        return Some(*client);
    }
    // Every hop is a trusted proxy: the leftmost is where it began.
    hops.first().copied().or(Some(peer))
}

fn parse_hop(hop: &str) -> Option<IpAddr> {
    hop.parse::<IpAddr>()
        .ok()
        .or_else(|| hop.parse::<SocketAddr>().ok().map(|s| s.ip()))
        .map(|ip| ip.to_canonical())
}

/// Whether a bind address is loopback: the one case that needs no TLS.
pub fn binds_loopback(bind: &str) -> bool {
    if let Ok(addr) = bind.parse::<SocketAddr>() {
        return addr.ip().is_loopback();
    }
    match bind.to_socket_addrs() {
        Ok(addrs) => {
            let addrs: Vec<SocketAddr> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|a| a.ip().is_loopback())
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn ranges(list: &[&str]) -> Vec<Cidr> {
        list.iter().map(|r| r.parse().unwrap()).collect()
    }

    #[test]
    fn ranges_parse_and_contain() {
        let ten: Cidr = "10.0.0.0/8".parse().unwrap();
        assert!(ten.contains(ip("10.255.1.2")));
        assert!(!ten.contains(ip("11.0.0.1")));
        assert!(
            ten.contains(ip("::ffff:10.1.2.3")),
            "an IPv4-mapped address is that address"
        );
        let one: Cidr = "192.168.1.5".parse().unwrap();
        assert!(one.contains(ip("192.168.1.5")));
        assert!(!one.contains(ip("192.168.1.6")));
        let v6: Cidr = "fd00::/8".parse().unwrap();
        assert!(v6.contains(ip("fd12::1")));
        assert!(!v6.contains(ip("fe80::1")));
        assert!(!v6.contains(ip("10.0.0.1")));
        let all: Cidr = "0.0.0.0/0".parse().unwrap();
        assert!(all.contains(ip("203.0.113.9")));
        assert_eq!(
            "10.1.2.3/8".parse::<Cidr>().unwrap().to_string(),
            "10.0.0.0/8"
        );
        assert!("10.0.0.0/33".parse::<Cidr>().is_err());
        assert!("not-an-address".parse::<Cidr>().is_err());
    }

    #[test]
    fn without_trusted_proxies_the_connection_is_the_client() {
        assert_eq!(
            client_ip(Some(ip("203.0.113.9")), Some("198.51.100.7"), &[]),
            Some(ip("203.0.113.9"))
        );
    }

    #[test]
    fn a_header_from_an_untrusted_connection_is_not_believed() {
        let trusted = ranges(&["10.0.0.0/8"]);
        assert_eq!(
            client_ip(Some(ip("203.0.113.9")), Some("198.51.100.7"), &trusted),
            Some(ip("203.0.113.9"))
        );
    }

    #[test]
    fn behind_a_trusted_proxy_the_rightmost_untrusted_hop_is_the_client() {
        let trusted = ranges(&["10.0.0.0/8", "127.0.0.1"]);
        assert_eq!(
            client_ip(
                Some(ip("10.0.0.5")),
                Some("198.51.100.7, 203.0.113.9, 10.0.0.4"),
                &trusted
            ),
            Some(ip("203.0.113.9")),
            "the hop the trusted proxies received it from, not what the client claimed"
        );
        assert_eq!(
            client_ip(Some(ip("127.0.0.1")), Some("[2001:db8::7]:4321"), &trusted),
            Some(ip("2001:db8::7")),
            "a hop with a port is still an address"
        );
    }

    #[test]
    fn a_trusted_proxy_with_no_header_is_itself_the_client() {
        let trusted = ranges(&["10.0.0.0/8"]);
        assert_eq!(
            client_ip(Some(ip("10.0.0.5")), None, &trusted),
            Some(ip("10.0.0.5"))
        );
        assert_eq!(
            client_ip(Some(ip("10.0.0.5")), Some("garbage, more"), &trusted),
            Some(ip("10.0.0.5"))
        );
    }

    #[test]
    fn when_every_hop_is_trusted_the_first_is_the_client() {
        let trusted = ranges(&["10.0.0.0/8"]);
        assert_eq!(
            client_ip(Some(ip("10.0.0.5")), Some("10.9.9.9, 10.0.0.4"), &trusted),
            Some(ip("10.9.9.9"))
        );
    }

    #[test]
    fn no_connection_means_no_address() {
        assert_eq!(client_ip(None, Some("198.51.100.7"), &[]), None);
    }

    #[test]
    fn loopback_binds_are_recognised() {
        assert!(binds_loopback("127.0.0.1:8443"));
        assert!(binds_loopback("[::1]:8443"));
        assert!(binds_loopback("127.0.0.1:0"));
        assert!(!binds_loopback("0.0.0.0:8443"));
        assert!(!binds_loopback("[::]:8443"));
        assert!(!binds_loopback("192.168.1.10:8443"));
        assert!(!binds_loopback("not a bind"));
    }
}
