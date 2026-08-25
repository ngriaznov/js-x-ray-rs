//! Upstream: `src/ShadyLink.ts`
//!
//! `ipaddr.js` is replaced by `std::net::{Ipv4Addr, Ipv6Addr}` plus a direct
//! port of its `SpecialRanges` CIDR tables (see `IPV4_SPECIAL_RANGES` /
//! `IPV6_SPECIAL_RANGES` below) — only the boolean "is this address inside
//! any special (non-unicast) range" question is needed here, so the named
//! range table itself is not reproduced.

use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};
use url::Url;

use crate::collectable_set::CollectableSetRegistry;
use crate::estree::SourceLocation;
use crate::utils::{SourceArrayLocation, to_array_location};

static IPV4_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$").unwrap());

static SHADY_LINK_REGEXPS: LazyLock<[Regex; 2]> = LazyLock::new(|| {
    [
        Regex::new(r"(http[s]?://(bit\.ly|ipinfo\.io|httpbin\.org|api\.ipify\.org).*)$").unwrap(),
        Regex::new(
            r"(http[s]?://.*\.(link|xyz|tk|ml|ga|cf|gq|pw|top|club|mw|bd|ke|am|sbs|date|quest|cd|bid|ws|icu|cam|uno|email|stream))$",
        )
        .unwrap(),
    ]
});

/// IANA-registered URI schemes plus common app-specific ones, without the
/// trailing `:` that JS's `URL.protocol` carries.
static KNOWN_PROTOCOLS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "http", "https", "file", "data", "blob", "ftp", "ftps", "sftp", "tftp", "mailto", "xmpp",
        "irc", "ircs", "sip", "sips", "tel", "sms", "mms", "ssh", "telnet", "vnc", "rdp", "git",
        "svn", "cvs", "hg", "magnet", "ed2k", "torrent", "bitcoin", "ethereum", "ipfs", "ipns",
        "slack", "discord", "spotify", "steam", "skype", "zoommtg", "msteams", "vscode",
        "vscode-insiders", "jetbrains", "intent", "market", "itms", "itms-apps", "fb", "twitter",
        "instagram", "whatsapp", "tg", "ws", "wss", "ldap", "ldaps", "nntp", "news", "rtsp",
        "rtspu", "rtsps", "webcal", "feed", "podcast", "javascript", "about", "view-source",
        "acap", "cap", "cid", "mid", "urn", "tag", "dns", "geo", "ni", "nih",
    ]
    .into_iter()
    .collect()
});

/// Upstream `IPv4.prototype.SpecialRanges`: `(network, prefix length)` pairs.
const IPV4_SPECIAL_RANGES: &[(Ipv4Addr, u32)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8),
    (Ipv4Addr::new(255, 255, 255, 255), 32),
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    (Ipv4Addr::new(240, 0, 0, 0), 4),
    (Ipv4Addr::new(192, 175, 48, 0), 24),
    (Ipv4Addr::new(192, 31, 196, 0), 24),
    (Ipv4Addr::new(192, 52, 193, 0), 24),
];

/// Upstream `IPv6.prototype.SpecialRanges`: `(network, prefix length)` pairs.
const IPV6_SPECIAL_RANGES: &[(Ipv6Addr, u32)] = &[
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 0), 128),
    (Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0), 10),
    (Ipv6Addr::new(0xff00, 0, 0, 0, 0, 0, 0, 0), 8),
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1), 128),
    (Ipv6Addr::new(0xfc00, 0, 0, 0, 0, 0, 0, 0), 7),
    (Ipv6Addr::new(0, 0, 0, 0, 0, 0xffff, 0, 0), 96),
    (Ipv6Addr::new(0xfec0, 0, 0, 0, 0, 0, 0, 0), 10),
    (Ipv6Addr::new(0x100, 0, 0, 0, 0, 0, 0, 0), 64),
    (Ipv6Addr::new(0, 0, 0, 0, 0xffff, 0, 0, 0), 96),
    (Ipv6Addr::new(0x64, 0xff9b, 0, 0, 0, 0, 0, 0), 96),
    (Ipv6Addr::new(0x64, 0xff9b, 0x1, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2002, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2001, 0x2, 0, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2001, 0x3, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2001, 0x4, 0x112, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2620, 0x4f, 0x8000, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2001, 0x10, 0, 0, 0, 0, 0, 0), 28),
    (Ipv6Addr::new(0x2001, 0x20, 0, 0, 0, 0, 0, 0), 28),
    (Ipv6Addr::new(0x2001, 0x30, 0, 0, 0, 0, 0, 0), 28),
    (Ipv6Addr::new(0x5f00, 0, 0, 0, 0, 0, 0, 0), 16),
    (Ipv6Addr::new(0x2001, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x3fff, 0, 0, 0, 0, 0, 0, 0), 20),
];

fn ipv4_in_range(ip: Ipv4Addr, network: Ipv4Addr, prefix: u32) -> bool {
    let mask = (!0u32).checked_shl(32 - prefix).unwrap_or(0);
    ip.to_bits() & mask == network.to_bits() & mask
}

fn ipv6_in_range(ip: Ipv6Addr, network: Ipv6Addr, prefix: u32) -> bool {
    let mask = (!0u128).checked_shl(128 - prefix).unwrap_or(0);
    ip.to_bits() & mask == network.to_bits() & mask
}

/// Upstream `ipaddr.subnetMatch(address, SpecialRanges) !== "unicast"`.
fn is_unicast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !IPV4_SPECIAL_RANGES
            .iter()
            .any(|&(network, prefix)| ipv4_in_range(v4, network, prefix)),
        IpAddr::V6(v6) => !IPV6_SPECIAL_RANGES
            .iter()
            .any(|&(network, prefix)| ipv6_in_range(v6, network, prefix)),
    }
}

/// Upstream `ShadyLink.#isPrivateIPAddress`.
fn is_private_ip_address(ip_address: &str) -> bool {
    let Ok(ip) = ip_address.parse::<IpAddr>() else {
        return false;
    };
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };

    !is_unicast(ip)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShadyLinkResult {
    pub safe: bool,
    pub is_local_address: bool,
}

impl ShadyLinkResult {
    const fn safe() -> Self {
        Self { safe: true, is_local_address: false }
    }

    const fn unsafe_() -> Self {
        Self { safe: false, is_local_address: false }
    }

    const fn local_address() -> Self {
        Self { safe: false, is_local_address: true }
    }
}

pub struct IsUrlSafeOptions<'a> {
    pub collectable_set_registry: &'a mut CollectableSetRegistry,
    pub file: Option<&'a str>,
    pub location: Option<SourceLocation>,
    pub metadata: Option<&'a Map<String, Value>>,
}

pub struct IsIpAddressSafeOptions<'a> {
    pub collectable_set_registry: &'a mut CollectableSetRegistry,
    pub file: Option<&'a str>,
    pub location: Option<SourceLocation>,
    pub metadata: Option<&'a Map<String, Value>>,
}

pub struct ShadyLink;

impl ShadyLink {
    pub fn is_url_safe(input: &str, options: IsUrlSafeOptions<'_>) -> ShadyLinkResult {
        let IsUrlSafeOptions { collectable_set_registry, file, location, metadata } = options;

        let Ok(parsed_url) = Url::parse(input) else {
            return ShadyLinkResult::safe();
        };
        if !KNOWN_PROTOCOLS.contains(parsed_url.scheme()) {
            return ShadyLinkResult::safe();
        }

        let source_array_location = to_array_location(location);
        collectable_set_registry.add(
            "url",
            parsed_url.as_str(),
            file.map(str::to_owned),
            source_array_location,
            metadata.cloned(),
        );

        let hostname = parsed_url.host_str().unwrap_or("");

        if hostname == "localhost" {
            collectable_set_registry.add(
                "hostname",
                hostname,
                file.map(str::to_owned),
                source_array_location,
                metadata.cloned(),
            );

            return ShadyLinkResult::local_address();
        }

        if parsed_url.scheme() == "file" {
            if !hostname.is_empty() {
                collectable_set_registry.add(
                    "hostname",
                    hostname,
                    file.map(str::to_owned),
                    source_array_location,
                    metadata.cloned(),
                );
            }

            return ShadyLinkResult::safe();
        }

        let clean_hostname = hostname
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(hostname);

        if clean_hostname == "::" || Self::is_valid_ip_address(clean_hostname) {
            let result = Self::is_ip_address_safe_at(
                clean_hostname,
                collectable_set_registry,
                file,
                source_array_location,
                metadata,
            );
            if !result.safe {
                return result;
            }
        } else if !hostname.is_empty() {
            collectable_set_registry.add(
                "hostname",
                hostname,
                file.map(str::to_owned),
                source_array_location,
                metadata.cloned(),
            );
        }

        if parsed_url.scheme() != "https" {
            return ShadyLinkResult::unsafe_();
        }

        let is_shady_link = SHADY_LINK_REGEXPS.iter().any(|regex| regex.is_match(input));

        ShadyLinkResult { safe: !is_shady_link, is_local_address: false }
    }

    /// JS `\s` (no `u` flag) matches a slightly different Unicode set than
    /// `char::is_whitespace`; both agree on every separator that can appear
    /// in a URL hostname, which is the only input this ever sees.
    pub fn is_valid_ip_address(input: &str) -> bool {
        if input.len() > 45 || input.chars().any(char::is_whitespace) {
            return false;
        }
        if input == "::" {
            return false;
        }

        if let Some(captures) = IPV4_REGEX.captures(input) {
            return (1..=4)
                .all(|group| captures[group].parse::<u32>().is_ok_and(|octet| octet <= 255));
        }

        input.contains(':') && input.parse::<Ipv6Addr>().is_ok()
    }

    pub fn is_ip_address_safe(input: &str, options: IsIpAddressSafeOptions<'_>) -> ShadyLinkResult {
        let IsIpAddressSafeOptions { collectable_set_registry, file, location, metadata } =
            options;
        let source_array_location = to_array_location(location);

        Self::is_ip_address_safe_at(
            input,
            collectable_set_registry,
            file,
            source_array_location,
            metadata,
        )
    }

    fn is_ip_address_safe_at(
        input: &str,
        collectable_set_registry: &mut CollectableSetRegistry,
        file: Option<&str>,
        location: SourceArrayLocation,
        metadata: Option<&Map<String, Value>>,
    ) -> ShadyLinkResult {
        collectable_set_registry.add(
            "ip",
            input,
            file.map(str::to_owned),
            location,
            metadata.cloned(),
        );

        if is_private_ip_address(input) {
            return ShadyLinkResult::local_address();
        }

        ShadyLinkResult::safe()
    }
}
