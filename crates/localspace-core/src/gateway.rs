//! The egress gateway — the only component in Core with a socket (spec §8).
//!
//! No harness ever opens one. `web.search` and `web.fetch` are Core tools; a
//! harness with a `net` allowlist gets a proxied fetch through this same path.
//! Fetched content is cached into the workspace as a cited document and is always
//! treated as data, never as instruction.

use anyhow::{Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub mode: proto::NetworkMode,
    /// Admin ceiling: users may only go stricter.
    pub ceiling: proto::NetworkMode,
    /// Wildcard domains, e.g. `*.wikipedia.org`.
    pub allowlist: Vec<String>,
    pub blocklist: Vec<String>,
    /// Always permitted for ingestion connectors: the intranet is not the internet.
    pub intranet: Vec<String>,
    pub search_url: Option<String>,
    pub max_requests_per_session: u32,
    pub max_bytes_per_session: u64,
    pub timeout: Duration,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        GatewayConfig {
            mode: proto::NetworkMode::Ask,
            ceiling: proto::NetworkMode::Ask,
            allowlist: Vec::new(),
            blocklist: Vec::new(),
            intranet: Vec::new(),
            search_url: None,
            max_requests_per_session: 100,
            max_bytes_per_session: 64 * 1024 * 1024,
            timeout: Duration::from_secs(20),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Egress {
    Allowed,
    /// `ask` mode, first use of this domain this session.
    NeedsApproval(String),
    Denied(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fetched {
    pub url: String,
    pub content: String,
    pub fetched_at_ms: u64,
    pub bytes: usize,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

pub struct Gateway {
    pub config: GatewayConfig,
    approved: HashSet<String>,
    requests: u32,
    bytes: u64,
    /// Every outbound request, for the audit trail.
    pub log: Vec<(String, bool, usize)>,
}

impl Gateway {
    pub fn new(config: GatewayConfig) -> Gateway {
        Gateway {
            config,
            approved: HashSet::new(),
            requests: 0,
            bytes: 0,
            log: Vec::new(),
        }
    }

    /// Users may only choose a mode at or below the admin ceiling.
    pub fn set_mode(&mut self, mode: proto::NetworkMode) -> proto::NetworkMode {
        let clamped = if mode.rank() > self.config.ceiling.rank() {
            self.config.ceiling
        } else {
            mode
        };
        self.config.mode = clamped;
        clamped
    }

    pub fn approve_domain(&mut self, domain: &str) {
        self.approved.insert(domain.to_ascii_lowercase());
    }

    /// Decide whether a URL may be fetched, without fetching it.
    pub fn check(&self, url: &str) -> Egress {
        let Some(host) = host_of(url) else {
            return Egress::Denied(format!("`{url}` is not a fetchable URL"));
        };

        if self.config.mode == proto::NetworkMode::Airgapped
            && !matches_any(&host, &self.config.intranet)
        {
            return Egress::Denied("this environment is airgapped; no egress is possible".into());
        }
        if matches_any(&host, &self.config.blocklist) {
            return Egress::Denied(format!("`{host}` is on the blocklist"));
        }
        if !self.config.allowlist.is_empty()
            && !matches_any(&host, &self.config.allowlist)
            && !matches_any(&host, &self.config.intranet)
        {
            return Egress::Denied(format!("`{host}` is not on the domain allowlist"));
        }
        if self.requests >= self.config.max_requests_per_session {
            return Egress::Denied("this session's request quota is spent".into());
        }
        if self.bytes >= self.config.max_bytes_per_session {
            return Egress::Denied("this session's byte quota is spent".into());
        }

        match self.config.mode {
            proto::NetworkMode::Airgapped => Egress::Allowed, // intranet only, checked above
            proto::NetworkMode::Online => Egress::Allowed,
            proto::NetworkMode::Ask => {
                if self.approved.contains(&host) {
                    Egress::Allowed
                } else {
                    Egress::NeedsApproval(host)
                }
            }
        }
    }

    /// Fetch one URL. The caller must have satisfied `check` first.
    pub fn fetch(&mut self, url: &str, mode: &str) -> Result<Fetched> {
        match self.check(url) {
            Egress::Allowed => {}
            Egress::NeedsApproval(d) => {
                anyhow::bail!("`{d}` has not been approved for this session")
            }
            Egress::Denied(why) => anyhow::bail!("{why}"),
        }

        let clean = strip_tracking(url);
        let res = ureq::get(&clean)
            .config()
            .timeout_global(Some(self.config.timeout))
            .build()
            .header("User-Agent", "localSpace/0.1 (+gateway)")
            .call()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("fetching {clean}"));

        let raw = match res {
            Ok(mut r) => r
                .body_mut()
                .read_to_string()
                .map_err(|e| anyhow::anyhow!("{e}"))?,
            Err(e) => {
                self.log.push((clean.clone(), false, 0));
                return Err(e);
            }
        };

        self.requests += 1;
        self.bytes += raw.len() as u64;
        self.log.push((clean.clone(), true, raw.len()));

        let title = extract_title(&raw);
        let content = if mode == "raw" {
            raw.clone()
        } else {
            html_to_text(&raw)
        };

        Ok(Fetched {
            url: clean,
            bytes: raw.len(),
            content,
            fetched_at_ms: crate::dag::now_ms(),
            title,
        })
    }

    /// Search through the configured backend (SearXNG for organisations).
    pub fn search(&mut self, query: &str, site: Option<&str>) -> Result<Vec<SearchHit>> {
        let base = self
            .config
            .search_url
            .clone()
            .context("no search backend is configured for this environment")?;
        let q = match site {
            Some(s) => format!("{query} site:{s}"),
            None => query.to_string(),
        };
        let url = format!(
            "{}/search?format=json&q={}",
            base.trim_end_matches('/'),
            urlencode(&q)
        );

        match self.check(&url) {
            Egress::Allowed | Egress::NeedsApproval(_) => {}
            Egress::Denied(why) => anyhow::bail!("{why}"),
        }

        let mut res = ureq::get(&url)
            .config()
            .timeout_global(Some(self.config.timeout))
            .build()
            .call()
            .map_err(|e| anyhow::anyhow!("{e}"))
            .with_context(|| format!("searching via {base}"))?;
        let body: serde_json::Value = res
            .body_mut()
            .read_json()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        self.requests += 1;

        Ok(body["results"]
            .as_array()
            .map(|a| {
                a.iter()
                    .take(8)
                    .map(|r| SearchHit {
                        title: r["title"].as_str().unwrap_or_default().to_string(),
                        url: r["url"].as_str().unwrap_or_default().to_string(),
                        snippet: r["content"].as_str().unwrap_or_default().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// `*.example.com` matches `a.example.com` and `example.com`.
/// A bare `10.0.0.0/8` matches by prefix, which is enough for intranet ranges.
pub fn matches_any(host: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let p = p.to_ascii_lowercase();
        if let Some(suffix) = p.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else if let Some((prefix, _)) = p.split_once('/') {
            // CIDR-ish: match the leading octets that are non-zero.
            let lead: Vec<&str> = prefix.split('.').take_while(|o| *o != "0").collect();
            !lead.is_empty() && host.starts_with(&lead.join("."))
        } else {
            host == p
        }
    })
}

const TRACKING: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "gclid",
    "fbclid",
    "mc_cid",
    "mc_eid",
    "ref",
    "ref_src",
];

pub fn strip_tracking(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let (query, fragment) = match query.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (query, None),
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("");
            !TRACKING.contains(&key)
        })
        .filter(|p| !p.is_empty())
        .collect();

    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}

pub fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let end = lower[open_end..].find("</title>")? + open_end;
    let title = html[open_end..end].trim();
    if title.is_empty() {
        None
    } else {
        Some(decode_entities(title))
    }
}

/// Strip markup, dropping the contents of `script` and `style` entirely.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let bytes = html.as_bytes();
    let lower = html.to_lowercase();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Skip whole script/style elements.
            for tag in ["script", "style", "noscript", "svg"] {
                let open = format!("<{tag}");
                if lower[i..].starts_with(&open) {
                    let close = format!("</{tag}>");
                    if let Some(end) = lower[i..].find(&close) {
                        i += end + close.len();
                    } else {
                        i = bytes.len();
                    }
                    out.push(' ');
                    break;
                }
            }
            if i >= bytes.len() {
                break;
            }
            if bytes[i] != b'<' {
                continue;
            }
            // Block-level tags become line breaks so the text stays readable.
            let is_block = [
                "<p", "<br", "<div", "<li", "<h1", "<h2", "<h3", "<tr", "</p", "</div",
            ]
            .iter()
            .any(|t| lower[i..].starts_with(t));
            match html[i..].find('>') {
                Some(end) => i += end + 1,
                None => break,
            }
            out.push(if is_block { '\n' } else { ' ' });
        } else {
            let ch = html[i..].chars().next().unwrap_or(' ');
            out.push(ch);
            i += ch.len_utf8();
        }
    }

    let decoded = decode_entities(&out);
    // Collapse the whitespace the tag stripping left behind.
    let mut text = String::with_capacity(decoded.len());
    let mut blank_run = 0;
    for line in decoded.lines() {
        let trimmed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        text.push_str(&trimmed);
        text.push('\n');
    }
    text.trim().to_string()
}

fn decode_entities(s: &str) -> String {
    s.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gw(mode: proto::NetworkMode, allowlist: &[&str]) -> Gateway {
        Gateway::new(GatewayConfig {
            mode,
            ceiling: proto::NetworkMode::Online,
            allowlist: allowlist.iter().map(|s| s.to_string()).collect(),
            intranet: vec!["*.corp.example".into()],
            ..Default::default()
        })
    }

    #[test]
    fn airgapped_denies_the_internet_but_not_the_intranet() {
        let g = gw(proto::NetworkMode::Airgapped, &[]);
        assert!(matches!(
            g.check("https://example.com/x"),
            Egress::Denied(_)
        ));
        assert_eq!(g.check("https://wiki.corp.example/page"), Egress::Allowed);
    }

    #[test]
    fn ask_mode_needs_one_approval_per_domain_per_session() {
        let mut g = gw(proto::NetworkMode::Ask, &[]);
        assert_eq!(
            g.check("https://arxiv.org/abs/1"),
            Egress::NeedsApproval("arxiv.org".into())
        );
        g.approve_domain("arxiv.org");
        assert_eq!(g.check("https://arxiv.org/abs/2"), Egress::Allowed);
        // A different domain still asks.
        assert!(matches!(
            g.check("https://elsewhere.com/x"),
            Egress::NeedsApproval(_)
        ));
    }

    #[test]
    fn online_mode_still_respects_the_allowlist() {
        let g = gw(proto::NetworkMode::Online, &["*.wikipedia.org"]);
        assert_eq!(
            g.check("https://en.wikipedia.org/wiki/Rust"),
            Egress::Allowed
        );
        assert!(matches!(
            g.check("https://evil.example/x"),
            Egress::Denied(_)
        ));
    }

    #[test]
    fn a_user_cannot_relax_past_the_admin_ceiling() {
        let mut g = Gateway::new(GatewayConfig {
            mode: proto::NetworkMode::Airgapped,
            ceiling: proto::NetworkMode::Ask,
            ..Default::default()
        });
        assert_eq!(
            g.set_mode(proto::NetworkMode::Online),
            proto::NetworkMode::Ask
        );
        assert_eq!(
            g.set_mode(proto::NetworkMode::Airgapped),
            proto::NetworkMode::Airgapped
        );
    }

    #[test]
    fn wildcards_and_intranet_ranges_match_as_expected() {
        let pats = vec!["*.wikipedia.org".to_string(), "arxiv.org".to_string()];
        assert!(matches_any("en.wikipedia.org", &pats));
        assert!(matches_any("wikipedia.org", &pats));
        assert!(matches_any("arxiv.org", &pats));
        assert!(!matches_any("notwikipedia.org", &pats));

        let intranet = vec!["10.0.0.0/8".to_string()];
        assert!(matches_any("10.1.4.22", &intranet));
        assert!(!matches_any("192.168.1.1", &intranet));
    }

    #[test]
    fn tracking_parameters_are_stripped_but_real_ones_survive() {
        assert_eq!(
            strip_tracking("https://x.com/a?id=7&utm_source=news&fbclid=abc"),
            "https://x.com/a?id=7"
        );
        assert_eq!(
            strip_tracking("https://x.com/a?utm_source=news"),
            "https://x.com/a"
        );
        assert_eq!(strip_tracking("https://x.com/a"), "https://x.com/a");
    }

    #[test]
    fn html_becomes_readable_text_without_script_contents() {
        let html = r#"<html><head><title>Risk &amp; Reward</title>
            <style>body{color:red}</style></head>
            <body><h1>Heading</h1><p>First para.</p>
            <script>alert("ignore me")</script>
            <p>Second&nbsp;para.</p></body></html>"#;
        let text = html_to_text(html);
        assert!(text.contains("Heading"));
        assert!(text.contains("First para."));
        assert!(text.contains("Second para."));
        assert!(!text.contains("alert"), "script content leaked: {text}");
        assert!(!text.contains("color:red"), "style content leaked: {text}");
        assert_eq!(extract_title(html).as_deref(), Some("Risk & Reward"));
    }

    #[test]
    fn quotas_close_the_session_down() {
        let mut g = gw(proto::NetworkMode::Online, &[]);
        g.config.max_requests_per_session = 0;
        assert!(matches!(g.check("https://example.com"), Egress::Denied(_)));
    }
}
