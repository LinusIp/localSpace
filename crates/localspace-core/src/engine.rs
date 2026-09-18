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
    match plan.verdict {
        Verdict::Resident => {
            f.extend(["-ngl".into(), "999".into()]);
        }
        Verdict::Hybrid | Verdict::Streaming => {
            if map.is_moe() {
                // Everything on the GPU except the expert layers the plan keeps
                // off it, counted in whole layers: what `--n-cpu-moe` expresses.
                f.extend(["-ngl".into(), "999".into()]);
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
    args: Vec<String>,
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
    ) -> Result<Engine> {
        std::fs::create_dir_all(log_dir).ok();
        let port = free_port()?;
        let log_path = log_dir.join(format!("{}.log", sanitize(model_id)));
        let mut args: Vec<String> = vec![
            "-m".into(),
            model_path.to_string_lossy().into_owned(),
            "--host".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--alias".into(),
            model_id.to_string(),
        ];
        args.extend(flags.iter().cloned());
        let spec = Arc::new(Spec {
            binary: binary.to_path_buf(),
            args,
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
            .spawn(move || supervise(spec, shared, sink, router, model, port, context_len))
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
    crate::child::command(&spec.binary)
        .args(&spec.args)
        .env("LLAMA_API_KEY", &spec.key)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| anyhow!("{e}"))
}

fn supervise(
    spec: Arc<Spec>,
    shared: Arc<Shared>,
    sink: EventSink,
    router: Arc<RwLock<Router>>,
    model: String,
    port: u16,
    context_len: u32,
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
        assert!(f.windows(2).any(|w| w == ["-ngl", "999"]), "{f:?}");
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
        assert!(f.windows(2).any(|w| w == ["-ngl", "999"]), "{f:?}");
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
