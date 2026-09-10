//! The installed-harness registry: install, validate, instantiate, enable.
//!
//! Harnesses are held in a `BTreeMap` keyed by harness id, which is also the
//! canonical order tool descriptions are emitted in (§16.1) — sorting by id
//! rather than recency is what keeps the prompt prefix stable across focus
//! changes, and therefore what keeps the worker's KV prefix cache warm.

use crate::manifest::{Capabilities, DocKind, Manifest, SurfaceKind, Tier};
use crate::runtime::{HarnessRuntime, RuntimeConfig, native::NativeHarness, wasm::WasmHarness};
use crate::tools::ToolSet;
use anyhow::{Context, Result, bail};
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
            kind: self.manifest.package.kind.label().to_string(),
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

    /// One file of a `web` view. `path` is relative to the directory that
    /// holds the entry module and may not leave it: the harness's origin
    /// serves that directory and nothing else of the package or the machine.
    pub fn surface_file(&self, view_id: &str, path: &str) -> Result<(Vec<u8>, String)> {
        let view = self
            .manifest
            .view(view_id)
            .with_context(|| format!("`{}` has no view `{view_id}`", self.id()))?;
        if view.kind != SurfaceKind::Web {
            bail!("view `{view_id}` is not a web surface");
        }
        let module = view
            .module
            .as_ref()
            .context("web view declares no module")?;
        let root = self
            .dir
            .join(module)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| self.dir.clone());
        let rel = path.trim_start_matches(['/', '\\']);
        let clean = rel
            .split(['/', '\\'])
            .all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.contains(':'));
        if rel.is_empty() || !clean {
            bail!("`{path}` is not a file under the view's directory");
        }
        let root_c = root
            .canonicalize()
            .with_context(|| format!("the view's directory {} is missing", root.display()))?;
        let full_c = root
            .join(rel)
            .canonicalize()
            .with_context(|| format!("no file `{path}` in view `{view_id}`"))?;
        if !full_c.starts_with(&root_c) || !full_c.is_file() {
            bail!("`{path}` is not a file under the view's directory");
        }
        let bytes =
            std::fs::read(&full_c).with_context(|| format!("reading {}", full_c.display()))?;
        Ok((bytes, mime_for(rel).to_string()))
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

        // A library or types package (spec §17.1) has no surface and no tools;
        // it is linked into harnesses that depend on it, not run.
        let tools = if manifest.package.kind.is_harness() {
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
            tools
        } else {
            ToolSet::default()
        };

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
        // Only a harness has logic to run. A library is linked at install time
        // into the harnesses that depend on it — composition is not built yet,
        // see docs/STATUS.md — so for now it is present, resolved and locked,
        // and costs nothing at runtime.
        if !installed.manifest.package.kind.is_harness() {
            installed.last_used = std::time::Instant::now();
            return Ok(());
        }
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
                        let declared: Vec<&str> = installed
                            .tools
                            .tools
                            .iter()
                            .map(|t| t.name.as_str())
                            .collect();
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
                Err(e) => bail!(
                    "`{}` failed to report its tools: {e}",
                    installed.manifest.harness.id
                ),
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

    #[test]
    fn a_web_view_serves_only_the_files_beside_its_entry_module() {
        // The harness's origin serves the directory holding `index.js` and
        // nothing above it: not the manifest, not the logic, not the machine.
        let dir = std::env::temp_dir().join(format!(
            "localspace-web-surface-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("ui/web/assets")).unwrap();
        std::fs::write(dir.join("ui/web/index.js"), b"export {}").unwrap();
        std::fs::write(dir.join("ui/web/assets/a.css"), b"p{}").unwrap();
        std::fs::write(dir.join("harness.toml"), b"secret").unwrap();
        let mut h = fake("io.t.web");
        h.manifest = Manifest::parse(
            r#"
[harness]
id = "io.t.web"
version = "1.0.0"
api = "^1.0"
title = "T"
publisher = "p"

[contributes]
tools = "tools.json"
views = [{ id = "web", kind = "web", module = "ui/web/index.js", placement = "main", title = "W" }]
"#,
        )
        .unwrap();
        h.dir = dir.clone();

        let (bytes, mime) = h.surface_file("web", "index.js").unwrap();
        assert_eq!(bytes, b"export {}");
        assert_eq!(mime, "text/javascript; charset=utf-8");
        let (_, mime) = h.surface_file("web", "assets/a.css").unwrap();
        assert_eq!(mime, "text/css; charset=utf-8");
        let (_, mime) = h.surface_file("web", "/assets/a.css").unwrap();
        assert_eq!(mime, "text/css; charset=utf-8");

        for escape in [
            "../harness.toml",
            "assets/../../harness.toml",
            "..\\harness.toml",
            "",
            ".",
            "assets",
            "C:/Windows/win.ini",
        ] {
            assert!(
                h.surface_file("web", escape).is_err(),
                "`{escape}` must not be served"
            );
        }
        assert!(
            h.surface_file("board", "index.js").is_err(),
            "not a web view"
        );
        std::fs::remove_dir_all(&dir).ok();
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

/// The media type a web surface's file is served with, from its extension.
pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod web_surface_tests {
    use super::*;

    #[test]
    fn media_types_follow_the_extension() {
        assert_eq!(mime_for("index.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("a/b/c.wasm"), "application/wasm");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }
}
