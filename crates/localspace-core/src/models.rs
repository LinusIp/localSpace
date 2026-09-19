//! The model catalog (architecture v2 §4.4): a curated list with the numbers
//! the placement planner needs, downloaded from Hugging Face on request into
//! the data directory, plus files the user brought along. Downloads are
//! provisioning egress, separate from the runtime gateway, and refused when
//! the environment is air-gapped.

use crate::engine::EventSink;
use crate::fit::{self, Fit};
use crate::hardware::Hardware;
use crate::planner::{self, MoeLayout, PlanRequest, TensorMap, Verdict};
use crate::profile::{HardwareTier, Machine};
use anyhow::{Context, Result, anyhow, bail};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The catalog shipped with the build, as a file beside the harnesses.
pub const BUILT_IN: &str = include_str!("../../../models/catalog.json");

/// Kept free on the models' drive beyond the model itself: a download never
/// fills a disk to the brim, where the database and the logs also live.
pub const DISK_HEADROOM_MIB: u64 = 1024;

/// The context a model is started with on a computer below the reference
/// tiers: what the small profile's working set needs, and a KV cache a
/// laptop's card can hold beside the weights.
pub const FITTED_CONTEXT: u32 = 8192;

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
    /// The commit of that repository the entry's facts were taken at. Files
    /// are fetched from it, so a repository that renames or replaces a file
    /// (two of the first five entries had, by 2026-09-19) breaks nothing and
    /// the digests keep matching. Empty means the repository's `main`.
    #[serde(default)]
    pub revision: String,
    pub files: Vec<String>,
    #[serde(default = "yes")]
    pub supports_tools: bool,
    #[serde(default)]
    pub notes: String,
    pub tensor: Option<Tensor>,
    /// Set for an imported file: where it lives.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// What each finished file must be, by its name: its exact size, and its
    /// SHA-256 as the publisher gives it. A file that is not listed is held
    /// to the length the server announces.
    #[serde(default)]
    pub verify: HashMap<String, FileCheck>,
}

/// What a finished file must be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCheck {
    pub bytes: u64,
    /// Lower-case hex, as Hugging Face publishes it for the file.
    #[serde(default)]
    pub sha256: String,
}

fn yes() -> bool {
    true
}

/// The verdict as the client's code reads it; `label()` is what a person reads.
fn verdict_id(verdict: fit::Verdict) -> &'static str {
    match verdict {
        fit::Verdict::RunsWell => "runs_well",
        fit::Verdict::Works => "works",
        fit::Verdict::TooSlow => "too_slow",
        fit::Verdict::WillNotFit => "will_not_fit",
    }
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

    /// The model's numbers as `fit` takes them.
    pub fn shape(&self) -> Option<fit::Shape> {
        let map = self.tensor_map()?;
        Some(fit::Shape {
            weight_bytes: map.total_bytes(),
            active_bytes: map.core_bytes + map.expert_bytes_per_token(),
            layers: map.layers,
            kv_bytes_per_token_fp16: map.kv_bytes_per_token_fp16,
        })
    }

    /// What `fit` is asked for this model.
    pub fn ask(&self) -> fit::Ask {
        fit::Ask {
            context_len: self.context_len.min(FITTED_CONTEXT),
            kv_quantized: false,
        }
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
    /// Where a repository's files are fetched from, and how a dropped
    /// connection is retried.
    source: Source,
}

/// Where files come from. Hugging Face, unless a test stands in for it.
#[derive(Debug, Clone)]
pub struct Source {
    /// `<base>/<repo>/resolve/<revision>/<file>` is a file's address.
    pub base: String,
    /// How long to wait before continuing after the connection dropped.
    pub retry_pause: Duration,
    /// How many times in a row a download may get nothing before it gives
    /// up; any progress starts the count again.
    pub attempts: u32,
}

impl Default for Source {
    fn default() -> Self {
        Source {
            base: "https://huggingface.co".into(),
            retry_pause: Duration::from_secs(5),
            attempts: 6,
        }
    }
}

impl Catalog {
    /// The built-in catalog, plus `catalog.json` in `catalog_dir` when it
    /// exists (an organisation's own list), plus the user's imports.
    pub fn load(catalog_dir: Option<&Path>, models_dir: &Path) -> Catalog {
        let mut models = parse(BUILT_IN).map(|c| c.models).unwrap_or_default();
        if let Some(dir) = catalog_dir
            && let Ok(text) = std::fs::read_to_string(dir.join("catalog.json"))
        {
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
            source: Source::default(),
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

    /// Fetch from somewhere else: a test's stand-in for Hugging Face.
    pub fn set_source(&mut self, source: Source) {
        self.source = source;
    }

    /// What is already here of a model that is not finished: the bytes of its
    /// finished files and of the one a download stopped in. `None` when
    /// nothing was begun, or everything is here.
    pub fn paused(&self, m: &CatalogModel) -> Option<proto::DownloadState> {
        let part = |f: &String| self.dir.join(format!("{f}.part"));
        if m.path.is_some() || !m.files.iter().any(|f| part(f).is_file()) {
            return None;
        }
        let here: u64 = m
            .files
            .iter()
            .map(|f| {
                let partial = std::fs::metadata(part(f)).map(|x| x.len());
                let finished = std::fs::metadata(self.dir.join(f)).map(|x| x.len());
                partial.or(finished).unwrap_or(0)
            })
            .sum();
        Some(proto::DownloadState {
            done_bytes: here,
            total_bytes: m.bytes.max(here),
            stage: "paused".into(),
        })
    }

    /// What a download of `id` still has to fetch, in bytes: the model's
    /// size less what is already here. For the check of the disk's room.
    pub fn remaining_bytes(&self, id: &str) -> u64 {
        let Some(m) = self.get(id) else { return 0 };
        let here = self.paused(m).map(|p| p.done_bytes).unwrap_or(0);
        m.bytes.saturating_sub(here)
    }

    pub fn get(&self, id: &str) -> Option<&CatalogModel> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Whether any model is on this computer yet.
    pub fn any_installed(&self) -> bool {
        self.models
            .iter()
            .any(|m| self.installed_path(&m.id).is_some())
    }

    /// The path `llama-server` loads: the first file, once every file is here.
    pub fn installed_path(&self, id: &str) -> Option<PathBuf> {
        let m = self.get(id)?;
        if let Some(p) = &m.path {
            return p.is_file().then(|| p.clone());
        }
        let paths: Vec<PathBuf> = m.files.iter().map(|f| self.dir.join(f)).collect();
        let all_here = paths.iter().all(|p| p.is_file())
            && m.files
                .iter()
                .all(|f| !self.dir.join(format!("{f}.part")).exists());
        (all_here && !paths.is_empty()).then(|| paths[0].clone())
    }

    /// Every entry with its verdict for this machine and its state on disk.
    /// A workstation or server of the reference tiers is planned by the
    /// placement planner; every other computer, given what it was found to
    /// be, by `fit`: three words a person reads, a range of words a second,
    /// and where the model sits.
    pub fn entries(
        &self,
        machine: &Machine,
        hardware: Option<&Hardware>,
        downloads: &HashMap<String, proto::DownloadState>,
        loaded: Option<&str>,
    ) -> Vec<proto::ModelCatalogEntry> {
        let fitted = hardware.filter(|_| machine.tier() == HardwareTier::BelowFloor);
        self.models
            .iter()
            .map(|m| {
                let placed = fitted
                    .and_then(|hw| m.shape().map(|shape| fit::fit(&shape, hw, m.ask(), None)));
                let (verdict, tok_s, first_ms, summary, notes) = match (&placed, m.tensor_map()) {
                    (Some(placed), _) => (
                        verdict_id(placed.verdict).to_string(),
                        placed.tokens_per_second,
                        0.0,
                        placed.placement.clone(),
                        Vec::new(),
                    ),
                    (None, Some(map)) => {
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
                    (None, None) => (
                        "unknown".into(),
                        0.0,
                        0.0,
                        "no tensor map: the planner cannot place this model".into(),
                        Vec::new(),
                    ),
                };
                // A download that is running, or one that stopped part-way
                // and will continue from there.
                let download = downloads.get(&m.id).cloned().or_else(|| self.paused(m));
                proto::ModelCatalogEntry {
                    id: m.id.clone(),
                    title: m.title.clone(),
                    family: m.family.clone(),
                    params_b: m.params_b,
                    active_params_b: if m.active_params_b > 0.0 {
                        m.active_params_b
                    } else {
                        m.params_b
                    },
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
                    verdict_label: placed
                        .as_ref()
                        .map(|p| p.verdict.label().to_string())
                        .unwrap_or_default(),
                    speed: placed.as_ref().map(Fit::speed_in_words).unwrap_or_default(),
                    placement: placed
                        .as_ref()
                        .map(|p| p.placement.clone())
                        .unwrap_or_default(),
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

    /// Where `id` sits on a computer below the reference tiers, with at
    /// most `gpu_layer_cap` layers on the card. `None` for a model whose
    /// numbers are not known.
    pub fn fitted(
        &self,
        id: &str,
        hardware: &Hardware,
        gpu_layer_cap: Option<u32>,
    ) -> Result<Option<Fit>> {
        let m = self
            .get(id)
            .ok_or_else(|| anyhow!("no model `{id}` in the catalog"))?;
        let Some(shape) = m.shape() else {
            return Ok(None);
        };
        let placed = fit::fit(&shape, hardware, m.ask(), gpu_layer_cap);
        if placed.verdict == fit::Verdict::WillNotFit {
            bail!(
                "{} will not fit on this computer. {}",
                m.title,
                placed.placement
            );
        }
        Ok(Some(placed))
    }

    /// The model to start with on this computer: the largest that runs well,
    /// or failing that the largest that works, or failing that the smallest
    /// that fits at all. Only what can be fetched or is already here, and
    /// never one the disk has no room for.
    pub fn recommend(&self, hardware: &Hardware) -> Option<String> {
        let room = |m: &CatalogModel| {
            self.installed_path(&m.id).is_some()
                || hardware
                    .disk_free_mib
                    .is_none_or(|free| m.bytes / (1024 * 1024) < free)
        };
        let placed: Vec<(&CatalogModel, Fit)> = self
            .models
            .iter()
            .filter(|m| !m.repo.is_empty() || self.installed_path(&m.id).is_some())
            .filter(|m| room(m))
            .filter_map(|m| {
                m.shape()
                    .map(|shape| (m, fit::fit(&shape, hardware, m.ask(), None)))
            })
            .collect();
        let largest = |verdict: fit::Verdict| {
            placed
                .iter()
                .filter(|(_, p)| p.verdict == verdict)
                .max_by(|a, b| a.0.params_b.total_cmp(&b.0.params_b))
                .map(|(m, _)| m.id.clone())
        };
        largest(fit::Verdict::RunsWell)
            .or_else(|| largest(fit::Verdict::Works))
            .or_else(|| {
                placed
                    .iter()
                    .filter(|(_, p)| p.verdict == fit::Verdict::TooSlow)
                    .min_by_key(|(m, _)| m.bytes)
                    .map(|(m, _)| m.id.clone())
            })
    }

    /// Whether this model is placeable at all here, with the planner's reasons.
    pub fn placement(
        &self,
        id: &str,
        machine: &Machine,
    ) -> Result<Option<(TensorMap, planner::PlacementPlan)>> {
        let m = self
            .get(id)
            .ok_or_else(|| anyhow!("no model `{id}` in the catalog"))?;
        let Some(map) = m.tensor_map() else {
            return Ok(None);
        };
        let req = PlanRequest {
            context_len: m.context_len.min(16384),
            ..PlanRequest::default()
        };
        let plan = planner::plan(&map, machine, &req);
        if plan.verdict == Verdict::DoesNotFit {
            bail!(
                "{} does not fit this machine: {}",
                m.title,
                plan.notes.join("; ")
            );
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
        let meta =
            std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
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
            revision: String::new(),
            files: vec![path.file_name().unwrap().to_string_lossy().to_string()],
            supports_tools: true,
            notes: "Imported in place. Its size stands in for a tensor map, as a dense model."
                .into(),
            tensor: Some(Tensor {
                core_bytes: meta.len(),
                routed_expert_bytes: 0,
                layers: 32,
                moe: None,
                kv_bytes_per_token_fp16: 0,
            }),
            path: Some(path.to_path_buf()),
            verify: HashMap::new(),
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
        let m = self
            .get(id)
            .ok_or_else(|| anyhow!("no model `{id}` in the catalog"))?
            .clone();
        if m.repo.is_empty() {
            bail!(
                "{} is an imported file; there is nothing to download",
                m.title
            );
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
        let source = self.source.clone();
        std::fs::create_dir_all(&dir)?;
        std::thread::Builder::new()
            .name(format!("download-{id}"))
            .spawn(move || {
                let id = m.id.clone();
                let result = fetch_all(&m, &dir, &source, &downloads, &sink);
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
                    level: if result.is_ok() {
                        proto::NoticeLevel::Info
                    } else {
                        proto::NoticeLevel::Error
                    },
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

/// Fetch every file of a model that is not here yet. A file arrives as
/// `<name>.part` and takes its name only when it is whole, so a model is never
/// "installed" half-way; and the part is **kept** when a download stops, so
/// the next one continues from its last byte instead of from zero.
fn fetch_all(
    m: &CatalogModel,
    dir: &Path,
    source: &Source,
    downloads: &Arc<Mutex<HashMap<String, proto::DownloadState>>>,
    sink: &EventSink,
) -> Result<()> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(None)
        .timeout_connect(Some(Duration::from_secs(30)))
        // A refusal is read by its number here, not turned into an error: 416
        // means the part is already as long as the file.
        .http_status_as_error(false)
        .build()
        .into();
    let mut done_before: u64 = 0;
    for (i, file) in m.files.iter().enumerate() {
        let dest = dir.join(file);
        let part = dir.join(format!("{file}.part"));
        if dest.is_file() && !part.exists() {
            // Already here from an earlier run; count it and move on.
            done_before += std::fs::metadata(&dest).map(|x| x.len()).unwrap_or(0);
            continue;
        }
        let revision = if m.revision.is_empty() {
            "main"
        } else {
            &m.revision
        };
        let url = format!("{}/{}/resolve/{revision}/{file}", source.base, m.repo);
        let mut last = Instant::now();
        let mut report = |file_bytes: u64, file_total: Option<u64>| {
            if last.elapsed() < Duration::from_millis(500) {
                return;
            }
            last = Instant::now();
            let done = done_before + file_bytes;
            // A single file's announced length corrects the catalog's estimate.
            let total = match file_total {
                Some(len) if m.files.len() == 1 => len,
                _ => m.bytes,
            }
            .max(done);
            downloads.lock().unwrap().insert(
                m.id.clone(),
                proto::DownloadState {
                    done_bytes: done,
                    total_bytes: total,
                    stage: "downloading".into(),
                },
            );
            sink(proto::Event::ModelProgress {
                id: m.id.clone(),
                done_bytes: done,
                total_bytes: total,
                stage: format!("downloading {} of {}", i + 1, m.files.len()),
            });
        };
        let written = fetch_file(&agent, &url, &part, m.verify.get(file), source, &mut report)?;
        std::fs::rename(&part, &dest).with_context(|| format!("finishing {}", dest.display()))?;
        done_before += written;
    }
    Ok(())
}

/// One file into `part`, continuing from whatever `part` already holds. The
/// server is asked for the rest (`Range`); when it sends the whole file
/// anyway, the part starts over. A connection that drops is picked up again
/// after a pause, from the last byte written, until `attempts` tries in a row
/// have brought nothing. Returns the file's length, which is `expected`'s when
/// the catalog says what it must be, and the server's otherwise; a file of
/// any other length is removed and refused.
fn fetch_file(
    agent: &ureq::Agent,
    url: &str,
    part: &Path,
    expected: Option<&FileCheck>,
    source: &Source,
    report: &mut dyn FnMut(u64, Option<u64>),
) -> Result<u64> {
    let mut announced: Option<u64> = expected.map(|check| check.bytes);
    let mut fruitless = 0u32;
    // A part that turns out longer than the file is thrown away and begun
    // again, once: nothing here may download the same file for ever.
    let mut begun_again = false;
    loop {
        let have = std::fs::metadata(part).map(|x| x.len()).unwrap_or(0);
        if have > 0 && announced == Some(have) {
            break;
        }
        let mut request = agent.get(url);
        if have > 0 {
            request = request.header("Range", &format!("bytes={have}-"));
        }
        let progressed = match request.call() {
            Ok(mut res) => {
                let status = res.status().as_u16();
                let resumed = match status {
                    206 => true,
                    200 => false,
                    416 => {
                        // Nothing lies beyond what the part holds: it is the
                        // whole file when nothing says how long that is. A
                        // part longer than the file is left from something
                        // else and is begun again, once; a file that ends
                        // short of its published length is refused below.
                        if announced.is_some_and(|len| have > len) && !begun_again {
                            begun_again = true;
                            let _ = std::fs::remove_file(part);
                            continue;
                        }
                        break;
                    }
                    other => bail!("{url} answered {other}"),
                };
                let length = res.body().content_length();
                let start = if resumed { have } else { 0 };
                if announced.is_none() {
                    announced = length.map(|len| start + len);
                }
                let mut out = std::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .append(resumed)
                    .truncate(!resumed)
                    .open(part)
                    .with_context(|| format!("opening {}", part.display()))?;
                let mut reader = res.body_mut().as_reader();
                let mut buf = vec![0u8; 1 << 20];
                let mut written = start;
                let dropped = loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break false,
                        Ok(n) => {
                            out.write_all(&buf[..n])?;
                            written += n as u64;
                            report(written, announced);
                        }
                        // The connection dropped: what was written stays.
                        Err(_) => break true,
                    }
                };
                out.flush()?;
                drop(out);
                // The server has sent all it has. Whether that is the file
                // is judged below, by its length.
                if !dropped && announced.is_none_or(|len| written >= len) {
                    break;
                }
                written > start
            }
            Err(_) => false,
        };
        fruitless = if progressed { 0 } else { fruitless + 1 };
        if fruitless >= source.attempts {
            bail!(
                "stopped after {} tries that brought nothing. What is here is kept: \
                 downloading again continues from it",
                source.attempts
            );
        }
        std::thread::sleep(source.retry_pause);
    }
    let written = std::fs::metadata(part).map(|x| x.len()).unwrap_or(0);
    if let Some(len) = announced
        && written != len
    {
        let _ = std::fs::remove_file(part);
        bail!("the file came to {written} bytes where {len} were expected; it was removed");
    }
    Ok(written)
}

fn parse(text: &str) -> Result<CatalogFile> {
    serde_json::from_str(text).context("parsing the model catalog")
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
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
        let entries = catalog.entries(&laptop(), None, &HashMap::new(), None);
        assert!(entries.len() >= 5, "{}", entries.len());
        let small = entries
            .iter()
            .find(|e| e.id == "qwen2.5-3b-instruct-q4_k_m")
            .unwrap();
        assert_eq!(small.verdict, "resident", "{}", small.plan_summary);
        assert!(!small.installed && !small.loaded && small.download.is_none());
        let big = entries
            .iter()
            .find(|e| e.id == "gpt-oss-120b-mxfp4")
            .unwrap();
        // A 4 GB laptop cannot hold it: the planner may still offer to stream the
        // experts from NVMe, at a rate nobody would call interactive.
        assert!(
            matches!(big.verdict.as_str(), "streaming" | "does not fit"),
            "on a 4 GB laptop: {}",
            big.plan_summary
        );
        assert!(
            big.estimated_tok_s < 15.0,
            "far below the gate: {}",
            big.plan_summary
        );

        let entries = catalog.entries(&w32(), None, &HashMap::new(), None);
        let big = entries
            .iter()
            .find(|e| e.id == "gpt-oss-120b-mxfp4")
            .unwrap();
        assert!(
            matches!(big.verdict.as_str(), "hybrid" | "streaming"),
            "{}",
            big.plan_summary
        );
        assert!(big.estimated_tok_s > 0.0);
    }

    /// The development laptop as it was found on 2026-09-18.
    fn found_laptop() -> Hardware {
        use crate::hardware::{Backend, Gpu, GpuListing, Vendor};
        Hardware {
            gpus: vec![Gpu {
                device: "Vulkan0".into(),
                backend: Backend::Vulkan,
                name: "NVIDIA GeForce RTX 3050 Ti Laptop GPU".into(),
                vendor: Vendor::Nvidia,
                total_mib: 3962,
                free_mib: 3367,
                used_by_others_mib: Some(49),
                integrated: false,
                bandwidth_gbps: Some(192.0),
            }],
            gpu_listing: GpuListing::Listed,
            ram_total_mib: 15_613,
            ram_free_mib: 7_184,
            ram_bandwidth_gbps: 19.3,
            disk_free_mib: Some(140_000),
            cores: 16,
            cpu: None,
            cpu_features: Vec::new(),
        }
    }

    #[test]
    fn on_an_ordinary_computer_every_entry_says_how_it_will_run_before_any_download() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        let found = found_laptop();
        let entries = catalog.entries(&laptop(), Some(&found), &HashMap::new(), None);
        let entry = |id: &str| entries.iter().find(|e| e.id == id).unwrap();

        let small = entry("qwen2.5-3b-instruct-q4_k_m");
        assert_eq!(small.verdict, "runs_well");
        assert_eq!(
            small.verdict_label,
            "Runs well \u{2014} faster than you read"
        );
        assert_eq!(small.speed, "about 20 to 30 words a second");
        assert_eq!(small.placement, "All of it fits in the graphics memory.");

        // Larger than the card: it still runs, with the layers that fit, and says so.
        let medium = entry("qwen2.5-7b-instruct-q4_k_m");
        assert_eq!(medium.verdict, "works", "{}", medium.placement);
        assert!(
            medium
                .placement
                .starts_with("About half of it fits in the graphics memory")
        );
        assert!(medium.speed.starts_with("about "), "{}", medium.speed);

        // Never hidden, never a number: it will not fit, and that is all.
        let huge = entry("gpt-oss-120b-mxfp4");
        assert_eq!(huge.verdict, "will_not_fit");
        assert_eq!(huge.verdict_label, "Will not fit on this computer");
        assert_eq!(huge.speed, "");

        // The model to start with: the largest that runs well here.
        assert_eq!(
            catalog.recommend(&found).as_deref(),
            Some("qwen2.5-3b-instruct-q4_k_m")
        );
        // With no room on the disk for it, the largest that the disk can take.
        let mut full = found_laptop();
        full.disk_free_mib = Some(1_000);
        assert_eq!(
            catalog.recommend(&full).as_deref(),
            Some("qwen2.5-0.5b-instruct-q4_k_m")
        );
    }

    #[test]
    fn a_workstation_of_the_reference_tiers_keeps_its_planner() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        let entries = catalog.entries(&w32(), Some(&found_laptop()), &HashMap::new(), None);
        let small = entries
            .iter()
            .find(|e| e.id == "qwen2.5-3b-instruct-q4_k_m")
            .unwrap();
        assert_eq!(small.verdict, "resident");
        assert_eq!(small.verdict_label, "");
    }

    // --- downloads: a stand-in for Hugging Face ---------------------------

    /// Serves one body at every address, honours `Range` when told to, cuts
    /// its first `drops` answers short after `cut` bytes by closing the
    /// connection, and remembers the `Range` each request carried.
    struct StandIn {
        base: String,
        ranges: Arc<Mutex<Vec<Option<String>>>>,
    }

    fn stand_in(body: Vec<u8>, honour_range: bool, drops: usize, cut: usize) -> StandIn {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let ranges: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
        let seen = ranges.clone();
        std::thread::spawn(move || {
            let mut dropped = 0;
            for mut stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut range = None;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("range:") {
                        range = Some(value.trim().to_string());
                    }
                }
                seen.lock().unwrap().push(range.clone());
                let from = range
                    .filter(|_| honour_range)
                    .and_then(|r| {
                        r.strip_prefix("bytes=")?
                            .strip_suffix('-')?
                            .parse::<usize>()
                            .ok()
                    })
                    .unwrap_or(0);
                if from >= body.len() {
                    let _ = stream.write_all(
                        b"HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    continue;
                }
                let rest = &body[from..];
                let head = if from > 0 {
                    format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {from}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len() - 1,
                        body.len(),
                        rest.len()
                    )
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        rest.len()
                    )
                };
                let _ = stream.write_all(head.as_bytes());
                if dropped < drops {
                    dropped += 1;
                    let _ = stream.write_all(&rest[..cut.min(rest.len())]);
                    continue; // the connection closes with the answer unfinished
                }
                let _ = stream.write_all(rest);
            }
        });
        StandIn { base, ranges }
    }

    fn a_body() -> Vec<u8> {
        (0..3_000_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn one_file_model(expected: Option<u64>) -> CatalogModel {
        let mut m: CatalogModel = serde_json::from_str(
            r#"{"id":"m","title":"M","params_b":1.0,"bytes":3000000,"context_len":2048,
                "repo":"example/m","files":["m.gguf"],"tensor":null}"#,
        )
        .unwrap();
        if let Some(bytes) = expected {
            m.verify.insert(
                "m.gguf".into(),
                FileCheck {
                    bytes,
                    sha256: String::new(),
                },
            );
        }
        m
    }

    fn fetch(m: &CatalogModel, dir: &Path, base: &str) -> Result<()> {
        let source = Source {
            base: base.to_string(),
            retry_pause: Duration::from_millis(10),
            attempts: 3,
        };
        let sink: EventSink = Arc::new(|_| {});
        fetch_all(m, dir, &source, &Arc::default(), &sink)
    }

    #[test]
    fn a_dropped_connection_is_picked_up_from_its_last_byte() {
        let dir = tempfile::tempdir().unwrap();
        let server = stand_in(a_body(), true, 1, 1_000_000);
        fetch(&one_file_model(Some(3_000_000)), dir.path(), &server.base).unwrap();
        assert_eq!(std::fs::read(dir.path().join("m.gguf")).unwrap(), a_body());
        assert!(!dir.path().join("m.gguf.part").exists());
        assert_eq!(
            *server.ranges.lock().unwrap(),
            [None, Some("bytes=1000000-".to_string())],
            "the second request asks for the rest, not for everything"
        );
    }

    #[test]
    fn a_part_left_by_an_earlier_run_is_continued_not_begun_again() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("m.gguf.part"), &a_body()[..1_200_000]).unwrap();
        let server = stand_in(a_body(), true, 0, 0);
        // Before: the catalog says it is paused, with what is here.
        let mut catalog = Catalog::load(None, dir.path());
        catalog.models.push(one_file_model(Some(3_000_000)));
        let paused = catalog.paused(catalog.get("m").unwrap()).unwrap();
        assert_eq!(
            (paused.done_bytes, paused.stage.as_str()),
            (1_200_000, "paused")
        );
        assert_eq!(catalog.remaining_bytes("m"), 1_800_000);
        assert!(
            catalog.installed_path("m").is_none(),
            "a part is not a model"
        );

        fetch(&one_file_model(Some(3_000_000)), dir.path(), &server.base).unwrap();
        assert_eq!(std::fs::read(dir.path().join("m.gguf")).unwrap(), a_body());
        assert_eq!(
            *server.ranges.lock().unwrap(),
            [Some("bytes=1200000-".to_string())]
        );
        assert!(catalog.paused(catalog.get("m").unwrap()).is_none());
        assert!(catalog.installed_path("m").is_some());
    }

    #[test]
    fn a_server_that_sends_everything_anyway_has_the_file_begun_again() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("m.gguf.part"), &a_body()[..1_200_000]).unwrap();
        let server = stand_in(a_body(), false, 0, 0);
        fetch(&one_file_model(None), dir.path(), &server.base).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("m.gguf")).unwrap(),
            a_body(),
            "not the old part with the whole file after it"
        );
    }

    #[test]
    fn a_file_that_is_not_the_published_size_is_removed_and_refused() {
        let dir = tempfile::tempdir().unwrap();
        let server = stand_in(a_body(), true, 0, 0);
        let refused = fetch(&one_file_model(Some(3_000_010)), dir.path(), &server.base);
        let why = format!("{:#}", refused.unwrap_err());
        assert!(
            why.contains("3000000 bytes where 3000010 were expected"),
            "{why}"
        );
        assert!(!dir.path().join("m.gguf").exists(), "never installed");
        assert!(!dir.path().join("m.gguf.part").exists(), "and not kept");
        assert!(
            server.ranges.lock().unwrap().len() <= 2,
            "it asked once more for the rest and then stopped"
        );
    }

    #[test]
    fn a_server_that_never_answers_ends_the_download_and_keeps_what_is_here() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("m.gguf.part"), &a_body()[..500_000]).unwrap();
        // A port nothing listens on.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let refused = fetch(
            &one_file_model(Some(3_000_000)),
            dir.path(),
            &format!("http://127.0.0.1:{port}"),
        );
        let why = format!("{:#}", refused.unwrap_err());
        assert!(why.contains("What is here is kept"), "{why}");
        assert_eq!(
            std::fs::metadata(dir.path().join("m.gguf.part"))
                .unwrap()
                .len(),
            500_000
        );
    }

    #[test]
    fn a_model_is_installed_only_when_every_file_is_here_and_finished() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        // A model in three files: every one of them, finished.
        const MODEL: &str = "qwen2.5-14b-instruct-q4_k_m";
        assert!(catalog.installed_path(MODEL).is_none());
        let files = &catalog.get(MODEL).unwrap().files;
        assert_eq!(files.len(), 3);
        for f in files {
            std::fs::write(dir.path().join(f), b"x").unwrap();
        }
        std::fs::write(dir.path().join(format!("{}.part", files[2])), b"").unwrap();
        assert!(
            catalog.installed_path(MODEL).is_none(),
            "a .part means unfinished"
        );
        std::fs::remove_file(dir.path().join(format!("{}.part", files[2]))).unwrap();
        assert_eq!(
            catalog.installed_path(MODEL),
            Some(dir.path().join(&files[0]))
        );
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
        assert!(
            again.get("import-my-model").is_some(),
            "imports persist in imports.json"
        );
        let entry = again
            .entries(&laptop(), None, &HashMap::new(), None)
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
