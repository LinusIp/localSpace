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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The catalog shipped with the build, as a file beside the harnesses.
pub const BUILT_IN: &str = include_str!("../../../models/catalog.json");

/// Kept free on the models' drive beyond the model itself: a download never
/// fills a disk to the brim, where the database and the logs also live.
pub const DISK_HEADROOM_MIB: u64 = 1024;

/// Below this many billion parameters a model is of the smallest band (the
/// "tiny" band of the any-hardware plan, 1 to 4 billion, and what is under
/// it), and is shown with [`SMALL_MODEL_WORDS`] wherever it is recommended
/// or listed. The verdicts are all about speed; the message script measured
/// that the 1.5B answers "17 × 24 = 388" at forty words a second, and a
/// person who is told only that it runs well takes the product for broken
/// where the small model is small (docs/DECISIONS.md, 2026-09-19, the
/// answers after day 3, A). One sentence on the band, not a second scale.
const SMALL_MODEL_BELOW_B: f32 = 4.0;
pub const SMALL_MODEL_WORDS: &str = "Small models answer quickly but get things wrong more often.";

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
    /// The licence in words a person can act on, written from the licence
    /// itself: "Free to use, also for commercial use."
    #[serde(default)]
    pub license_words: String,
    /// Whether the licence permits commercial use. The default recommendation
    /// only ever offers a model for which this is true (docs/DECISIONS.md,
    /// 2026-09-19); an entry that does not say is not recommended.
    #[serde(default)]
    pub commercial_use: bool,
    /// The day this model last went through the message script
    /// (`scripts/message-script.mjs`) on some machine, and a person read what
    /// came back (`docs/test-a/MESSAGE-SCRIPT.md`). **A model may only be the
    /// default if it has been**: an entry that does not say is listed, can be
    /// chosen, and is never offered first. The failure this guards against
    /// is not a bad model but an untested one arriving in front of a stranger
    /// (docs/DECISIONS.md, 2026-09-19, the answers after day 3).
    #[serde(default)]
    pub script_run: String,
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

/// A file whose SHA-256 was found to be the published one, as the file then
/// was. It is hashed again only when its length or its time of modification
/// has changed: a model is gigabytes, and is not read through at every start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    bytes: u64,
    modified_ms: u64,
    sha256: String,
}

/// The stamps of the models folder, by file name; kept in `verified.json`
/// beside the files.
type Verified = Arc<Mutex<HashMap<String, Stamp>>>;

const VERIFIED_FILE: &str = "verified.json";

fn length_and_time(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some((meta.len(), modified.as_millis() as u64))
}

/// Whether `file` in `dir` is what `check` says it must be, by its stamp.
/// A file the catalog gives no digest for has nothing to be held to.
fn holds(verified: &Verified, dir: &Path, file: &str, check: Option<&FileCheck>) -> bool {
    let Some(check) = check.filter(|c| !c.sha256.is_empty()) else {
        return true;
    };
    let Some((bytes, modified_ms)) = length_and_time(&dir.join(file)) else {
        return false;
    };
    verified.lock().unwrap().get(file).is_some_and(|stamp| {
        stamp.bytes == bytes && stamp.modified_ms == modified_ms && stamp.sha256 == check.sha256
    })
}

/// Record that `file` is the published one, and write the record down.
fn stamp(verified: &Verified, dir: &Path, file: &str, sha256: &str) {
    let Some((bytes, modified_ms)) = length_and_time(&dir.join(file)) else {
        return;
    };
    let mut stamps = verified.lock().unwrap();
    stamps.insert(
        file.to_string(),
        Stamp {
            bytes,
            modified_ms,
            sha256: sha256.to_string(),
        },
    );
    if let Ok(json) = serde_json::to_string_pretty(&*stamps) {
        let _ = std::fs::write(dir.join(VERIFIED_FILE), json);
    }
}

/// The SHA-256 of a file, in lower-case hex, read through in pieces;
/// `on_progress` hears how many bytes have been read.
pub fn sha256_of(path: &Path, on_progress: &mut dyn FnMut(u64)) -> Result<String> {
    use sha2::Digest;
    let mut file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut read = 0u64;
    loop {
        let n = file
            .read(&mut buf)
            .with_context(|| format!("reading {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        read += n as u64;
        on_progress(read);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// A model some of whose files are here and have not been looked at yet.
struct Unchecked {
    id: String,
    title: String,
    /// Each with what it must turn out to be.
    files: Vec<(String, FileCheck)>,
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

    /// What a person is told about this model's answers, where its size calls
    /// for a word: see [`SMALL_MODEL_WORDS`]. Empty for every larger model.
    pub fn quality_words(&self) -> &'static str {
        if self.params_b < SMALL_MODEL_BELOW_B {
            SMALL_MODEL_WORDS
        } else {
            ""
        }
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
    /// Which files have been found to be the published ones.
    verified: Verified,
    /// A look at files that were already there is under way.
    scanning: Arc<AtomicBool>,
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
        let stamps: HashMap<String, Stamp> =
            std::fs::read_to_string(models_dir.join(VERIFIED_FILE))
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
        let catalog = Catalog {
            models,
            dir: models_dir.to_path_buf(),
            source: Source::default(),
            verified: Arc::new(Mutex::new(stamps)),
            scanning: Arc::default(),
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
        let here = self.present_bytes(m);
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
        m.bytes.saturating_sub(self.present_bytes(m))
    }

    /// The bytes of a model that are in the folder: its finished files and
    /// the one a download stopped in.
    fn present_bytes(&self, m: &CatalogModel) -> u64 {
        m.files
            .iter()
            .map(|f| {
                let partial =
                    std::fs::metadata(self.dir.join(format!("{f}.part"))).map(|x| x.len());
                let finished = std::fs::metadata(self.dir.join(f)).map(|x| x.len());
                partial.or(finished).unwrap_or(0)
            })
            .sum()
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
                .all(|f| !self.dir.join(format!("{f}.part")).exists())
            // Here is not enough: a file is the model's only once its SHA-256
            // has been found to be the published one, whether it was
            // downloaded or arrived by other means.
            && m.files
                .iter()
                .all(|f| holds(&self.verified, &self.dir, f, m.verify.get(f)));
        (all_here && !paths.is_empty()).then(|| paths[0].clone())
    }

    /// Files that are here under a catalog entry's name, finished, and not
    /// yet known to be the published ones: a model copied in from a USB
    /// stick or a share, or one whose file has changed since it was checked.
    fn unchecked(&self) -> Vec<Unchecked> {
        self.models
            .iter()
            .filter(|m| m.path.is_none())
            .filter_map(|m| {
                let files: Vec<(String, FileCheck)> = m
                    .files
                    .iter()
                    .filter(|f| self.dir.join(f).is_file())
                    .filter(|f| !self.dir.join(format!("{f}.part")).exists())
                    .filter(|f| !holds(&self.verified, &self.dir, f, m.verify.get(*f)))
                    .filter_map(|f| Some((f.clone(), m.verify.get(f)?.clone())))
                    .collect();
                (!files.is_empty()).then(|| Unchecked {
                    id: m.id.clone(),
                    title: m.title.clone(),
                    files,
                })
            })
            .collect()
    }

    /// Look at the files that were already there (docs/DECISIONS.md,
    /// 2026-09-19): a file with an entry's name **and its SHA-256** counts as
    /// that model, with no download; one that is something else is left alone
    /// and does not count. On a thread of its own, one look at a time; the
    /// entry shows `verifying` meanwhile and `checked` tells the clients to
    /// ask again. Called whenever the catalog is asked for, so that a model
    /// copied in while the app is open is noticed too.
    pub fn scan_present(
        &self,
        downloads: Arc<Mutex<HashMap<String, proto::DownloadState>>>,
        sink: EventSink,
    ) {
        let busy = |id: &String| matches!(downloads.lock().unwrap().get(id), Some(s) if s.stage == "downloading" || s.stage == "verifying");
        let work: Vec<_> = self
            .unchecked()
            .into_iter()
            .filter(|model| !busy(&model.id))
            .collect();
        if work.is_empty() || self.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        let (dir, verified, scanning) = (
            self.dir.clone(),
            self.verified.clone(),
            self.scanning.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("models-already-here".into())
            .spawn(move || {
                for Unchecked { id, title, files } in work {
                    let total: u64 = files.iter().map(|(_, check)| check.bytes).sum();
                    let mut before = 0u64;
                    let mut last = Instant::now();
                    for (file, check) in files {
                        let mut report = |read: u64| {
                            if last.elapsed() < Duration::from_millis(500) {
                                return;
                            }
                            last = Instant::now();
                            let state = proto::DownloadState {
                                done_bytes: (before + read).min(total),
                                total_bytes: total,
                                stage: "verifying".into(),
                            };
                            downloads.lock().unwrap().insert(id.clone(), state.clone());
                            sink(proto::Event::ModelProgress {
                                id: id.clone(),
                                done_bytes: state.done_bytes,
                                total_bytes: state.total_bytes,
                                stage: state.stage,
                            });
                        };
                        report(0);
                        match sha256_of(&dir.join(&file), &mut report) {
                            Ok(found) if found == check.sha256 => stamp(&verified, &dir, &file, &found),
                            Ok(_) => sink(proto::Event::Notice {
                                level: proto::NoticeLevel::Warn,
                                text: format!(
                                    "{file} in the models folder is not the published file of {title}, \
                                     so it does not count. Downloading {title} replaces it."
                                ),
                            }),
                            Err(e) => tracing::warn!("models: {file} could not be read: {e:#}"),
                        }
                        before += check.bytes;
                    }
                    downloads.lock().unwrap().remove(&id);
                    sink(proto::Event::ModelProgress {
                        id: id.clone(),
                        done_bytes: total,
                        total_bytes: total,
                        stage: "checked".into(),
                    });
                }
                scanning.store(false, Ordering::SeqCst);
            });
        if spawned.is_err() {
            self.scanning.store(false, Ordering::SeqCst);
        }
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
        caps: &HashMap<String, u32>,
        downloads: &HashMap<String, proto::DownloadState>,
        loaded: Option<&str>,
    ) -> Vec<proto::ModelCatalogEntry> {
        let fitted = hardware.filter(|_| machine.tier() == HardwareTier::BelowFloor);
        self.models
            .iter()
            .map(|m| {
                // With what a load taught, when one did: a model that had to
                // give layers back shows the speed of where it now sits.
                let cap = caps.get(&m.id).copied();
                let placed =
                    fitted.and_then(|hw| m.shape().map(|shape| fit::fit(&shape, hw, m.ask(), cap)));
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
                    license_words: m.license_words.clone(),
                    commercial_use: m.commercial_use,
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
                    quality_words: m.quality_words().to_string(),
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
    ///
    /// Three standing rules (docs/DECISIONS.md, 2026-09-19). **Only a model
    /// whose licence permits commercial use is ever offered by default**, and
    /// nothing is said of what was passed over: the others are listed, with
    /// their licence in words. **Only a model that has been run through the
    /// message script on some machine is**: see `CatalogModel::script_run`.
    /// And **the ladder has a lowest rung worth
    /// standing on**: a model of under a billion parameters answers in words
    /// but cannot use a tool, so it is offered only where nothing larger so
    /// much as works; a slightly slower larger model comes before it.
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
            .filter(|m| m.commercial_use)
            .filter(|m| !m.script_run.is_empty())
            .filter(|m| !m.repo.is_empty() || self.installed_path(&m.id).is_some())
            .filter(|m| room(m))
            .filter_map(|m| {
                m.shape()
                    .map(|shape| (m, fit::fit(&shape, hardware, m.ask(), None)))
            })
            .collect();
        const LOWEST_RUNG_B: f32 = 1.0;
        let largest = |verdict: fit::Verdict, from_b: f32| {
            placed
                .iter()
                .filter(|(m, p)| p.verdict == verdict && m.params_b >= from_b)
                .max_by(|a, b| a.0.params_b.total_cmp(&b.0.params_b))
                .map(|(m, _)| m.id.clone())
        };
        largest(fit::Verdict::RunsWell, LOWEST_RUNG_B)
            .or_else(|| largest(fit::Verdict::Works, LOWEST_RUNG_B))
            .or_else(|| largest(fit::Verdict::RunsWell, 0.0))
            .or_else(|| largest(fit::Verdict::Works, 0.0))
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
            license_words: String::new(),
            commercial_use: false,
            script_run: String::new(),
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
                bail!("{} is already being downloaded or checked", m.title);
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
        let verified = self.verified.clone();
        std::fs::create_dir_all(&dir)?;
        std::thread::Builder::new()
            .name(format!("download-{id}"))
            .spawn(move || {
                let id = m.id.clone();
                let result = fetch_all(&m, &dir, &source, &verified, &downloads, &sink);
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
    verified: &Verified,
    downloads: &Arc<Mutex<HashMap<String, proto::DownloadState>>>,
    sink: &EventSink,
) -> Result<()> {
    // Told to everyone while a file's SHA-256 is compared with the published one.
    let verifying = |done: u64| {
        let state = proto::DownloadState {
            done_bytes: done.min(m.bytes),
            total_bytes: m.bytes.max(done),
            stage: "verifying".into(),
        };
        downloads
            .lock()
            .unwrap()
            .insert(m.id.clone(), state.clone());
        sink(proto::Event::ModelProgress {
            id: m.id.clone(),
            done_bytes: state.done_bytes,
            total_bytes: state.total_bytes,
            stage: state.stage,
        });
    };
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
        let check = m.verify.get(file).filter(|c| !c.sha256.is_empty());
        if dest.is_file() && !part.exists() {
            // Already here: from an earlier run, or by other means. It counts
            // when it is the published file, and then nothing is fetched.
            let here = std::fs::metadata(&dest).map(|x| x.len()).unwrap_or(0);
            let published = match check {
                None => true,
                Some(_) if holds(verified, dir, file, check) => true,
                Some(check) => {
                    verifying(done_before);
                    let found = sha256_of(&dest, &mut |_| {})?;
                    if found == check.sha256 {
                        stamp(verified, dir, file, &found);
                    }
                    found == check.sha256
                }
            };
            if published {
                done_before += here;
                continue;
            }
            // Something else under the model's name. Shorter than the file,
            // it may still be arriving: it is not touched. Otherwise the
            // download the person asked for replaces it.
            if check.is_some_and(|c| here < c.bytes) {
                bail!(
                    "{file} is already in the models folder and is not complete. If it is still \
                     being copied, wait for that to finish and try again; if not, delete it"
                );
            }
            std::fs::remove_file(&dest).with_context(|| format!("replacing {}", dest.display()))?;
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
        // The right length is not the right file: a download is the model's
        // only when its SHA-256 is the one its publisher gives.
        if let Some(check) = check {
            verifying(done_before + written);
            let found = sha256_of(&part, &mut |_| {})?;
            if found != check.sha256 {
                let _ = std::fs::remove_file(&part);
                bail!(
                    "{file} arrived whole and is not the published file (its SHA-256 is {found}); \
                     it was removed"
                );
            }
        }
        std::fs::rename(&part, &dest).with_context(|| format!("finishing {}", dest.display()))?;
        if let Some(check) = check {
            stamp(verified, dir, file, &check.sha256);
        }
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
        let entries = catalog.entries(&laptop(), None, &HashMap::new(), &HashMap::new(), None);
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

        let entries = catalog.entries(&w32(), None, &HashMap::new(), &HashMap::new(), None);
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
        let entries = catalog.entries(
            &laptop(),
            Some(&found),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        let entry = |id: &str| entries.iter().find(|e| e.id == id).unwrap();

        let small = entry("qwen2.5-3b-instruct-q4_k_m");
        assert_eq!(small.verdict, "runs_well");
        assert_eq!(
            small.verdict_label,
            "Runs well \u{2014} faster than you read"
        );
        assert_eq!(small.speed, "about 20 to 30 words a second");
        assert_eq!(small.placement, "All of it fits in the graphics memory.");

        // Larger than the card: it still runs, with the layers that fit, and
        // says so. Thirteen tokens a second measured: faster than a person
        // reads, which is what "runs well" means since the line is at 10.
        let medium = entry("qwen2.5-7b-instruct-q4_k_m");
        assert_eq!(medium.verdict, "runs_well", "{}", medium.placement);
        assert!(
            medium
                .placement
                .starts_with("About half of it fits in the graphics memory")
        );
        assert_eq!(medium.speed, "about 7 to 9 words a second");
        // Twice the size again. The words are derived from the numbers shown
        // beside them: this line once read "Works — about as fast as you read
        // · about 2 to 3 words a second", the product arguing with itself.
        let large = entry("qwen2.5-14b-instruct-q4_k_m");
        assert_eq!(large.speed, "about 2 to 3 words a second");
        assert_eq!(large.verdict, "too_slow");
        assert_eq!(large.verdict_label, "Too slow for everyday use");

        // The verdict is about speed alone. What to expect of the answers is
        // said of the smallest band, wherever it is listed, and of no other.
        assert_eq!(small.quality_words, SMALL_MODEL_WORDS);
        assert_eq!(
            entry("qwen2.5-0.5b-instruct-q4_k_m").quality_words,
            SMALL_MODEL_WORDS
        );
        assert_eq!(
            entry("qwen2.5-1.5b-instruct-q4_k_m").quality_words,
            SMALL_MODEL_WORDS
        );
        assert_eq!(medium.quality_words, "");
        assert_eq!(large.quality_words, "");
        assert_eq!(entry("qwen3-30b-a3b-q4_k_m").quality_words, "");

        // Never hidden, never a number: it will not fit, and that is all.
        let huge = entry("gpt-oss-120b-mxfp4");
        assert_eq!(huge.verdict, "will_not_fit");
        assert_eq!(huge.verdict_label, "Will not fit on this computer");
        assert_eq!(huge.speed, "");

        // What a load taught shows in the entry: with layers given back,
        // the 7B is said to be slower than it was first said to be.
        let taught: HashMap<String, u32> = [("qwen2.5-7b-instruct-q4_k_m".to_string(), 4)].into();
        let after = catalog.entries(&laptop(), Some(&found), &taught, &HashMap::new(), None);
        let slower = after.iter().find(|e| e.id == medium.id).unwrap();
        assert!(slower.estimated_tok_s < medium.estimated_tok_s);
        assert!(
            slower.placement.starts_with("A small part of it fits"),
            "{}",
            slower.placement
        );

        // The model to start with: the largest that runs well here, which is
        // the 7B: prefer the more reliable model once a model is fast enough
        // to read along with.
        assert_eq!(
            catalog.recommend(&found).as_deref(),
            Some("qwen2.5-7b-instruct-q4_k_m")
        );
        // **Of those whose licence permits commercial use**: without the 7B
        // and the 14B, the 3B is the largest that runs well, and it is passed
        // over without a word for the 1.5B: it is for research and
        // evaluation only.
        assert_eq!(small.verdict, "runs_well");
        assert!(!small.commercial_use);
        assert_eq!(
            small.license_words,
            "Free for research and evaluation only, not for commercial use."
        );
        let mut without = Catalog::load(None, dir.path());
        without.models.retain(|m| m.params_b < 7.0);
        assert_eq!(
            without.recommend(&found).as_deref(),
            Some("qwen2.5-1.5b-instruct-q4_k_m")
        );
        let recommended = entry("qwen2.5-7b-instruct-q4_k_m");
        assert!(recommended.commercial_use);
        assert_eq!(
            recommended.license_words,
            "Free to use, also for commercial use."
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
    fn nobody_lands_on_the_smallest_model_while_a_larger_one_so_much_as_works() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        // No card, and memory slow enough that the 0.5B runs well (16 tokens a
        // second) where the 1.5B only works (7): the 1.5B all the same, since
        // the 0.5B answers in words but cannot use a tool.
        let mut slow = found_laptop();
        slow.gpus.clear();
        slow.ram_bandwidth_gbps = 4.0;
        let entries = catalog.entries(
            &laptop(),
            Some(&slow),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        let verdict = |id: &str| entries.iter().find(|e| e.id == id).unwrap().verdict.clone();
        assert_eq!(verdict("qwen2.5-0.5b-instruct-q4_k_m"), "runs_well");
        assert_eq!(verdict("qwen2.5-1.5b-instruct-q4_k_m"), "works");
        assert_eq!(
            catalog.recommend(&slow).as_deref(),
            Some("qwen2.5-1.5b-instruct-q4_k_m")
        );
        // Slower still, the 1.5B is too slow for everyday use and the 0.5B
        // works: then, and only then, the 0.5B.
        slow.ram_bandwidth_gbps = 2.0;
        assert_eq!(
            catalog.recommend(&slow).as_deref(),
            Some("qwen2.5-0.5b-instruct-q4_k_m")
        );
    }

    /// What typical computers are told and offered: the probe the answers
    /// after day 3 asked for when the line for "runs well" moved from 15 to
    /// 10 tokens a second, kept, so that the next change to a line, to an
    /// efficiency or to the ladder shows what it does to every one of them
    /// (docs/DECISIONS.md, 2026-09-19). Each card comes through the engine's
    /// own device line and the card table, as it does on the day.
    #[test]
    fn what_typical_computers_are_told_and_offered() {
        const WELL: &str = "runs_well";
        const WORKS: &str = "works";
        const SLOW: &str = "too_slow";
        const NO: &str = "will_not_fit";
        const HALF_B: &str = "qwen2.5-0.5b-instruct-q4_k_m";
        const ONE_HALF_B: &str = "qwen2.5-1.5b-instruct-q4_k_m";
        const THREE_B: &str = "qwen2.5-3b-instruct-q4_k_m";
        const SEVEN_B: &str = "qwen2.5-7b-instruct-q4_k_m";
        const FOURTEEN_B: &str = "qwen2.5-14b-instruct-q4_k_m";
        const THIRTY_B: &str = "qwen3-30b-a3b-q4_k_m";
        const IDS: [&str; 6] = [HALF_B, ONE_HALF_B, THREE_B, SEVEN_B, FOURTEEN_B, THIRTY_B];

        struct Shape {
            what: &'static str,
            /// The engine's line for its card, or none.
            card: Option<&'static str>,
            memory_mib: u64,
            /// The measured copy rate of its memory, GB/s.
            copy_gbps: f32,
            default: &'static str,
            /// Of the 0.5B, 1.5B, 3B, 7B, 14B and 30B-A3B, in that order.
            verdicts: [&'static str; 6],
        }
        let shapes = [
            Shape {
                what: "the development laptop: 16 GB and an RTX 3050 Ti with 4 GB",
                card: Some("NVIDIA GeForce RTX 3050 Ti Laptop GPU (3962 MiB, 3367 MiB free)"),
                memory_mib: 15_613,
                copy_gbps: 19.3,
                default: SEVEN_B,
                verdicts: [WELL, WELL, WELL, WELL, SLOW, NO],
            },
            Shape {
                what: "16 GB and an older 4 GB card, a GTX 1650",
                card: Some("NVIDIA GeForce GTX 1650 (4096 MiB, 3500 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 12.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, WORKS, SLOW, NO],
            },
            Shape {
                // Its 14B is shown "about 3 to 4 words a second": about as fast as
                // a person reads, by the numbers themselves.
                what: "16 GB and an RTX 3060 Laptop with 6 GB",
                card: Some("NVIDIA GeForce RTX 3060 Laptop GPU (6144 MiB, 5400 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 15.0,
                default: SEVEN_B,
                verdicts: [WELL, WELL, WELL, WELL, WORKS, NO],
            },
            Shape {
                what: "16 GB and an RTX 4060 Laptop with 8 GB",
                card: Some("NVIDIA GeForce RTX 4060 Laptop GPU (8188 MiB, 7164 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 19.0,
                default: SEVEN_B,
                verdicts: [WELL, WELL, WELL, WELL, WORKS, NO],
            },
            Shape {
                // The 30B runs well here and nobody has ever run it: listed,
                // and not the default.
                what: "32 GB and an RTX 4060 Laptop with 8 GB",
                card: Some("NVIDIA GeForce RTX 4060 Laptop GPU (8188 MiB, 7164 MiB free)"),
                memory_mib: 32_400,
                copy_gbps: 19.0,
                default: SEVEN_B,
                verdicts: [WELL, WELL, WELL, WELL, WORKS, WELL],
            },
            Shape {
                what: "32 GB and an RTX 4080 Laptop with 12 GB",
                card: Some("NVIDIA GeForce RTX 4080 Laptop GPU (12282 MiB, 10900 MiB free)"),
                memory_mib: 32_400,
                copy_gbps: 19.0,
                default: FOURTEEN_B,
                verdicts: [WELL, WELL, WELL, WELL, WELL, WELL],
            },
            Shape {
                what: "32 GB and an RTX 4090 Laptop with 16 GB",
                card: Some("NVIDIA GeForce RTX 4090 Laptop GPU (16376 MiB, 14600 MiB free)"),
                memory_mib: 32_400,
                copy_gbps: 19.0,
                default: FOURTEEN_B,
                verdicts: [WELL, WELL, WELL, WELL, WELL, WELL],
            },
            Shape {
                // A card the table does not know is used, and promised only
                // what the processor would do. With faster memory that floor
                // is "at least 6 words a second" for the 7B, which is faster
                // than a person reads: the 7B.
                what: "16 GB of faster memory and an 8 GB card the table does not know",
                card: Some("Glenfly Arise 8G (8192 MiB, 7300 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 19.0,
                default: SEVEN_B,
                verdicts: [WELL, WELL, WELL, WELL, SLOW, NO],
            },
            Shape {
                // With slower memory the floor is "at least 3 words a second",
                // and the default stays the 1.5B although the card would
                // carry the 7B. The remedy is the card's entry in the table
                // (and, after the test, a floor by the card's memory class).
                what: "16 GB of slower memory and an 8 GB card the table does not know",
                card: Some("Glenfly Arise 8G (8192 MiB, 7300 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 12.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, SLOW, SLOW, NO],
            },
            Shape {
                what: "16 GB and the processor's own graphics",
                card: Some("AMD Radeon(TM) 780M Graphics (8192 MiB, 7000 MiB free)"),
                memory_mib: 16_000,
                copy_gbps: 19.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, WORKS, SLOW, NO],
            },
            Shape {
                what: "16 GB of faster memory and no card",
                card: None,
                memory_mib: 15_613,
                copy_gbps: 19.3,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, WORKS, SLOW, NO],
            },
            Shape {
                what: "16 GB of slower memory and no card",
                card: None,
                memory_mib: 16_000,
                copy_gbps: 12.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, SLOW, SLOW, NO],
            },
            Shape {
                what: "32 GB of faster memory and no card",
                card: None,
                memory_mib: 32_400,
                copy_gbps: 19.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, WORKS, SLOW, WELL],
            },
            Shape {
                what: "8 GB of slower memory and no card",
                card: None,
                memory_mib: 7_900,
                copy_gbps: 12.0,
                default: ONE_HALF_B,
                verdicts: [WELL, WELL, WELL, NO, NO, NO],
            },
        ];

        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        for shape in shapes {
            let mut found = found_laptop();
            found.ram_total_mib = shape.memory_mib;
            found.ram_free_mib = shape.memory_mib / 2;
            found.ram_bandwidth_gbps = shape.copy_gbps;
            found.gpus = shape
                .card
                .map(|line| {
                    crate::hardware::parse_devices(&format!(
                        "Available devices:\n  Vulkan0: {line}\n"
                    ))
                })
                .unwrap_or_default();
            for gpu in &mut found.gpus {
                gpu.used_by_others_mib = Some(300);
            }
            let entries = catalog.entries(
                &laptop(),
                Some(&found),
                &HashMap::new(),
                &HashMap::new(),
                None,
            );
            let told: Vec<&str> = IDS
                .iter()
                .map(|id| {
                    entries
                        .iter()
                        .find(|e| e.id == *id)
                        .map(|e| e.verdict.as_str())
                        .unwrap()
                })
                .collect();
            assert_eq!(told, shape.verdicts, "what it is told: {}", shape.what);
            assert_eq!(
                catalog.recommend(&found).as_deref(),
                Some(shape.default),
                "{}",
                shape.what
            );
            // Whatever is offered first has been through the message script
            // and may be used commercially; and nobody is ever offered the
            // 0.5B on a computer where a larger model so much as works.
            let offered = catalog.get(shape.default).unwrap();
            assert!(!offered.script_run.is_empty() && offered.commercial_use);
            assert_ne!(shape.default, HALF_B, "{}", shape.what);
        }
    }

    #[test]
    fn a_model_whose_licence_does_not_say_is_never_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(None, dir.path());
        // Keep only entries that say nothing of commercial use.
        for m in &mut catalog.models {
            m.commercial_use = false;
        }
        assert_eq!(catalog.recommend(&found_laptop()), None);
    }

    /// A gaming laptop with 32 GB of memory and an 8 GB card: the shape on
    /// which the rule was found to be missing.
    fn laptop_with_32_gb() -> Hardware {
        let mut found = found_laptop();
        found.gpus[0].name = "NVIDIA GeForce RTX 4060 Laptop GPU".into();
        found.gpus[0].total_mib = 8188;
        found.gpus[0].free_mib = 7164;
        found.gpus[0].used_by_others_mib = Some(300);
        found.gpus[0].bandwidth_gbps = Some(256.0);
        found.ram_total_mib = 32_400;
        found.ram_free_mib = 16_200;
        found.ram_bandwidth_gbps = 19.0;
        found
    }

    #[test]
    fn a_model_nobody_has_run_is_never_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut catalog = Catalog::load(None, dir.path());
        let found = laptop_with_32_gb();
        // The 30B runs well here and is the largest that does, and it has
        // never been started through Core on any machine: listed, with its
        // verdict, and not the default.
        let entries = catalog.entries(
            &laptop(),
            Some(&found),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
        let big = entries
            .iter()
            .find(|e| e.id == "qwen3-30b-a3b-q4_k_m")
            .unwrap();
        assert_eq!(big.verdict, "runs_well");
        assert_eq!(
            catalog.recommend(&found).as_deref(),
            Some("qwen2.5-7b-instruct-q4_k_m")
        );
        // Once it has been through the script, it is.
        for m in &mut catalog.models {
            if m.id == "qwen3-30b-a3b-q4_k_m" {
                m.script_run = "2026-10-01".into();
            }
        }
        assert_eq!(
            catalog.recommend(&found).as_deref(),
            Some("qwen3-30b-a3b-q4_k_m")
        );
        // And with no entry that says so, nothing is offered first.
        for m in &mut catalog.models {
            m.script_run.clear();
        }
        assert_eq!(catalog.recommend(&found), None);
    }

    #[test]
    fn every_model_said_to_have_been_through_the_script_is_in_its_record() {
        let record = include_str!("../../../docs/test-a/MESSAGE-SCRIPT.md");
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        let run: Vec<&CatalogModel> = catalog
            .models
            .iter()
            .filter(|m| !m.script_run.is_empty())
            .collect();
        assert!(!run.is_empty());
        for m in run {
            assert!(
                record
                    .lines()
                    .any(|l| l.trim_end() == format!("### {}", m.title)),
                "{} says it went through the message script on {}, and the record has no section for it",
                m.id,
                m.script_run
            );
        }
    }

    #[test]
    fn a_workstation_of_the_reference_tiers_keeps_its_planner() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(None, dir.path());
        let entries = catalog.entries(
            &w32(),
            Some(&found_laptop()),
            &HashMap::new(),
            &HashMap::new(),
            None,
        );
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
        // The stamps the folder already has, as a catalog opened on it would.
        let verified = Catalog::load(None, dir).verified;
        fetch_all(m, dir, &source, &verified, &Arc::default(), &sink)
    }

    fn digest(bytes: &[u8]) -> String {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(bytes))
    }

    /// The one-file model, held to the SHA-256 of `published`.
    fn one_file_model_published_as(published: &[u8]) -> CatalogModel {
        let mut m = one_file_model(Some(published.len() as u64));
        m.verify.get_mut("m.gguf").unwrap().sha256 = digest(published);
        m
    }

    /// A catalog on `dir` that knows the one-file model as well.
    fn catalog_with(dir: &Path, m: &CatalogModel) -> Catalog {
        let mut catalog = Catalog::load(None, dir);
        catalog.models.push(m.clone());
        catalog
    }

    /// Look at what is already there, and wait for the look to be over.
    fn scan(catalog: &Catalog) -> Vec<proto::Event> {
        let events: Arc<Mutex<Vec<proto::Event>>> = Arc::default();
        let heard = events.clone();
        catalog.scan_present(
            Arc::default(),
            Arc::new(move |event| heard.lock().unwrap().push(event)),
        );
        let began = Instant::now();
        while catalog.scanning.load(Ordering::SeqCst) {
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "the look did not end"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        events.lock().unwrap().clone()
    }

    #[test]
    fn a_download_is_the_models_only_when_its_sha256_is_the_published_one() {
        let dir = tempfile::tempdir().unwrap();
        let server = stand_in(a_body(), true, 0, 0);
        let m = one_file_model_published_as(&a_body());
        fetch(&m, dir.path(), &server.base).unwrap();
        assert!(catalog_with(dir.path(), &m).installed_path("m").is_some());

        // The right length and the wrong bytes: whole, and not the model.
        let other = tempfile::tempdir().unwrap();
        let mut wrong = a_body();
        wrong[1_500_000] ^= 0xff;
        let expects_other_bytes = one_file_model_published_as(&wrong);
        let refused = fetch(&expects_other_bytes, other.path(), &server.base);
        let why = format!("{:#}", refused.unwrap_err());
        assert!(why.contains("is not the published file"), "{why}");
        assert!(!other.path().join("m.gguf").exists(), "never installed");
        assert!(!other.path().join("m.gguf.part").exists(), "and not kept");
    }

    #[test]
    fn a_file_that_arrived_by_other_means_counts_by_its_sha256_and_nothing_is_fetched() {
        let dir = tempfile::tempdir().unwrap();
        let m = one_file_model_published_as(&a_body());
        // As from a USB stick: the file is simply there.
        std::fs::write(dir.path().join("m.gguf"), a_body()).unwrap();
        let catalog = catalog_with(dir.path(), &m);
        assert!(
            catalog.installed_path("m").is_none(),
            "here is not yet verified"
        );

        let events = scan(&catalog);
        assert!(catalog.installed_path("m").is_some());
        assert!(
            events.iter().any(|e| matches!(
                e,
                proto::Event::ModelProgress { id, stage, .. } if id == "m" && stage == "checked"
            )),
            "the clients are told to ask again: {events:?}"
        );

        // Known from now on: another start does not read it through again.
        let again = catalog_with(dir.path(), &m);
        assert!(again.installed_path("m").is_some());
        assert!(again.unchecked().is_empty());

        // And asking for it to be downloaded fetches nothing at all.
        let server = stand_in(a_body(), true, 0, 0);
        fetch(&m, dir.path(), &server.base).unwrap();
        assert!(
            server.ranges.lock().unwrap().is_empty(),
            "no request was made"
        );
    }

    #[test]
    fn a_file_copied_in_while_nothing_had_looked_is_verified_by_the_download_it_makes_needless() {
        let dir = tempfile::tempdir().unwrap();
        let m = one_file_model_published_as(&a_body());
        std::fs::write(dir.path().join("m.gguf"), a_body()).unwrap();
        let server = stand_in(a_body(), true, 0, 0);
        fetch(&m, dir.path(), &server.base).unwrap();
        assert!(
            server.ranges.lock().unwrap().is_empty(),
            "no request was made"
        );
        assert!(catalog_with(dir.path(), &m).installed_path("m").is_some());
    }

    #[test]
    fn something_else_under_a_models_name_does_not_count_and_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let m = one_file_model_published_as(&a_body());
        let mut other = a_body();
        other[7] ^= 0xff;
        std::fs::write(dir.path().join("m.gguf"), &other).unwrap();
        let catalog = catalog_with(dir.path(), &m);

        let events = scan(&catalog);
        assert!(catalog.installed_path("m").is_none());
        assert_eq!(
            std::fs::read(dir.path().join("m.gguf")).unwrap(),
            other,
            "not touched"
        );
        assert!(
            events.iter().any(|e| matches!(
                e,
                proto::Event::Notice { level: proto::NoticeLevel::Warn, text } if text.contains("is not the published file")
            )),
            "{events:?}"
        );

        // The download the person then asks for replaces it.
        let server = stand_in(a_body(), true, 0, 0);
        fetch(&m, dir.path(), &server.base).unwrap();
        assert_eq!(std::fs::read(dir.path().join("m.gguf")).unwrap(), a_body());
        assert!(catalog_with(dir.path(), &m).installed_path("m").is_some());
    }

    #[test]
    fn a_file_that_may_still_be_arriving_is_not_touched() {
        let dir = tempfile::tempdir().unwrap();
        let m = one_file_model_published_as(&a_body());
        std::fs::write(dir.path().join("m.gguf"), &a_body()[..1_000_000]).unwrap();
        let server = stand_in(a_body(), true, 0, 0);
        let refused = fetch(&m, dir.path(), &server.base);
        let why = format!("{:#}", refused.unwrap_err());
        assert!(why.contains("is not complete"), "{why}");
        assert_eq!(
            std::fs::metadata(dir.path().join("m.gguf")).unwrap().len(),
            1_000_000
        );
        assert!(server.ranges.lock().unwrap().is_empty());
    }

    #[test]
    fn a_file_that_changed_since_it_was_checked_is_checked_again() {
        let dir = tempfile::tempdir().unwrap();
        let m = one_file_model_published_as(&a_body());
        let file = dir.path().join("m.gguf");
        std::fs::write(&file, a_body()).unwrap();
        let catalog = catalog_with(dir.path(), &m);
        scan(&catalog);
        assert!(catalog.installed_path("m").is_some());

        // The same length, other bytes, a later time: the stamp no longer holds.
        let mut other = a_body();
        other[0] ^= 0xff;
        std::fs::write(&file, &other).unwrap();
        let later = std::time::SystemTime::now() + Duration::from_secs(5);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(catalog.installed_path("m").is_none());
        scan(&catalog);
        assert!(
            catalog.installed_path("m").is_none(),
            "and it is not the model"
        );
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
        let mut catalog = Catalog::load(None, dir.path());
        // A model in three files, of which the catalog gives no digest: every
        // one of them, finished. (With a digest, being here is not enough:
        // the tests of files that arrived by other means say what is.)
        const MODEL: &str = "three";
        catalog.models.push(
            serde_json::from_str(
                r#"{"id":"three","title":"Three","params_b":1.0,"bytes":3,"context_len":2048,
                    "repo":"example/three","files":["a.gguf","b.gguf","c.gguf"],"tensor":null}"#,
            )
            .unwrap(),
        );
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
            .entries(&laptop(), None, &HashMap::new(), &HashMap::new(), None)
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
