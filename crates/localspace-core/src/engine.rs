//! The inference engine as a supervised sidecar (architecture v2 §4.1).
//!
//! `llama-server` runs per model on a loopback port and is talked to over
//! HTTP through the same `OpenAiWorker` any endpoint uses. Core never links a
//! runtime: it starts one, turns the placement plan into its flags, watches
//! its health, restarts it when it dies, and stops it on request. The
//! supervisor thread reports through the event sink, so the shell sees
//! "loading", "ready" and "crashed" the moment they happen.

use crate::model::{OpenAiWorker, Router};
use crate::planner::{PlacementPlan, TensorMap, Verdict};
use anyhow::{Context, Result, anyhow};
use localspace_proto as proto;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

/// Where events from Core's own threads go: the same sink the transport gets.
pub type EventSink = Arc<dyn Fn(proto::Event) + Send + Sync>;

/// How a start of the engine went, for whoever planned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The engine answers; this is its process.
    Loaded { pid: u32 },
    /// The engine gave up while loading: a graphics card asked for far more
    /// than it has refuses, and the process exits.
    GaveUp,
}

/// Asked when a start of the engine is over, and before anyone is told it
/// is ready or has failed: did the model come to sit where the plan put it?
/// `None` accepts what happened. `Some(flags)` has the engine started again
/// with those flags in place of the ones it had, and the question is put
/// again when that start is over. Loading is not evidence of fitting:
/// measured on a 4 GB card with a 4.4 GB model, 20 layers on the card ran at
/// 15.9 tokens a second, 22 loaded and ran at 7.0 (slower than no card at
/// all, the rest having spilled into system memory), and 26 did not load.
/// Whoever answers bounds the number of times.
pub type AfterLoad = Box<dyn FnMut(LoadOutcome) -> Option<Vec<String>> + Send>;

/// How long a model may take to come up before the sidecar is given up on.
/// A hundred-billion-parameter model from NVMe is minutes, not seconds.
pub const LOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const RESTARTS: u32 = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(10 * 60);

/// The name of the binary on this platform.
pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Where `llama-server` is: as given, in the environment, under the data
/// directory, in the package this executable came in, or on PATH. `None`
/// means it is not installed, and the answer to that is a message, not a
/// download of an executable.
pub fn find_binary(given: Option<&Path>, data_dir: Option<&Path>) -> Option<PathBuf> {
    let named = std::env::var_os("LOCALSPACE_LLAMA_SERVER").map(PathBuf::from);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    let path = std::env::var_os("PATH");
    candidates(
        given,
        named.as_deref(),
        data_dir,
        exe_dir.as_deref(),
        path.as_deref(),
    )
    .into_iter()
    .find(|p| p.is_file())
}

/// The places looked in, in order. One a person put there (a flag, the
/// environment, `<data>/engines`) comes before the one the package carries in
/// `engine/` beside the executable, so a faster build placed by hand wins; the
/// package's comes before whatever PATH happens to hold.
fn candidates(
    given: Option<&Path>,
    named: Option<&Path>,
    data_dir: Option<&Path>,
    exe_dir: Option<&Path>,
    path: Option<&std::ffi::OsStr>,
) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    candidates.extend(given.map(Path::to_path_buf));
    candidates.extend(named.map(Path::to_path_buf));
    if let Some(d) = data_dir {
        candidates.push(d.join("engines").join(binary_name()));
        candidates.push(d.join("engines").join("llama-server").join(binary_name()));
    }
    if let Some(d) = exe_dir {
        candidates.push(d.join("engine").join(binary_name()));
    }
    if let Some(path) = path {
        for dir in std::env::split_paths(path) {
            candidates.push(dir.join(binary_name()));
        }
    }
    candidates
}

/// The flags a fit becomes, on a computer below the reference tiers: the
/// context, the number of layers on the card as the engine counts them
/// (never "all of them and hope"), and the one device it may use, by name,
/// so that a hybrid laptop never puts the model on the processor's own
/// graphics. With nothing on the card the engine is told to use no device
/// at all: an unknown or shared card is started carefully.
pub fn fitted_flags(placed: &crate::fit::Fit, context_len: u32) -> Vec<String> {
    let mut f: Vec<String> = vec![
        "-c".into(),
        context_len.to_string(),
        "-ngl".into(),
        placed.gpu_layers.to_string(),
        "--device".into(),
    ];
    f.push(match &placed.device {
        Some(device) => device.clone(),
        None => "none".into(),
    });
    f
}

/// The flags a placement plan becomes (v2 §4.2). What the planner decided —
/// what sits on the GPU, what stays in RAM, the KV precision, the context —
/// expressed in llama.cpp's own terms.
pub fn flags(plan: &PlacementPlan, map: &TensorMap, context_len: u32) -> Vec<String> {
    let mut f: Vec<String> = vec![
        "-c".into(),
        context_len.to_string(),
        "--flash-attn".into(),
        "on".into(),
    ];
    let layers = map.layers.max(1) as u64;
    // Every layer, as the engine counts them: the repeating ones and the output.
    let all = (layers + 1).to_string();
    match plan.verdict {
        Verdict::Resident => {
            f.extend(["-ngl".into(), all]);
        }
        Verdict::Hybrid | Verdict::Streaming => {
            if map.is_moe() {
                // Everything on the GPU except the expert layers the plan keeps
                // off it, counted in whole layers: what `--n-cpu-moe` expresses.
                f.extend(["-ngl".into(), all]);
                let off_gpu = plan.ram_expert_bytes + plan.nvme_expert_bytes;
                let per_layer = map.routed_expert_bytes / layers;
                let cpu_layers = if per_layer == 0 {
                    0
                } else {
                    off_gpu.div_ceil(per_layer).min(layers)
                };
                if cpu_layers > 0 {
                    f.extend(["--n-cpu-moe".into(), cpu_layers.to_string()]);
                }
            } else {
                // Dense and not resident: as many layers on the GPU as fit.
                let per_layer = map.core_bytes / layers;
                let gpu_layers = plan
                    .gpu_resident_bytes
                    .checked_div(per_layer)
                    .map_or(layers, |n| n.min(layers));
                f.extend(["-ngl".into(), gpu_layers.to_string()]);
            }
        }
        Verdict::DoesNotFit => {}
    }
    if plan.kv_precision != "f16" {
        f.extend([
            "--cache-type-k".into(),
            "q8_0".into(),
            "--cache-type-v".into(),
            "q8_0".into(),
        ]);
    }
    f
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Loading,
    Ready,
    Failed(String),
    Stopped,
}

struct Spec {
    binary: PathBuf,
    /// The model, the address and the name: the same at every start.
    args: Vec<String>,
    /// What the placement decided; replaced when a load did not hold.
    flags: Mutex<Vec<String>>,
    log_path: PathBuf,
    /// The key this start of the engine answers to. Without one the engine
    /// accepts any origin and any caller on the machine: a web page open in
    /// a browser could find the port and use the model, or read its slots.
    /// Handed over in the environment, so it is in no command line or log.
    key: String,
}

struct Shared {
    status: Mutex<Status>,
    child: Mutex<Option<Child>>,
    started: Instant,
}

/// One running sidecar.
pub struct Engine {
    pub model_id: String,
    pub port: u16,
    pub log_path: PathBuf,
    spec: Arc<Spec>,
    shared: Arc<Shared>,
    router: Arc<RwLock<Router>>,
}

impl Engine {
    /// Start `binary` on `model_path` with `flags`, on a free loopback port,
    /// logging to `log_dir`, and watch it until it answers or fails.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        binary: &Path,
        model_id: &str,
        model_path: &Path,
        flags: &[String],
        context_len: u32,
        log_dir: &Path,
        sink: EventSink,
        router: Arc<RwLock<Router>>,
        after_load: Option<AfterLoad>,
    ) -> Result<Engine> {
        std::fs::create_dir_all(log_dir).ok();
        let port = free_port()?;
        let log_path = log_dir.join(format!("{}.log", sanitize(model_id)));
        let args: Vec<String> = vec![
            "-m".into(),
            model_path.to_string_lossy().into_owned(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--alias".into(),
            model_id.to_string(),
        ];
        let spec = Arc::new(Spec {
            binary: binary.to_path_buf(),
            args,
            flags: Mutex::new(flags.to_vec()),
            log_path: log_path.clone(),
            key: crate::identity::random_token(),
        });
        let child = spawn(&spec).with_context(|| format!("starting {}", binary.display()))?;
        let shared = Arc::new(Shared {
            status: Mutex::new(Status::Loading),
            child: Mutex::new(Some(child)),
            started: Instant::now(),
        });
        let engine = Engine {
            model_id: model_id.to_string(),
            port,
            log_path,
            spec: spec.clone(),
            shared: shared.clone(),
            router: router.clone(),
        };
        let model = model_id.to_string();
        std::thread::Builder::new()
            .name(format!("engine-{}", sanitize(model_id)))
            .spawn(move || {
                supervise(
                    spec,
                    shared,
                    sink,
                    router,
                    model,
                    port,
                    context_len,
                    after_load,
                )
            })
            .context("spawning the engine supervisor")?;
        Ok(engine)
    }

    pub fn status(&self) -> Status {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn state(&self) -> proto::EngineState {
        let since = self.shared.started.elapsed().as_secs();
        match self.status() {
            Status::Loading => proto::EngineState {
                running: false,
                loading: true,
                model: Some(self.model_id.clone()),
                detail: format!(
                    "llama-server loading {} for {since} s on 127.0.0.1:{}",
                    self.model_id, self.port
                ),
            },
            Status::Ready => proto::EngineState {
                running: true,
                loading: false,
                model: Some(self.model_id.clone()),
                detail: format!(
                    "llama-server serving {} on 127.0.0.1:{}",
                    self.model_id, self.port
                ),
            },
            Status::Failed(why) => proto::EngineState {
                running: false,
                loading: false,
                model: Some(self.model_id.clone()),
                detail: format!("llama-server failed: {why}"),
            },
            Status::Stopped => proto::EngineState {
                running: false,
                loading: false,
                model: None,
                detail: "no model loaded".into(),
            },
        }
    }

    /// Kill the sidecar and forget its worker.
    pub fn stop(&self) {
        *self.shared.status.lock().unwrap() = Status::Stopped;
        if let Some(mut child) = self.shared.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let mut router = self.router.write().unwrap();
        if router
            .info()
            .map(|m| m.id == self.model_id)
            .unwrap_or(false)
        {
            router.chat = None;
        }
    }

    /// The last `n` lines of the sidecar's log.
    pub fn log_tail(&self, n: usize) -> Vec<String> {
        log_tail(&self.spec.log_path, n)
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // A Core going away takes its sidecar with it; a model does not keep
        // running for nobody.
        if let Some(mut child) = self.shared.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn(spec: &Spec) -> Result<Child> {
    let log = File::create(&spec.log_path)
        .with_context(|| format!("creating {}", spec.log_path.display()))?;
    let err = log.try_clone()?;
    let flags = spec.flags.lock().unwrap().clone();
    crate::child::command(&spec.binary)
        .args(&spec.args)
        .args(&flags)
        .env("LLAMA_API_KEY", &spec.key)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| anyhow!("{e}"))
}

/// Start the engine again with other flags. `Ok(false)` when a stop came
/// meanwhile, which wins: under the child's lock, so that the stop finds
/// either the old process or the new one, never neither.
fn start_again(spec: &Spec, shared: &Shared, flags: Vec<String>) -> Result<bool> {
    *spec.flags.lock().unwrap() = flags;
    let mut child = shared.child.lock().unwrap();
    if *shared.status.lock().unwrap() == Status::Stopped {
        return Ok(false);
    }
    if let Some(mut old) = child.take() {
        let _ = old.kill();
        let _ = old.wait();
    }
    *child = Some(spawn(spec)?);
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn supervise(
    spec: Arc<Spec>,
    shared: Arc<Shared>,
    sink: EventSink,
    router: Arc<RwLock<Router>>,
    model: String,
    port: u16,
    context_len: u32,
    mut after_load: Option<AfterLoad>,
) {
    let base = format!("http://127.0.0.1:{port}/v1");
    let health = format!("http://127.0.0.1:{port}/health");
    let mut restarts: Vec<Instant> = Vec::new();

    let emit_state = |shared: &Shared, sink: &EventSink| {
        let state = state_of(
            &shared.status.lock().unwrap(),
            &model,
            port,
            shared.started.elapsed(),
        );
        sink(proto::Event::EngineChanged(state));
    };

    loop {
        // Wait for the sidecar to answer, or to die.
        let mut last_report = Instant::now();
        loop {
            if *shared.status.lock().unwrap() == Status::Stopped {
                return;
            }
            if let Some(exit) = exited(&shared) {
                // It gave up while loading. Whoever planned it may know a
                // smaller plan: then that is tried before anyone is told.
                let smaller = after_load
                    .as_mut()
                    .and_then(|look| look(LoadOutcome::GaveUp));
                if let Some(flags) = smaller {
                    sink(proto::Event::TraceLine {
                        text: format!(
                            "engine: {model} did not load ({exit}); trying again with {}",
                            flags.join(" ")
                        ),
                    });
                    match start_again(&spec, &shared, flags) {
                        Ok(true) => continue,
                        Ok(false) => return,
                        Err(e) => tracing::warn!("engine: could not start again: {e}"),
                    }
                }
                let why = format!(
                    "exited during load ({exit}); {}",
                    last_log_line(&spec.log_path)
                );
                *shared.status.lock().unwrap() = Status::Failed(why.clone());
                sink(proto::Event::Notice {
                    level: proto::NoticeLevel::Error,
                    text: format!("{model}: llama-server {why}"),
                });
                emit_state(&shared, &sink);
                return;
            }
            if healthy(&health) {
                break;
            }
            if shared.started.elapsed() > LOAD_TIMEOUT {
                let why = format!("did not answer within {} s", LOAD_TIMEOUT.as_secs());
                *shared.status.lock().unwrap() = Status::Failed(why.clone());
                if let Some(mut child) = shared.child.lock().unwrap().take() {
                    let _ = child.kill();
                }
                sink(proto::Event::Notice {
                    level: proto::NoticeLevel::Error,
                    text: format!("{model}: llama-server {why}"),
                });
                emit_state(&shared, &sink);
                return;
            }
            if last_report.elapsed() > Duration::from_secs(5) {
                emit_state(&shared, &sink);
                last_report = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(400));
        }

        // It answers. Before anyone is told: is the model where the plan
        // put it? If not, it is started again as the answer says, and asked
        // again when that one answers.
        let pid = shared.child.lock().unwrap().as_ref().map(Child::id);
        let again = after_load
            .as_mut()
            .zip(pid)
            .and_then(|(look, pid)| look(LoadOutcome::Loaded { pid }));
        if let Some(flags) = again {
            sink(proto::Event::TraceLine {
                text: format!(
                    "engine: {model} did not sit where it was planned; starting it again with {}",
                    flags.join(" ")
                ),
            });
            match start_again(&spec, &shared, flags) {
                Ok(true) => {}
                Ok(false) => return,
                Err(e) => {
                    *shared.status.lock().unwrap() =
                        Status::Failed(format!("could not start again: {e}"));
                    emit_state(&shared, &sink);
                    return;
                }
            }
            emit_state(&shared, &sink);
            continue;
        }

        // Ready: the router gets the worker, the shell gets the news.
        {
            let worker = OpenAiWorker::new(&base, &model)
                .with_context_len(context_len)
                .with_key(Some(spec.key.clone()));
            router.write().unwrap().chat = Some(Arc::new(worker));
        }
        *shared.status.lock().unwrap() = Status::Ready;
        sink(proto::Event::TraceLine {
            text: format!(
                "engine: {model} ready on 127.0.0.1:{port} after {:.1} s",
                shared.started.elapsed().as_secs_f32()
            ),
        });
        emit_state(&shared, &sink);

        // Watch it. A crash restarts it, a few times; a stop is a stop.
        let exit = loop {
            if *shared.status.lock().unwrap() == Status::Stopped {
                return;
            }
            if let Some(exit) = exited(&shared) {
                break exit;
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        restarts.retain(|t| t.elapsed() < RESTART_WINDOW);
        if restarts.len() as u32 >= RESTARTS {
            let why = format!(
                "crashed {RESTARTS} times in {} min; not restarting",
                RESTART_WINDOW.as_secs() / 60
            );
            *shared.status.lock().unwrap() = Status::Failed(why.clone());
            router.write().unwrap().chat = None;
            sink(proto::Event::Notice {
                level: proto::NoticeLevel::Error,
                text: format!("{model}: llama-server {why}"),
            });
            emit_state(&shared, &sink);
            return;
        }
        restarts.push(Instant::now());
        sink(proto::Event::Notice {
            level: proto::NoticeLevel::Warn,
            text: format!("{model}: llama-server exited ({exit}); restarting"),
        });
        *shared.status.lock().unwrap() = Status::Loading;
        emit_state(&shared, &sink);
        match spawn(&spec) {
            Ok(child) => *shared.child.lock().unwrap() = Some(child),
            Err(e) => {
                *shared.status.lock().unwrap() = Status::Failed(format!("could not restart: {e}"));
                router.write().unwrap().chat = None;
                emit_state(&shared, &sink);
                return;
            }
        }
    }
}

fn state_of(status: &Status, model: &str, port: u16, since: Duration) -> proto::EngineState {
    match status {
        Status::Loading => proto::EngineState {
            running: false,
            loading: true,
            model: Some(model.to_string()),
            detail: format!(
                "llama-server loading {model} for {} s on 127.0.0.1:{port}",
                since.as_secs()
            ),
        },
        Status::Ready => proto::EngineState {
            running: true,
            loading: false,
            model: Some(model.to_string()),
            detail: format!("llama-server serving {model} on 127.0.0.1:{port}"),
        },
        Status::Failed(why) => proto::EngineState {
            running: false,
            loading: false,
            model: Some(model.to_string()),
            detail: format!("llama-server failed: {why}"),
        },
        Status::Stopped => proto::EngineState {
            running: false,
            loading: false,
            model: None,
            detail: "no model loaded".into(),
        },
    }
}

/// `Some(description)` once the child has exited.
fn exited(shared: &Shared) -> Option<String> {
    let mut guard = shared.child.lock().unwrap();
    let child = guard.as_mut()?;
    match child.try_wait() {
        Ok(Some(status)) => Some(status.to_string()),
        Ok(None) => None,
        Err(e) => Some(format!("unknown: {e}")),
    }
}

fn healthy(url: &str) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(2)))
        .build()
        .into();
    matches!(agent.get(url).call(), Ok(res) if res.status() == 200)
}

fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").context("finding a free port")?;
    Ok(listener.local_addr()?.port())
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn log_tail(path: &Path, n: usize) -> Vec<String> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    // Read only the end of a log that may be large.
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let window = 64 * 1024;
    if len > window {
        let _ = file.seek(SeekFrom::Start(len - window));
    }
    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

fn last_log_line(path: &Path) -> String {
    log_tail(path, 1)
        .pop()
        .unwrap_or_else(|| "no log output".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planner::{PlanRequest, plan, reference_moe_100b_q4};
    use crate::profile::Machine;

    #[test]
    fn an_engine_placed_by_hand_comes_before_the_packaged_one_and_that_before_path() {
        let path = std::env::join_paths([Path::new("on-path")]).unwrap();
        let found = candidates(
            Some(Path::new("given/llama-server")),
            Some(Path::new("named/llama-server")),
            Some(Path::new("data")),
            Some(Path::new("app")),
            Some(&path),
        );
        let expected = [
            PathBuf::from("given/llama-server"),
            PathBuf::from("named/llama-server"),
            Path::new("data").join("engines").join(binary_name()),
            Path::new("data")
                .join("engines")
                .join("llama-server")
                .join(binary_name()),
            Path::new("app").join("engine").join(binary_name()),
            Path::new("on-path").join(binary_name()),
        ];
        assert_eq!(found, expected);
    }

    #[test]
    fn a_fit_becomes_a_number_of_layers_and_one_named_device() {
        let placed = crate::fit::Fit {
            gpu_layers: 16,
            layers_total: 29,
            device: Some("Vulkan0".into()),
            gpu_mib: 2976,
            ram_mib: 2200,
            verdict: crate::fit::Verdict::Works,
            tokens_per_second: 12.5,
            words_per_second: (7, 9),
            at_least: false,
            placement: String::new(),
        };
        assert_eq!(
            fitted_flags(&placed, 8192),
            ["-c", "8192", "-ngl", "16", "--device", "Vulkan0"]
        );
        // Nothing on the card: the engine is told to use no device at all.
        let on_the_processor = crate::fit::Fit {
            gpu_layers: 0,
            device: None,
            ..placed
        };
        assert_eq!(
            fitted_flags(&on_the_processor, 8192),
            ["-c", "8192", "-ngl", "0", "--device", "none"]
        );
    }

    #[test]
    fn with_nothing_said_only_the_package_and_path_are_looked_in() {
        let found = candidates(None, None, None, Some(Path::new("app")), None);
        assert_eq!(found, [Path::new("app").join("engine").join(binary_name())]);
    }

    fn dense_map(bytes: u64, layers: u32) -> TensorMap {
        TensorMap {
            model_id: "dense".into(),
            core_bytes: bytes,
            routed_expert_bytes: 0,
            layers,
            moe: None,
            kv_bytes_per_token_fp16: 32 * 1024,
        }
    }

    fn resident_plan() -> PlacementPlan {
        PlacementPlan {
            model_id: "dense".into(),
            verdict: Verdict::Resident,
            gpu_resident_bytes: 4_000_000_000,
            kv_cache_bytes: 0,
            kv_precision: "f16",
            hot_expert_cache_bytes: 0,
            hot_expert_fraction: 0.0,
            ram_expert_bytes: 0,
            nvme_expert_bytes: 0,
            reservations: Vec::new(),
            estimated_tok_s: 40.0,
            first_token_ms: 200.0,
            cpu_expert_compute: false,
            notes: Vec::new(),
        }
    }

    #[test]
    fn a_resident_dense_model_puts_every_layer_on_the_gpu() {
        let f = flags(&resident_plan(), &dense_map(4_000_000_000, 32), 8192);
        // Every layer by its number, the 32 repeating ones and the output: never 999.
        assert!(f.windows(2).any(|w| w == ["-ngl", "33"]), "{f:?}");
        assert!(!f.iter().any(|a| a == "999"), "{f:?}");
        assert!(f.windows(2).any(|w| w == ["-c", "8192"]), "{f:?}");
        assert!(!f.iter().any(|a| a == "--n-cpu-moe"), "{f:?}");
        assert!(
            !f.iter().any(|a| a == "--cache-type-k"),
            "f16 KV needs no flag: {f:?}"
        );
    }

    #[test]
    fn a_dense_model_that_does_not_fit_gets_as_many_layers_as_fit() {
        let mut p = resident_plan();
        p.verdict = Verdict::Hybrid;
        p.gpu_resident_bytes = 2_000_000_000;
        let f = flags(&p, &dense_map(8_000_000_000, 32), 4096);
        let ngl = f.iter().position(|a| a == "-ngl").map(|i| f[i + 1].clone());
        assert_eq!(
            ngl.as_deref(),
            Some("8"),
            "a quarter of the layers fit: {f:?}"
        );
    }

    #[test]
    fn the_w32_reference_moe_keeps_its_experts_off_the_gpu_with_quantised_kv() {
        // The plan the planner itself produces for the W32 profile and the
        // reference model, turned into flags.
        let machine = Machine {
            gpus: vec![32],
            ram_gb: 64,
            cores: 16,
            nvme_gbps: 6.0,
            pcie_gbps: 25.0,
            amx: false,
            avx512: false,
            unified_memory: false,
        };
        let map = reference_moe_100b_q4();
        let p = plan(&map, &machine, &PlanRequest::default());
        assert_ne!(p.verdict, Verdict::DoesNotFit, "{}", p.summary());
        let f = flags(&p, &map, 16384);
        let cpu_moe = f
            .iter()
            .position(|a| a == "--n-cpu-moe")
            .map(|i| f[i + 1].parse::<u64>().unwrap());
        assert!(
            matches!(cpu_moe, Some(n) if n > 0 && n <= map.layers as u64),
            "{f:?}\n{}",
            p.summary()
        );
        assert!(f.windows(2).any(|w| w == ["-ngl", "49"]), "{f:?}");
        assert!(
            f.iter().any(|a| a == "--cache-type-k"),
            "quantised KV: {f:?}"
        );
    }

    #[test]
    fn the_binary_is_found_beside_the_data_or_nowhere() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            find_binary(None, Some(dir.path())).is_none()
                || std::env::var("PATH")
                    .map(|p| p.contains("llama"))
                    .unwrap_or(false)
        );
        let engines = dir.path().join("engines");
        std::fs::create_dir_all(&engines).unwrap();
        std::fs::write(engines.join(binary_name()), b"").unwrap();
        assert_eq!(
            find_binary(None, Some(dir.path())),
            Some(engines.join(binary_name()))
        );
        assert_eq!(
            find_binary(Some(&engines.join(binary_name())), None),
            Some(engines.join(binary_name()))
        );
    }

    #[test]
    fn the_log_tail_is_the_last_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.log");
        std::fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert_eq!(log_tail(&path, 2), vec!["two", "three"]);
        assert!(log_tail(&dir.path().join("missing.log"), 2).is_empty());
    }
}
