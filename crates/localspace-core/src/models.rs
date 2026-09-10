//! The model catalog (architecture v2 §4.4): a curated list with the numbers
//! the placement planner needs, downloaded from Hugging Face on request into
//! the data directory, plus files the user brought along. Downloads are
//! provisioning egress, separate from the runtime gateway, and refused when
//! the environment is air-gapped.

use crate::engine::EventSink;
use crate::planner::{self, MoeLayout, PlanRequest, TensorMap, Verdict};
use crate::profile::Machine;
use anyhow::{anyhow, bail, Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The catalog shipped with the build, as a file beside the harnesses.
pub const BUILT_IN: &str = include_str!("../../../models/catalog.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogFile {
    pub version: u32,
    #[serde(default)]
    pub notes: String,
    pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub family: String,
    pub params_b: f32,
    #[serde(default)]
    pub active_params_b: f32,
    #[serde(default)]
    pub quant: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub license_url: String,
    pub bytes: u64,
    pub context_len: u32,
    /// A Hugging Face repository, or empty for an imported file.
    #[serde(default)]
    pub repo: String,
    pub files: Vec<String>,
    #[serde(default = "yes")]
    pub supports_tools: bool,
    #[serde(default)]
    pub notes: String,
    pub tensor: Option<Tensor>,
    /// Set for an imported file: where it lives.
    #[serde(default)]
    pub path: Option<PathBuf>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tensor {
    pub core_bytes: u64,
    #[serde(default)]
    pub routed_expert_bytes: u64,
    pub layers: u32,
    pub moe: Option<MoeLayout>,
    /// K and V per token at FP16 across all layers; 4 KB a layer when unknown.
    #[serde(default)]
    pub kv_bytes_per_token_fp16: u64,
}

impl CatalogModel {
    pub fn tensor_map(&self) -> Option<TensorMap> {
        let t = self.tensor.as_ref()?;
        Some(TensorMap {
            model_id: self.id.clone(),
            core_bytes: t.core_bytes,
            routed_expert_bytes: t.routed_expert_bytes,
            layers: t.layers,
            moe: t.moe,
            kv_bytes_per_token_fp16: if t.kv_bytes_per_token_fp16 > 0 {
                t.kv_bytes_per_token_fp16
            } else {
                t.layers as u64 * 4096
            },
        })
    }

    pub fn source(&self) -> String {
        if self.path.is_some() {
            "import".into()
        } else {
            format!("hf:{}", self.repo)
        }
    }
}

pub struct Catalog {
    models: Vec<CatalogModel>,
    /// Where downloaded files and `imports.json` live.
    dir: PathBuf,
}

impl Catalog {
    /// The built-in catalog, plus `catalog.json` in `catalog_dir` when it
    /// exists (an organisation's own list), plus the user's imports.
    pub fn load(catalog_dir: Option<&Path>, models_dir: &Path) -> Catalog {
        let mut models = parse(BUILT_IN).map(|c| c.models).unwrap_or_default();
        if let Some(dir) = catalog_dir
            && let Ok(text) = std::fs::read_to_string(dir.join("catalog.json")) {
                match parse(&text) {
                    Ok(extra) => {
                        for m in extra.models {
                            models.retain(|x| x.id != m.id);
                            models.push(m);
                        }
                    }
                    Err(e) => tracing::warn!("ignoring {}: {e}", dir.join("catalog.json").display()),
                }
            }
        let catalog = Catalog {
            models,
            dir: models_dir.to_path_buf(),
        };
        let mut catalog = catalog;
        for m in catalog.imports() {
            catalog.models.retain(|x| x.id != m.id);
            catalog.models.push(m);
        }
        catalog
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn get(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// The path `llama-server` loads: the first file, once every file is here.
    pub fn installed_path(&self, id: &str) -> Option<PathBuf> {
        let m = self.get(id)?;
        if let Some(p) = &m.path {
            return p.is_file().then(|| p.clone());
        }
        let paths: Vec<PathBuf> = m.files.iter().map(|f| self.dir.join(f)).collect();
        let all_here = paths.iter().all(|p| p.is_file())
            && m.files.iter().all(|f| !self.dir.join(format!("{f}.part")).exists());
        (all_here && !paths.is_empty()).then(|| paths[0].clone())
    }

    /// Every entry with the planner's verdict for this machine and its state on disk.
    pub fn entries(
        &self,
        machine: &Machine,
        downloads: &HashMap<String, proto::DownloadState>,
        loaded: Option<&str>,
    ) -> Vec<proto::ModelCatalogEntry> {
        self.models
            .iter()
            .map(|m| {
                let (verdict, tok_s, first_ms, summary, notes) = match m.tensor_map() {
                    Some(map) => {
                        let req = PlanRequest {
                            context_len: m.context_len.min(16384),
                            ..PlanRequest::default()
                        };
                        let p = planner::plan(&map, machine, &req);
                        (
                            p.verdict.label().to_string(),
                            p.estimated_tok_s,
                            p.first_token_ms,
                            p.summary(),
                            p.notes.clone(),
                        )
                    }
                    None => (
                        "unknown".into(),
                        0.0,
                        0.0,
                        "no tensor map: the planner cannot place this model".into(),
                        Vec::new(),
                    ),
                };
                let download = downloads.get(&m.id).cloned();
                proto::ModelCatalogEntry {
                    id: m.id.clone(),
                    title: m.title.clone(),
                    family: m.family.clone(),
                    params_b: m.params_b,
                    active_params_b: if m.active_params_b > 0.0 { m.active_params_b } else { m.params_b },
                    quant: m.quant.clone(),
                    license: m.license.clone(),
                    license_url: m.license_url.clone(),
                    bytes: m.bytes,
                    context_len: m.context_len,
                    source: m.source(),
                    files: m.files.clone(),
                    installed: self.installed_path(&m.id).is_some(),
                    loaded: loaded == Some(m.id.as_str()),
                    download,
                    verdict,
                    estimated_tok_s: tok_s,
                    first_token_ms: first_ms,
                    plan_summary: summary,
                    plan_notes: notes,
                    supports_tools: m.supports_tools,
                    notes: m.notes.clone(),
                }
            })
            .collect()
    }

    /// Whether this model is placeable at all here, with the planner's reasons.
    pub fn placement(&self, id: &str, machine: &Machine) -> Result<Option<(TensorMap, planner::PlacementPlan)>> {
        let m = self.get(id).ok_or_else(|| anyhow!("no model `{id}` in the catalog"))?;
        let Some(map) = m.tensor_map() else { return Ok(None) };
        let req = PlanRequest {
            context_len: m.context_len.min(16384),
            ..PlanRequest::default()
        };
        let plan = planner::plan(&map, machine, &req);
        if plan.verdict == Verdict::DoesNotFit {
            bail!("{} does not fit this machine: {}", m.title, plan.notes.join("; "));
        }
        Ok(Some((map, plan)))
    }

    // --- imports -----------------------------------------------------------

    fn imports_path(&self) -> PathBuf {
        self.dir.join("imports.json")
    }

    fn imports(&self) -> Vec<CatalogModel> {
        std::fs::read_to_string(self.imports_path())
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<CatalogModel>>(&t).ok())
            .unwrap_or_default()
    }

    /// Bring a GGUF file into the catalog where it is; nothing is copied.
    /// Without a tensor map the planner treats it as dense, sized by the file.
    pub fn import(&mut self, path: &Path) -> Result<CatalogModel> {
        let meta = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
        if !meta.is_file() {
            bail!("{} is not a file", path.display());
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".into());
        let id = format!("import-{}", sanitize(&name));
        let model = CatalogModel {
            id: id.clone(),
            title: name.clone(),
            family: "imported".into(),
            params_b: (meta.len() as f32 / 0.55e9).max(0.1),
            active_params_b: 0.0,
            quant: "as given".into(),
            license: "as licensed to you".into(),
            license_url: String::new(),
            bytes: meta.len(),
            context_len: 8192,
            repo: String::new(),
            files: vec![path.file_name().unwrap().to_string_lossy().to_string()],
            supports_tools: true,
            notes: "Imported in place. Its size stands in for a tensor map, as a dense model.".into(),
            tensor: Some(Tensor {
                core_bytes: meta.len(),
                routed_expert_bytes: 0,
                layers: 32,
                moe: None,
                kv_bytes_per_token_fp16: 0,
            }),
            path: Some(path.to_path_buf()),
        };
        let mut imports = self.imports();
        imports.retain(|m| m.id != id);
        imports.push(model.clone());
        std::fs::create_dir_all(&self.dir).ok();
        std::fs::write(self.imports_path(), serde_json::to_string_pretty(&imports)?)?;
        self.models.retain(|m| m.id != id);
        self.models.push(model.clone());
        Ok(model)
    }

    // --- downloads ---------------------------------------------------------

    /// Fetch every file of a catalog model on its own thread, reporting into
    /// `downloads` and through `sink`. One download per model at a time.
    pub fn download(
        &self,
        id: &str,
        downloads: Arc<Mutex<HashMap<String, proto::DownloadState>>>,
        sink: EventSink,
    ) -> Result<()> {
        let m = self.get(id).ok_or_else(|| anyhow!("no model `{id}` in the catalog"))?.clone();
        if m.repo.is_empty() {
            bail!("{} is an imported file; there is nothing to download", m.title);
        }
        {
            let mut d = downloads.lock().unwrap();
            if matches!(d.get(id), Some(s) if s.stage == "downloading" || s.stage == "verifying") {
                bail!("{} is already downloading", m.title);
            }
            d.insert(
                id.to_string(),
                proto::DownloadState {
                    done_bytes: 0,
                    total_bytes: m.bytes,
                    stage: "downloading".into(),
                },
            );
        }
        let dir = self.dir.clone();
        std::fs::create_dir_all(&dir)?;
        std::thread::Builder::new()
            .name(format!("download-{id}"))
            .spawn(move || {
                let id = m.id.clone();
                let result = fetch_all(&m, &dir, &downloads, &sink);
                let stage = match &result {
                    Ok(()) => "done".to_string(),
                    Err(e) => format!("failed: {e:#}"),
                };
                let state = {
                    let mut d = downloads.lock().unwrap();
                    let entry = d.entry(id.clone()).or_insert(proto::DownloadState {
                        done_bytes: 0,
                        total_bytes: m.bytes,
                        stage: String::new(),
                    });
                    entry.stage = stage.clone();
                    if result.is_ok() {
                        entry.done_bytes = entry.total_bytes;
                    }
                    entry.clone()
                };
                sink(proto::Event::ModelProgress {
                    id: id.clone(),
                    done_bytes: state.done_bytes,
                    total_bytes: state.total_bytes,
                    stage: stage.clone(),
                });
                sink(proto::Event::Notice {
                    level: if result.is_ok() { proto::NoticeLevel::Info } else { proto::NoticeLevel::Error },
                    text: match result {
                        Ok(()) => format!("{} downloaded; load it from Models", m.title),
                        Err(e) => format!("{} download {e:#}", m.title),
                    },
                });
            })
            .context("spawning the download")?;
        Ok(())
    }
}

fn fetch_all(
    m: &CatalogModel,
    dir: &Path,
    downloads: &Arc<Mutex<HashMap<String, proto::DownloadState>>>,
    sink: &EventSink,
) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(None)
        .timeout_connect(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut done_before: u64 = 0;
    let mut total: u64 = m.bytes;
    for (i, file) in m.files.iter().enumerate() {
        let dest = dir.join(file);
        if dest.is_file() && !dir.join(format!("{file}.part")).exists() {
            // Already here from an earlier run; count it and move on.
            done_before += std::fs::metadata(&dest).map(|x| x.len()).unwrap_or(0);
            continue;
        }
        let url = format!("https://huggingface.co/{}/resolve/main/{}", m.repo, file);
        let mut res = agent
            .get(&url)
            .call()
            .with_context(|| format!("fetching {url}"))?;
        if res.status() != 200 {
            bail!("{url} answered {}", res.status());
        }
        let length = res.body().content_length();
        if let Some(len) = length {
            // The first file's real size corrects the estimate for single-file
            // models; multi-file models keep the catalog's total.
            if m.files.len() == 1 {
                total = len;
            }
        }
        let part = dir.join(format!("{file}.part"));
        let mut out = std::fs::File::create(&part).with_context(|| format!("creating {}", part.display()))?;
        let mut reader = res.body_mut().as_reader();
        let mut buf = vec![0u8; 1 << 20];
        let mut written: u64 = 0;
        let mut last = Instant::now();
        loop {
            let n = reader.read(&mut buf).with_context(|| format!("reading {url}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            written += n as u64;
            if last.elapsed() > Duration::from_millis(500) {
                last = Instant::now();
                let done = done_before + written;
                downloads.lock().unwrap().insert(
                    m.id.clone(),
                    proto::DownloadState {
                        done_bytes: done,
                        total_bytes: total.max(done),
                        stage: "downloading".into(),
                    },
                );
                sink(proto::Event::ModelProgress {
                    id: m.id.clone(),
                    done_bytes: done,
                    total_bytes: total.max(done),
                    stage: format!("downloading {} of {}", i + 1, m.files.len()),
                });
            }
        }
        out.flush()?;
        drop(out);
        if let Some(len) = length
            && written != len {
                bail!("{file}: got {written} of {len} bytes");
            }
        std::fs::rename(&part, &dest).with_context(|| format!("finishing {}", dest.display()))?;
        done_before += written;
    }
    Ok(())
}

fn parse(text: &str) -> Result<CatalogFile> {
    serde_json::from_str(text).context("parsing the model catalog")
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c.to_ascii_lowercase() } else { '-' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laptop() -> Machine {
        Machine {
            gpus: vec![4],
            ram_gb: 15,
            cores: 16,
            nvme_gbps: 3.0,
            pcie_gbps: 8.0,
            amx: false,
            avx512: false,
            unified_memory: false,
        }
    }

    fn w32() -> Machine {
        Machine {
            gpus: vec![32],
            ram_gb: 64,
            cores: 16,
            nvme_gbps: 6.0,
            pcie_gbps: 25.0,
            amx: false,
            avx512: false,
            unified_memory: false,
        }
    }

    #[test]
    fn the_built_in_catalog_parses_and_plans_for_both_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        let entries = catalog.entries(&laptop(), &HashMap::new(), None);
        assert!(entries.len() >= 5, "{}", entries.len());
        let small = entries.iter().find(|e| e.id == "qwen2.5-3b-instruct-q4_k_m").unwrap();
        assert_eq!(small.verdict, "resident", "{}", small.plan_summary);
        assert!(!small.installed && !small.loaded && small.download.is_none());
        let big = entries.iter().find(|e| e.id == "gpt-oss-120b-mxfp4").unwrap();
        // A 4 GB laptop cannot hold it: the planner may still offer to stream the
        // experts from NVMe, at a rate nobody would call interactive.
        assert!(
            matches!(big.verdict.as_str(), "streaming" | "does not fit"),
            "on a 4 GB laptop: {}",
            big.plan_summary
        );
        assert!(big.estimated_tok_s < 15.0, "far below the gate: {}", big.plan_summary);

        let entries = catalog.entries(&w32(), &HashMap::new(), None);
        let big = entries.iter().find(|e| e.id == "gpt-oss-120b-mxfp4").unwrap();
        assert!(matches!(big.verdict.as_str(), "hybrid" | "streaming"), "{}", big.plan_summary);
        assert!(big.estimated_tok_s > 0.0);
    }

    #[test]
    fn a_model_is_installed_only_when_every_file_is_here_and_finished() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        assert!(catalog.installed_path("gpt-oss-120b-mxfp4").is_none());
        let files = &catalog.get("gpt-oss-120b-mxfp4").unwrap().files;
        for f in files {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        std::fs::write(dir.path().join(format!("{}.part", files[2])), b"").unwrap();
        assert!(catalog.installed_path("gpt-oss-120b-mxfp4").is_none(), "a .part means unfinished");
        std::fs::remove_file(dir.path().join(format!("{}.part", files[2]))).unwrap();
        assert_eq!(catalog.installed_path("gpt-oss-120b-mxfp4"), Some(dir.path().join(&files[0])));
    }

    #[test]
    fn an_imported_file_joins_the_catalog_in_place_and_survives_a_reload() {
        let dir = tempfile::tempdir().unwrap();
        let gguf = dir.path().join("my-model.gguf");
        std::fs::write(&gguf, vec![0u8; 1024]).unwrap();
        let mut catalog = Catalog::load(None, dir.path());
        let m = catalog.import(&gguf).unwrap();
        assert_eq!(m.id, "import-my-model");
        assert_eq!(catalog.installed_path(&m.id), Some(gguf.clone()));
        let again = Catalog::load(None, dir.path());
        assert!(again.get("import-my-model").is_some(), "imports persist in imports.json");
        let entry = again
            .entries(&laptop(), &HashMap::new(), None)
            .into_iter()
            .find(|e| e.id == "import-my-model")
            .unwrap();
        assert_eq!(entry.source, "import");
        assert!(entry.installed);
    }

    #[test]
    fn an_organisation_catalog_overrides_the_built_in_entry_of_the_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let org = tempfile::tempdir().unwrap();
        std::fs::write(
            org.path().join("catalog.json"),
            r#"{"version":1,"models":[{"id":"qwen2.5-3b-instruct-q4_k_m","title":"Our 3B","params_b":3.0,"bytes":1,"context_len":4096,"repo":"org/mirror","files":["a.gguf"],"tensor":null}]}"#,
        )
        .unwrap();
        let catalog = Catalog::load(Some(org.path()), dir.path());
        let m = catalog.get("qwen2.5-3b-instruct-q4_k_m").unwrap();
        assert_eq!(m.title, "Our 3B");
        assert_eq!(m.repo, "org/mirror");
    }
}
