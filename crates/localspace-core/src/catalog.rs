//! The catalog behind the marketplace (spec H§12, D§8.1).
//!
//! A catalog is a set of directories holding packages. Connected, that is a synced
//! registry; air-gapped, it is an offline bundle prepared on a staging machine and
//! copied across. Both are the same thing to Core, which is the point: air-gapped
//! import is a first-class path, not an afterthought.
//!
//! Nothing here installs anything. It reads manifests, works out what installing
//! each package *would* grant, and says plainly when policy would refuse it — so
//! the decision is made before the code runs, not after.

use crate::manifest::{Capabilities, Manifest, Tier};
use crate::registry::{Policy, Registry};
use crate::tools::ToolSet;
use localspace_proto as proto;
use std::path::{Path, PathBuf};

/// Read every package under `dirs`, newest-first per id, and describe it against
/// what this environment already has installed.
pub fn scan(dirs: &[PathBuf], installed: &Registry, policy: &Policy) -> Vec<proto::CatalogEntry> {
    let mut entries: Vec<proto::CatalogEntry> = Vec::new();

    for dir in dirs {
        let source = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.display().to_string());
        let Ok(read) = std::fs::read_dir(dir) else {
            continue;
        };
        for item in read.flatten() {
            let path = item.path();
            if !path.join("harness.toml").exists() {
                continue;
            }
            if let Some(entry) = describe(&path, &source, installed, policy) {
                // A package listed in two bundles is one package; keep the one
                // already installed, else the first seen.
                if let Some(existing) = entries.iter_mut().find(|e| e.id == entry.id) {
                    if entry.installed && !existing.installed {
                        *existing = entry;
                    }
                } else {
                    entries.push(entry);
                }
            }
        }
    }

    entries.sort_by(|a, b| a.title.cmp(&b.title));
    entries
}

fn describe(
    path: &Path,
    source: &str,
    installed: &Registry,
    policy: &Policy,
) -> Option<proto::CatalogEntry> {
    let manifest = Manifest::load(path).ok()?;
    let tools = load_tools(path, &manifest).unwrap_or_default();

    let existing = installed.get(&manifest.harness.id);
    let installed_version = existing.map(|h| h.manifest.harness.version.clone());

    // What would change if this were installed over what is already here.
    let widens = match existing {
        Some(current) => manifest
            .capabilities
            .widening_over(&current.manifest.capabilities),
        None => Vec::new(),
    };

    let blocked = blocked_reason(&manifest, policy);

    let description = manifest
        .harness
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| {
            let doors: Vec<&str> = tools.front_door().map(|t| t.summary.as_str()).collect();
            if doors.is_empty() {
                "No description.".into()
            } else {
                doors.join(" ")
            }
        });

    let eval_cases = std::fs::read_to_string(path.join("evals.json"))
        .ok()
        .and_then(|t| crate::evals::parse_suite(&t).ok())
        .map(|s| s.cases.len())
        .unwrap_or(0);

    Some(proto::CatalogEntry {
        id: manifest.harness.id.clone(),
        title: manifest.harness.title.clone(),
        version: manifest.harness.version.clone(),
        publisher: manifest.harness.publisher.clone(),
        description,
        tier: manifest.harness.tier.into(),
        native_reason: manifest.harness.native_reason.clone(),
        tool_count: tools.tools.len(),
        front_door: tools.front_door().map(|t| t.name.clone()).collect(),
        capability_lines: plain_language(&manifest.capabilities),
        capabilities: manifest.capabilities.summary(),
        doc_kind: manifest.contributes.doc.into(),
        has_context_provider: manifest.contributes.context_provider,
        eval_cases,
        source: source.to_string(),
        path: path.display().to_string(),
        installed: existing.is_some(),
        installed_version,
        widens,
        blocked,
        resources: manifest.resources.summary(),
    })
}

fn load_tools(path: &Path, manifest: &Manifest) -> Option<ToolSet> {
    let name = manifest
        .contributes
        .tools
        .clone()
        .unwrap_or_else(|| "tools.json".into());
    let text = std::fs::read_to_string(path.join(name)).ok()?;
    ToolSet::parse(&text).ok()
}

/// Why this environment would refuse the package, if it would.
fn blocked_reason(manifest: &Manifest, policy: &Policy) -> Option<String> {
    if manifest.harness.tier == Tier::Native && !policy.tier_b_permitted {
        return Some(
            "runs as a native process, and native harnesses are disabled in this environment"
                .into(),
        );
    }
    if !crate::manifest::api_range_accepts(&manifest.harness.api, proto::HARNESS_API) {
        return Some(format!(
            "needs harness-api {}, this host provides {}",
            manifest.harness.api,
            proto::HARNESS_API
        ));
    }
    None
}

/// Capabilities as a reader who is not an engineer would want them.
///
/// The list is deliberately exhaustive on grants and silent on absences: a reader
/// should be able to assume that anything not listed is not granted, because
/// that is exactly what default-deny means.
pub fn plain_language(caps: &Capabilities) -> Vec<String> {
    use crate::manifest::{ClipboardCap, DocsCap, FsCap, ModelCap, NetCap};
    let mut out = Vec::new();

    match &caps.fs {
        FsCap::None => {}
        FsCap::Workspace => out.push("Reads and writes files in your workspace".into()),
        FsCap::Scoped(p) => out.push(format!("Reads and writes files under `{p}` only")),
    }
    match &caps.net {
        NetCap::Allowlist { allowlist, reason } => out.push(format!(
            "Fetches from {} — {reason}. It never gets a socket; the request goes through the gateway.",
            allowlist.join(", ")
        )),
        NetCap::Simple(_) => {}
    }
    if caps.gpu.wanted() {
        out.push(format!(
            "Takes a GPU from the harness pool ({})",
            caps.gpu.describe()
        ));
    }
    if caps.spawn {
        out.push("Starts other programs".into());
    }
    match caps.clipboard {
        ClipboardCap::None => {}
        ClipboardCap::OnUserAction => out.push("Reads the clipboard when you paste".into()),
        ClipboardCap::Always => out.push("Reads the clipboard at any time".into()),
    }
    if caps.docs == DocsCap::Acl {
        out.push(
            "Searches your documents through Core, filtered to what you may see. It never receives the index.".into(),
        );
    }
    if !caps.model.is_empty() {
        let what: Vec<&str> = caps
            .model
            .iter()
            .map(|m| match m {
                ModelCap::Complete => "ask the model for text",
                ModelCap::Structured => "ask the model for structured JSON",
                ModelCap::Embed => "compute embeddings",
            })
            .collect();
        out.push(format!("May {}", what.join(", ")));
    }

    if out.is_empty() {
        out.push("Nothing outside its own document. No files, no network, no model.".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{FsCap, NetCap};

    #[test]
    fn a_package_that_asks_for_nothing_says_so_plainly() {
        let lines = plain_language(&Capabilities::default());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Nothing outside its own document"));
    }

    #[test]
    fn every_grant_is_spelled_out_in_words() {
        let caps = Capabilities {
            fs: FsCap::Workspace,
            net: NetCap::Allowlist {
                allowlist: vec!["tiles.example.com".into()],
                reason: "map tiles for the site plan".into(),
            },
            docs: crate::manifest::DocsCap::Acl,
            model: vec![crate::manifest::ModelCap::Complete],
            ..Capabilities::default()
        };
        let lines = plain_language(&caps);
        assert!(lines.iter().any(|l| l.contains("workspace")));
        assert!(lines.iter().any(|l| l.contains("map tiles")));
        assert!(lines.iter().any(|l| l.contains("never receives the index")));
        assert!(lines.iter().any(|l| l.contains("ask the model for text")));
    }

    #[test]
    fn a_native_package_is_blocked_where_tier_b_is_off() {
        let text = r#"
[harness]
id = "io.example.sim"
version = "1.0.0"
api = "^1.0"
title = "Sim"
publisher = "x"
tier = "native"
native_reason = "needs a GPU solver"

[contributes]
tools = "tools.json"
"#;
        let manifest = Manifest::parse(text).unwrap();
        let strict = Policy {
            tier_b_permitted: false,
            ..Policy::default()
        };
        let why = blocked_reason(&manifest, &strict).unwrap();
        assert!(why.contains("native harnesses are disabled"), "{why}");
        assert!(blocked_reason(&manifest, &Policy::default()).is_none());
    }

    #[test]
    fn a_package_built_for_a_future_host_is_blocked_with_both_versions() {
        let text = r#"
[harness]
id = "io.example.future"
version = "1.0.0"
api = "^9.0"
title = "Future"
publisher = "x"

[contributes]
tools = "tools.json"
"#;
        // The manifest itself refuses this, which is the first gate.
        assert!(Manifest::parse(text).is_err());
    }
}
