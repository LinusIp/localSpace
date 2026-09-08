//! The installed-harness registry: install, validate, instantiate, enable.
//!
//! Harnesses are held in a `BTreeMap` keyed by harness id, which is also the
//! canonical order tool descriptions are emitted in (§16.1) — sorting by id
//! rather than recency is what keeps the prompt prefix stable across focus
//! changes, and therefore what keeps the worker's KV prefix cache warm.

use crate::manifest::{Capabilities, DocKind, Manifest, SurfaceKind, Tier};
use crate::runtime::{native::NativeHarness, wasm::WasmHarness, HarnessRuntime, RuntimeConfig};
use crate::tools::ToolSet;
use anyhow::{bail, Context, Result};
use localspace_proto as proto;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Installed {
    pub manifest: Manifest,
    pub tools: ToolSet,
    pub dir: PathBuf,
    pub enabled: bool,
    /// Set when org policy refused a declared capability: the harness runs with
    /// that feature disabled rather than failing to install.
    pub degraded: Option<String>,
    pub runtime: Option<Box<dyn HarnessRuntime>>,
    /// One document per harness, named after it.
    pub doc_id: String,
    /// When the logic instance last did anything. Idle past `idle_unload`, it
    /// is dropped; the document stays; the next call re-instantiates it.
    pub last_used: std::time::Instant,
    pub idle_unload: std::time::Duration,
}

impl Installed {
    pub fn id(&self) -> &str {
        &self.manifest.harness.id
    }

    pub fn doc_kind(&self) -> proto::DocKind {
        self.manifest.contributes.doc.into()
    }

    pub fn summary(&self) -> proto::HarnessSummary {
        proto::HarnessSummary {
            id: self.manifest.harness.id.clone(),
            title: self.manifest.harness.title.clone(),
            version: self.manifest.harness.version.clone(),
            publisher: self.manifest.harness.publisher.clone(),
            tier: self.manifest.harness.tier.into(),
            views: self
                .manifest
                .contributes
                .views
                .iter()
                .map(|v| proto::ViewDesc {
                    id: v.id.clone(),
                    kind: v.kind.into(),
                    placement: v.placement.into(),
                    title: v
                        .title
                        .clone()
                        .unwrap_or_else(|| self.manifest.harness.title.clone()),
                })
                .collect(),
            tool_count: self.tools.tools.len(),
            front_door: self.tools.front_door().map(|t| t.name.clone()).collect(),
            has_context_provider: self.manifest.contributes.context_provider,
            doc_kind: self.doc_kind(),
            capabilities: self.manifest.capabilities.summary(),
            enabled: self.enabled,
            degraded: self.degraded.clone(),
            resources: self.manifest.resources.summary(),
            loaded: self.runtime.is_some(),
            accepts: self.manifest.contributes.accepts.clone(),
            produces: self.manifest.contributes.produces.clone(),
        }
    }

    /// Bytes of an `egui` surface module, for the Client's SurfaceRunner.
    pub fn surface_module(&self, view_id: &str) -> Result<Vec<u8>> {
        let view = self
            .manifest
            .view(view_id)
            .with_context(|| format!("`{}` has no view `{view_id}`", self.id()))?;
        if view.kind != SurfaceKind::Egui {
            bail!("view `{view_id}` is not an egui surface");
        }
        let module = view
            .module
            .as_ref()
            .context("egui view declares no module")?;
        let path = self.dir.join(module);
        std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
    }
}

/// Org policy: what capabilities may be granted at all.
#[derive(Debug, Clone)]
pub struct Policy {
    pub tier_b_permitted: bool,
    pub net_grantable: bool,
    pub max_fs: &'static str,
}

impl Default for Policy {
    fn default() -> Self {
        // Personal mode: Tier B allowed after the user approves the native_reason.
        Policy {
            tier_b_permitted: true,
            net_grantable: true,
            max_fs: "workspace",
        }
    }
}

impl Policy {
    pub fn organisation_default() -> Policy {
        // Regulated buyers keep Tier B off; the flagship harnesses are Tier A.
        Policy {
            tier_b_permitted: false,
            net_grantable: true,
            max_fs: "workspace",
        }
    }

    /// Apply policy to a package's declared capabilities.
    /// Returns the effective capabilities and a note when something was refused.
    pub fn apply(&self, caps: &Capabilities) -> (Capabilities, Option<String>) {
        let mut effective = caps.clone();
        let mut refused: Vec<String> = Vec::new();

        if !self.net_grantable && !caps.net.is_none() {
            effective.net = Default::default();
            refused.push("network access".into());
        }
        if self.max_fs == "none" && caps.fs != crate::manifest::FsCap::None {
            effective.fs = crate::manifest::FsCap::None;
            refused.push("filesystem access".into());
        }
        let note = if refused.is_empty() {
            None
        } else {
            Some(format!(
                "org policy disabled: {} — the harness runs without those features",
                refused.join(", ")
            ))
        };
        (effective, note)
    }
}

#[derive(Default)]
pub struct Registry {
    harnesses: BTreeMap<String, Installed>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry::default()
    }

    /// Canonical order: by harness id, always.
    pub fn iter(&self) -> impl Iterator<Item = &Installed> {
        self.harnesses.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Installed> {
        self.harnesses.values_mut()
    }

    pub fn get(&self, id: &str) -> Option<&Installed> {
        self.harnesses.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Installed> {
        self.harnesses.get_mut(id)
    }

    pub fn len(&self) -> usize {
        self.harnesses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.harnesses.is_empty()
    }

    pub fn remove(&mut self, id: &str) -> Option<Installed> {
        self.harnesses.remove(id)
    }

    /// Which harness owns a fully-qualified tool name.
    pub fn owner_of(&self, tool: &str) -> Option<&Installed> {
        self.iter().find(|h| h.tools.get(tool).is_some())
    }

    /// Read and validate a package. Does not instantiate its runtime.
    pub fn stage(dir: &Path, policy: &Policy) -> Result<Installed> {
        let manifest = Manifest::load(dir)?;

        if manifest.harness.tier == Tier::Native && !policy.tier_b_permitted {
            bail!(
                "`{}` is a Tier B (native) harness and Tier B is disabled here",
                manifest.harness.id
            );
        }

        let tools_path = manifest
            .contributes
            .tools
            .clone()
            .unwrap_or_else(|| "tools.json".into());
        let tools_text = std::fs::read_to_string(dir.join(&tools_path))
            .with_context(|| format!("reading {}", dir.join(&tools_path).display()))?;
        let tools = ToolSet::parse(&tools_text)?;

        // Every tool name must be namespaced by the harness, so two packages
        // cannot collide in the model's tool list.
        for t in &tools.tools {
            if !t.name.contains('.') {
                bail!(
                    "tool `{}` is not namespaced (expected e.g. `canvas.add_shape`)",
                    t.name
                );
            }
        }

        let (effective, degraded) = policy.apply(&manifest.capabilities);
        let mut manifest = manifest;
        manifest.capabilities = effective;

        let doc_id = manifest.harness.id.replace('.', "_");
        let idle_unload = manifest.resources.idle_unload_duration();
        Ok(Installed {
            manifest,
            tools,
            dir: dir.to_path_buf(),
            enabled: true,
            degraded,
            runtime: None,
            doc_id,
            last_used: std::time::Instant::now(),
            idle_unload,
        })
    }

    /// Bring a staged harness's logic online.
    pub fn instantiate(
        installed: &mut Installed,
        services: std::sync::Arc<dyn crate::runtime::CoreServices>,
    ) -> Result<()> {
        let cfg = RuntimeConfig {
            harness_id: installed.manifest.harness.id.clone(),
            capabilities: installed.manifest.capabilities.clone(),
            services,
            logic_memory_mb: installed.manifest.resources.memory_mb.logic,
        };

        let logic = installed
            .manifest
            .contributes
            .logic
            .clone()
            .unwrap_or_else(|| match installed.manifest.harness.tier {
                Tier::Wasm => "logic.wasm".into(),
                Tier::Native => "logic".into(),
            });
        let path = installed.dir.join(&logic);

        let runtime: Box<dyn HarnessRuntime> = match installed.manifest.harness.tier {
            Tier::Wasm => {
                let bytes = std::fs::read(&path)
                    .with_context(|| format!("reading harness logic {}", path.display()))?;
                Box::new(WasmHarness::load(&bytes, cfg)?)
            }
            Tier::Native => Box::new(NativeHarness::spawn(&path, &[], cfg)?),
        };
        installed.runtime = Some(runtime);
        installed.last_used = std::time::Instant::now();

        // The module's own tool list must match the package's, or the manifest is lying.
        if let Some(rt) = installed.runtime.as_mut() {
            match rt.tools_json() {
                Ok(text) => {
                    if let Ok(reported) = ToolSet::parse(&text) {
                        let declared: Vec<&str> =
                            installed.tools.tools.iter().map(|t| t.name.as_str()).collect();
                        for t in &reported.tools {
                            if !declared.contains(&t.name.as_str()) {
                                bail!(
                                    "`{}` reports tool `{}` which is not in tools.json",
                                    installed.manifest.harness.id,
                                    t.name
                                );
                            }
                        }
                    }
                }
                Err(e) => bail!("`{}` failed to report its tools: {e}", installed.manifest.harness.id),
            }
        }
        Ok(())
    }

    /// Bring a harness's logic online if it is not, and note the use.
    ///
    /// Nothing is resident that is not in use (spec §1.2): instances are made on
    /// first call and dropped by `unload_idle`, so this runs on every call path.
    pub fn ensure_runtime(
        installed: &mut Installed,
        services: std::sync::Arc<dyn crate::runtime::CoreServices>,
    ) -> Result<()> {
        if installed.runtime.is_none() {
            Registry::instantiate(installed, services)?;
        }
        installed.last_used = std::time::Instant::now();
        Ok(())
    }

    /// Drop logic instances idle past their declared `idle_unload`.
    /// Returns the ids that were unloaded, for the trace.
    pub fn unload_idle(&mut self, now: std::time::Instant) -> Vec<String> {
        let mut dropped = Vec::new();
        for h in self.harnesses.values_mut() {
            if h.runtime.is_some() && now.duration_since(h.last_used) >= h.idle_unload {
                h.runtime = None;
                dropped.push(h.manifest.harness.id.clone());
            }
        }
        dropped
    }

    pub fn insert(&mut self, installed: Installed) {
        self.harnesses
            .insert(installed.manifest.harness.id.clone(), installed);
    }

    /// Install every package directory under `root`.
    pub fn load_dir(
        &mut self,
        root: &Path,
        policy: &Policy,
        services: std::sync::Arc<dyn crate::runtime::CoreServices>,
    ) -> Vec<(String, anyhow::Error)> {
        let mut failures = Vec::new();
        let Ok(entries) = std::fs::read_dir(root) else {
            return failures;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.join("harness.toml").exists() {
                continue;
            }
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            match Registry::stage(&dir, policy) {
                Ok(mut installed) => {
                    if let Err(e) = Registry::instantiate(&mut installed, services.clone()) {
                        failures.push((name, e));
                        continue;
                    }
                    self.insert(installed);
                }
                Err(e) => failures.push((name, e)),
            }
        }
        failures
    }
}

/// Documents a harness owns, so Core can create them at install.
pub fn doc_kind_of(manifest: &Manifest) -> proto::DocKind {
    match manifest.contributes.doc {
        DocKind::Crdt => proto::DocKind::Crdt,
        DocKind::Blob => proto::DocKind::Blob,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{FsCap, NetCap};

    #[test]
    fn policy_disables_a_refused_capability_rather_than_failing_the_install() {
        let policy = Policy {
            tier_b_permitted: false,
            net_grantable: false,
            max_fs: "workspace",
        };
        let caps = Capabilities {
            net: NetCap::Allowlist {
                allowlist: vec!["tiles.example.com".into()],
                reason: "map tiles".into(),
            },
            fs: FsCap::Workspace,
            ..Default::default()
        };
        let (effective, note) = policy.apply(&caps);
        assert!(effective.net.is_none(), "network must be stripped");
        assert_eq!(effective.fs, FsCap::Workspace, "fs was within policy");
        assert!(note.unwrap().contains("network access"));
    }

    #[test]
    fn a_policy_that_grants_everything_leaves_no_note() {
        let (_, note) = Policy::default().apply(&Capabilities::default());
        assert!(note.is_none());
    }

    #[test]
    fn harnesses_iterate_in_canonical_id_order_not_insertion_order() {
        // §16.1: tool descriptions are emitted sorted by harness id, never by
        // recency, so the prompt prefix survives a focus change.
        let mut reg = Registry::new();
        for id in ["io.z.zed", "io.a.alpha", "io.m.mid"] {
            reg.insert(fake(id));
        }
        let order: Vec<&str> = reg.iter().map(|h| h.id()).collect();
        assert_eq!(order, vec!["io.a.alpha", "io.m.mid", "io.z.zed"]);
    }

    fn fake(id: &str) -> Installed {
        let manifest = Manifest::parse(&format!(
            r#"
[harness]
id = "{id}"
version = "1.0.0"
api = "^1.0"
title = "T"
publisher = "p"

[contributes]
tools = "tools.json"
"#
        ))
        .unwrap();
        Installed {
            manifest,
            tools: ToolSet::default(),
            dir: PathBuf::from("."),
            enabled: true,
            degraded: None,
            runtime: None,
            doc_id: id.replace('.', "_"),
            last_used: std::time::Instant::now(),
            idle_unload: std::time::Duration::from_secs(300),
        }
    }
}
