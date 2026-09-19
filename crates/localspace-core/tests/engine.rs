//! The sidecar path end to end (architecture v2 §4.1), against the fake
//! `llama-server` this crate builds as a test double: a catalog model on disk,
//! `LoadModel` starts the process with the plan's flags, the supervisor sees
//! it come up, the router gets its worker, a chat turn goes through it, the
//! shell hears every state change, `UnloadModel` stops it — and a crash is
//! restarted.

use localspace_core::hardware::{Backend, Gpu, GpuListing, Hardware, Vendor};
use localspace_core::profile::{Machine, ModelProfile};
use localspace_core::{Config, Core};
use localspace_proto as proto;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FAKE: &str = env!("CARGO_BIN_EXE_fake_llama_server");

/// A data directory whose catalog has one model whose file is a stub, so it
/// counts as downloaded.
fn data_dir_with_model(dir: &Path) -> PathBuf {
    let models = dir.join("models");
    std::fs::create_dir_all(&models).unwrap();
    std::fs::write(models.join("tiny.gguf"), b"not a real model").unwrap();
    let catalog = dir.join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    std::fs::write(
        catalog.join("catalog.json"),
        r#"{"version":1,"models":[{"id":"tiny","title":"Tiny","params_b":0.1,"bytes":16,"context_len":2048,
            "repo":"example/tiny","files":["tiny.gguf"],
            "tensor":{"core_bytes":16,"routed_expert_bytes":0,"layers":2,"moe":null,"kv_bytes_per_token_fp16":256}}]}"#,
    )
    .unwrap();
    catalog
}

/// The machine these tests describe, instead of the one they run on: a
/// W32-class workstation, so the planner calls the stub model "resident" and
/// puts its layers on the GPU wherever the tests run. Detection on a runner
/// without a GPU made the verdict "does not fit" and refused the load; what
/// is under test here is the sidecar path, not the planner.
fn described_machine() -> Machine {
    Machine {
        gpus: vec![32],
        ram_gb: 64,
        cores: 16,
        nvme_gbps: 6.0,
        pcie_gbps: 25.0,
        avx512: false,
        amx: false,
        unified_memory: false,
    }
}

fn core_with_fake_engine(dir: &Path) -> (Core, Arc<Mutex<Vec<proto::Event>>>) {
    let catalog = data_dir_with_model(dir);
    let mut cfg = Config::personal("tester");
    cfg.machine = described_machine();
    cfg.profile = ModelProfile::w32();
    cfg.data_dir = Some(dir.to_path_buf());
    cfg.models_dir = Some(catalog);
    cfg.llama_server = Some(PathBuf::from(FAKE));
    let mut core = Core::new(cfg).expect("creating Core");
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    core.set_event_sink(Box::new(move |_to, ev| sink.lock().unwrap().push(ev)));
    (core, events)
}

fn wait_until(
    core: &Core,
    what: &str,
    mut ok: impl FnMut(&proto::EngineState) -> bool,
) -> proto::EngineState {
    let start = Instant::now();
    loop {
        let state = core.environment().engine;
        if ok(&state) {
            return state;
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "waited 30 s for {what}; engine is {state:?}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn the_catalog_lists_the_model_as_installed_with_a_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let (mut core, _) = core_with_fake_engine(dir.path());
    match core.handle(proto::Request::ListModelCatalog) {
        proto::Response::ModelCatalog { entries } => {
            let tiny = entries
                .iter()
                .find(|e| e.id == "tiny")
                .expect("the catalog model");
            assert!(tiny.installed, "its file is on disk");
            assert!(!tiny.loaded);
            assert_eq!(tiny.verdict, "resident", "{}", tiny.plan_summary);
            assert!(tiny.estimated_tok_s > 0.0);
        }
        other => panic!("expected the catalog, got {other:?}"),
    }
}

#[test]
fn loading_a_model_starts_the_sidecar_and_a_chat_turn_goes_through_it() {
    let dir = tempfile::tempdir().unwrap();
    let (mut core, events) = core_with_fake_engine(dir.path());

    assert!(matches!(
        core.handle(proto::Request::LoadModel { id: "tiny".into() }),
        proto::Response::Ok
    ));
    let loading = core.environment().engine;
    assert_eq!(loading.model.as_deref(), Some("tiny"));
    assert!(loading.loading || loading.running, "{loading:?}");

    let ready = wait_until(&core, "the sidecar to answer", |s| s.running);
    assert!(ready.detail.contains("127.0.0.1:"), "{ready:?}");
    assert!(!ready.loading);

    // The router has the worker now, and the environment says which model.
    let env = core.environment();
    assert_eq!(env.model.as_ref().map(|m| m.id.as_str()), Some("tiny"));

    // The shell heard it come up.
    let heard: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            proto::Event::EngineChanged(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    assert!(
        heard.iter().any(|s| s.running),
        "an engine_changed with running=true: {heard:?}"
    );

    // The flags the planner chose went to the process: the fake logs its args.
    let log = std::fs::read_to_string(dir.path().join("engines").join("tiny.log")).unwrap();
    assert!(log.contains("--alias"), "{log}");
    assert!(
        log.contains("-ngl"),
        "a resident plan puts layers on the GPU: {log}"
    );

    // A turn through the sidecar.
    match core.handle(proto::Request::SendMessage {
        text: "hello".into(),
    }) {
        proto::Response::Transcript { messages } => {
            let reply = messages
                .iter()
                .rev()
                .find(|m| m.role == proto::Role::Assistant)
                .unwrap();
            assert!(
                reply.content.contains("hello from the fake engine"),
                "the reply came from the sidecar: {}",
                reply.content
            );
        }
        other => panic!("expected the transcript, got {other:?}"),
    }

    match core.handle(proto::Request::EngineLog { lines: 5 }) {
        proto::Response::EngineLog { lines } => assert!(!lines.is_empty()),
        other => panic!("expected the log, got {other:?}"),
    }

    // The turn above went through because Core presents the key of this
    // start. Anyone else on the machine, a web page in a browser that found
    // the port included, is refused; only the health check is open.
    let port: u16 = ready
        .detail
        .split("127.0.0.1:")
        .nth(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|digits| digits.parse().ok())
        .expect("the engine's port in its state");
    let without_the_key = ureq::get(&format!("http://127.0.0.1:{port}/v1/models")).call();
    assert!(
        matches!(without_the_key, Err(ureq::Error::StatusCode(401))),
        "a caller without the key is refused: {without_the_key:?}"
    );
    assert!(
        ureq::get(&format!("http://127.0.0.1:{port}/health"))
            .call()
            .is_ok()
    );
    assert!(
        !log.contains("LLAMA_API_KEY"),
        "the key is in no command line: {log}"
    );

    // The catalog says it is loaded; unloading stops the process.
    match core.handle(proto::Request::ListModelCatalog) {
        proto::Response::ModelCatalog { entries } => {
            assert!(entries.iter().any(|e| e.id == "tiny" && e.loaded));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        core.handle(proto::Request::UnloadModel),
        proto::Response::Ok
    ));
    let stopped = core.environment().engine;
    assert!(!stopped.running && stopped.model.is_none(), "{stopped:?}");
    assert!(
        core.environment().model.is_none(),
        "the worker is gone with the process"
    );
}

/// An ordinary computer, described: a laptop with a 4 GB card the table
/// knows, as the development laptop was found on 2026-09-18.
fn an_ordinary_laptop() -> (Machine, Hardware) {
    let machine = Machine {
        gpus: vec![4],
        ram_gb: 15,
        cores: 16,
        ..Machine::default()
    };
    let hardware = Hardware {
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
    };
    (machine, hardware)
}

/// The same Core as the other tests', on that laptop, with a second model in
/// its catalog that no disk has room for.
fn core_on_an_ordinary_laptop(dir: &Path) -> Core {
    let catalog = data_dir_with_model(dir);
    std::fs::write(
        catalog.join("catalog.json"),
        r#"{"version":1,"models":[
            {"id":"tiny","title":"Tiny","params_b":0.1,"bytes":16,"context_len":2048,
             "repo":"example/tiny","files":["tiny.gguf"],
             "tensor":{"core_bytes":16,"routed_expert_bytes":0,"layers":2,"moe":null,"kv_bytes_per_token_fp16":256}},
            {"id":"vast","title":"Vast","params_b":3.0,"bytes":900000000000000,"context_len":2048,
             "repo":"example/vast","files":["vast.gguf"],
             "tensor":{"core_bytes":2000000000,"routed_expert_bytes":0,"layers":36,"moe":null,"kv_bytes_per_token_fp16":36864}}]}"#,
    )
    .unwrap();
    let (machine, hardware) = an_ordinary_laptop();
    let mut cfg = Config::personal("tester");
    cfg.machine = machine;
    cfg.hardware = Some(hardware);
    cfg.data_dir = Some(dir.to_path_buf());
    cfg.models_dir = Some(catalog);
    cfg.llama_server = Some(PathBuf::from(FAKE));
    Core::new(cfg).expect("creating Core")
}

#[test]
fn on_an_ordinary_computer_the_engine_gets_a_number_of_layers_and_one_named_device() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_on_an_ordinary_laptop(dir.path());

    // What the first run shows: the computer in plain words, and no first
    // run any more once a model is here.
    match core.handle(proto::Request::DescribeComputer) {
        proto::Response::Computer(computer) => {
            assert_eq!(
                computer.sentence,
                "NVIDIA GeForce RTX 3050 Ti Laptop GPU, 4 GB of graphics memory, 16 GB of system memory"
            );
            assert!(computer.notes.is_empty(), "{:?}", computer.notes);
            assert_eq!(computer.disk_free_gb, Some(136));
            // A model's file is here (as from a stick), and nothing has been
            // chosen yet: the first run is still to come.
            assert!(computer.first_run);
            assert_eq!(computer.last_model, None);
            assert!(computer.recommended.is_some());
        }
        other => panic!("expected the computer, got {other:?}"),
    }

    // The verdict is in a person's words before anything is loaded.
    match core.handle(proto::Request::ListModelCatalog) {
        proto::Response::ModelCatalog { entries } => {
            let tiny = entries.iter().find(|e| e.id == "tiny").unwrap();
            assert_eq!(tiny.verdict, "runs_well");
            assert!(tiny.verdict_label.starts_with("Runs well"));
        }
        other => panic!("{other:?}"),
    }

    assert!(matches!(
        core.handle(proto::Request::LoadModel { id: "tiny".into() }),
        proto::Response::Ok
    ));
    wait_until(&core, "the sidecar to answer", |s| s.running);
    // The fake logs its arguments: every layer by its number (the two
    // repeating ones and the output), the one device by name, and never 999.
    let log = std::fs::read_to_string(dir.path().join("engines").join("tiny.log")).unwrap();
    assert!(log.contains(r#""-ngl", "3""#), "{log}");
    assert!(log.contains(r#""--device", "Vulkan0""#), "{log}");
    assert!(!log.contains("\"999\""), "{log}");
    // A model has been chosen now: no first run any more, and the next start
    // of the window begins with this one.
    match core.handle(proto::Request::DescribeComputer) {
        proto::Response::Computer(computer) => {
            assert!(!computer.first_run);
            assert_eq!(computer.last_model.as_deref(), Some("tiny"));
        }
        other => panic!("expected the computer, got {other:?}"),
    }
    assert!(matches!(
        core.handle(proto::Request::UnloadModel),
        proto::Response::Ok
    ));
}

#[test]
fn a_download_with_no_room_for_it_is_refused_before_it_starts_and_names_the_place() {
    let dir = tempfile::tempdir().unwrap();
    let mut core = core_on_an_ordinary_laptop(dir.path());
    match core.handle(proto::Request::DownloadModel { id: "vast".into() }) {
        proto::Response::Error { message } => {
            assert!(message.starts_with("Vast needs "), "{message}");
            assert!(
                message.contains(" free. Make room there and try again."),
                "{message}"
            );
            let place = if cfg!(windows) {
                "drive "
            } else {
                "the disk that holds "
            };
            assert!(message.contains(place), "{message}");
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    // Nothing was started: no partial file, no download in the catalog.
    assert!(!dir.path().join("models").join("vast.gguf.part").exists());
    match core.handle(proto::Request::ListModelCatalog) {
        proto::Response::ModelCatalog { entries } => {
            assert!(
                entries
                    .iter()
                    .find(|e| e.id == "vast")
                    .unwrap()
                    .download
                    .is_none()
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_load_that_did_not_hold_is_started_again_as_the_look_says_before_anyone_is_told() {
    use localspace_core::engine::{AfterLoad, Engine, LoadOutcome};
    use localspace_core::model::Router;
    use std::sync::RwLock;

    let dir = tempfile::tempdir().unwrap();
    let model = dir.path().join("m.gguf");
    std::fs::write(&model, b"not a real model").unwrap();
    let router = Arc::new(RwLock::new(Router::default()));
    let events: Arc<Mutex<Vec<proto::Event>>> = Arc::default();
    let heard = events.clone();
    // The look: the first time it says "not where it was planned, one layer
    // on the card instead of three"; the second time it is content.
    let looks: Arc<Mutex<Vec<u32>>> = Arc::default();
    let seen = looks.clone();
    let after: AfterLoad = Box::new(move |outcome| {
        let LoadOutcome::Loaded { pid } = outcome else {
            panic!("the fake engine does not give up: {outcome:?}");
        };
        let mut seen = seen.lock().unwrap();
        seen.push(pid);
        (seen.len() == 1).then(|| vec!["-c".into(), "2048".into(), "-ngl".into(), "1".into()])
    });
    let flags: Vec<String> = ["-c", "2048", "-ngl", "3"].map(String::from).to_vec();
    let engine = Engine::start(
        Path::new(FAKE),
        "m",
        &model,
        &flags,
        2048,
        &dir.path().join("engines"),
        Arc::new(move |event| heard.lock().unwrap().push(event)),
        router.clone(),
        Some(after),
        None,
    )
    .unwrap();

    let start = Instant::now();
    while !engine.state().running {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{:?}",
            engine.state()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let looks = looks.lock().unwrap().clone();
    assert_eq!(looks.len(), 2, "asked again when the second start answered");
    assert_ne!(looks[0], looks[1], "a new process, not the old one");
    // The log is the running process's: started with what the look said.
    let log = std::fs::read_to_string(dir.path().join("engines").join("m.log")).unwrap();
    assert!(log.contains(r#""-ngl", "1""#), "{log}");
    assert!(!log.contains(r#""-ngl", "3""#), "{log}");
    // Nobody was told it was ready until it sat where it should.
    let ready: Vec<bool> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            proto::Event::EngineChanged(s) => Some(s.running),
            _ => None,
        })
        .collect();
    assert_eq!(ready.iter().filter(|r| **r).count(), 1, "{ready:?}");
    assert_eq!(ready.last(), Some(&true), "{ready:?}");
    assert!(router.read().unwrap().chat.is_some());
    engine.stop();
}

/// Starts the fake engine on a stub that says how many layers "fit".
fn start_on_a_card_that_fits(
    layers: u32,
    dir: &Path,
    after: Option<localspace_core::engine::AfterLoad>,
) -> (
    localspace_core::engine::Engine,
    Arc<Mutex<Vec<proto::Event>>>,
) {
    start_on(&format!("fits {layers} layers"), dir, after, None)
}

/// Starts the fake engine on a model stub that says what it says.
fn start_on(
    stub: &str,
    dir: &Path,
    after: Option<localspace_core::engine::AfterLoad>,
    warm_up: Option<localspace_core::model::ChatRequest>,
) -> (
    localspace_core::engine::Engine,
    Arc<Mutex<Vec<proto::Event>>>,
) {
    use localspace_core::model::Router;
    let model = dir.join("m.gguf");
    std::fs::write(&model, stub).unwrap();
    let events: Arc<Mutex<Vec<proto::Event>>> = Arc::default();
    let heard = events.clone();
    let flags: Vec<String> = ["-c", "2048", "-ngl", "29"].map(String::from).to_vec();
    let engine = localspace_core::engine::Engine::start(
        Path::new(FAKE),
        "m",
        &model,
        &flags,
        2048,
        &dir.join("engines"),
        Arc::new(move |event| heard.lock().unwrap().push(event)),
        Arc::new(std::sync::RwLock::new(Router::default())),
        after,
        warm_up,
    )
    .unwrap();
    (engine, events)
}

#[test]
fn an_engine_that_gives_up_while_loading_is_tried_again_smaller_before_anyone_is_told() {
    use localspace_core::engine::{AfterLoad, LoadOutcome};
    let dir = tempfile::tempdir().unwrap();
    let outcomes: Arc<Mutex<Vec<LoadOutcome>>> = Arc::default();
    let seen = outcomes.clone();
    let mut layers = 29u32;
    let after: AfterLoad = Box::new(move |outcome| {
        seen.lock().unwrap().push(outcome);
        (outcome == LoadOutcome::GaveUp).then(|| {
            layers -= 7;
            vec![
                "-c".into(),
                "2048".into(),
                "-ngl".into(),
                layers.to_string(),
            ]
        })
    });
    let (engine, events) = start_on_a_card_that_fits(20, dir.path(), Some(after));
    let start = Instant::now();
    while !engine.state().running {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{:?}",
            engine.state()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let outcomes = outcomes.lock().unwrap().clone();
    assert_eq!(outcomes.len(), 3, "{outcomes:?}");
    assert_eq!(outcomes[..2], [LoadOutcome::GaveUp, LoadOutcome::GaveUp]);
    assert!(matches!(outcomes[2], LoadOutcome::Loaded { .. }));
    let log = std::fs::read_to_string(dir.path().join("engines").join("m.log")).unwrap();
    assert!(
        log.contains(r#""-ngl", "15""#),
        "29, then 22, then 15: {log}"
    );
    // Nobody heard of a failure: only of a model that became ready.
    let events = events.lock().unwrap();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            proto::Event::Notice {
                level: proto::NoticeLevel::Error,
                ..
            }
        )),
        "{events:?}"
    );
    engine.stop();
}

fn wait_for_ready(engine: &localspace_core::engine::Engine) {
    let start = Instant::now();
    while !engine.state().running {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{:?}",
            engine.state()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_engine_reads_the_prompts_stable_part_before_anyone_is_told_it_is_ready() {
    let dir = tempfile::tempdir().unwrap();
    let mut warm_up = localspace_core::model::ChatRequest::new("the stable part".into());
    warm_up.max_tokens = 1;
    let (engine, events) = start_on("fits 99 layers", dir.path(), None, Some(warm_up));
    wait_for_ready(&engine);
    let log = std::fs::read_to_string(dir.path().join("engines").join("m.log")).unwrap();
    assert!(log.contains("request POST /v1/chat/completions"), "{log}");
    // In that order: it read, and then it was ready.
    let events = events.lock().unwrap();
    let read = events.iter().position(
        |e| matches!(e, proto::Event::TraceLine { text } if text.contains("read the prompt's stable part")),
    );
    let ready = events
        .iter()
        .position(|e| matches!(e, proto::Event::EngineChanged(s) if s.running));
    assert!(read.is_some() && read < ready, "{events:?}");
    engine.stop();
}

#[test]
fn a_warm_up_that_fails_is_silent_and_the_model_is_ready_all_the_same() {
    let dir = tempfile::tempdir().unwrap();
    let warm_up = localspace_core::model::ChatRequest::new("the stable part".into());
    let (engine, events) = start_on(
        "fits 99 layers; chat fails",
        dir.path(),
        None,
        Some(warm_up),
    );
    wait_for_ready(&engine);
    let log = std::fs::read_to_string(dir.path().join("engines").join("m.log")).unwrap();
    assert!(
        log.contains("request POST /v1/chat/completions"),
        "it was tried: {log}"
    );
    let events = events.lock().unwrap();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, proto::Event::Notice { .. })),
        "nothing is said of it: {events:?}"
    );
    engine.stop();
}

#[test]
fn an_engine_that_gives_up_and_has_no_smaller_plan_is_said_to_have_failed() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start_on_a_card_that_fits(20, dir.path(), None);
    let start = Instant::now();
    while !engine.state().detail.contains("failed") {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "{:?}",
            engine.state()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!engine.state().running);
    assert!(
        engine.state().detail.contains("exited during load"),
        "{:?}",
        engine.state()
    );
    assert!(events.lock().unwrap().iter().any(|e| matches!(
        e,
        proto::Event::Notice {
            level: proto::NoticeLevel::Error,
            ..
        }
    )));
}

/// The verification item 3 asks for by name, on real hardware: "verify on a
/// machine with less VRAM than the model needs". The real engine, the real
/// 7B model (4.4 GB) and a card smaller than it; Core is told the card is
/// twice its size, so that its first plan is wrong in the way that matters.
/// Ignored, because it needs what a runner does not have:
///
///   LOCALSPACE_REAL_ENGINE=<llama-server> LOCALSPACE_REAL_MODELS=<folder with the 7B's two files> ///     cargo test -p localspace-core --test engine -- --ignored --nocapture a_real_card
#[test]
#[ignore = "needs the real engine, the 7B model on disk and a graphics card smaller than it"]
fn a_real_card_smaller_than_the_model_ends_with_a_plan_that_holds() {
    const MODEL: &str = "qwen2.5-7b-instruct-q4_k_m";
    let (Ok(engine), Ok(models)) = (
        std::env::var("LOCALSPACE_REAL_ENGINE"),
        std::env::var("LOCALSPACE_REAL_MODELS"),
    ) else {
        panic!("set LOCALSPACE_REAL_ENGINE and LOCALSPACE_REAL_MODELS");
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("models")).unwrap();
    for part in ["00001-of-00002", "00002-of-00002"] {
        let name = format!("{MODEL}-{part}.gguf");
        // A link, not a copy: the same volume, no second 4 GB.
        std::fs::hard_link(
            Path::new(&models).join(&name),
            dir.path().join("models").join(&name),
        )
        .expect("linking the model into the test's folder");
    }
    let (machine, mut hardware) = an_ordinary_laptop();
    hardware.gpus[0].total_mib = 8192;
    hardware.gpus[0].free_mib = 8000;
    hardware.gpus[0].used_by_others_mib = Some(0);
    let mut cfg = Config::personal("tester");
    cfg.machine = machine;
    cfg.hardware = Some(hardware);
    cfg.data_dir = Some(dir.path().to_path_buf());
    cfg.llama_server = Some(PathBuf::from(engine));
    let mut core = Core::new(cfg).expect("creating Core");
    let events: Arc<Mutex<Vec<proto::Event>>> = Arc::default();
    let sink = events.clone();
    core.set_event_sink(Box::new(move |_to, ev| sink.lock().unwrap().push(ev)));

    let began = Instant::now();
    assert!(matches!(
        core.handle(proto::Request::LoadModel { id: MODEL.into() }),
        proto::Response::Ok
    ));
    loop {
        let state = core.environment().engine;
        if state.running {
            break;
        }
        assert!(!state.detail.contains("failed"), "{state:?}");
        assert!(began.elapsed() < Duration::from_secs(600), "{state:?}");
        std::thread::sleep(Duration::from_millis(250));
    }
    let lines: Vec<String> = events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            proto::Event::TraceLine { text }
                if text.starts_with("fit:") || text.starts_with("engine:") =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .collect();
    for line in &lines {
        eprintln!("{line}");
    }
    eprintln!("ready after {:.0} s", began.elapsed().as_secs_f32());
    let taught = lines
        .iter()
        .filter(|l| l.starts_with("fit:") && l.contains("; now "))
        .count();
    assert!(
        taught >= 1,
        "the first plan was for a card twice the size: it cannot have held"
    );
    assert!(taught < 5, "and it settled before the starts ran out");

    // What was learnt shows where a person reads it: not "all of it fits".
    match core.handle(proto::Request::ListModelCatalog) {
        proto::Response::ModelCatalog { entries } => {
            let entry = entries.iter().find(|e| e.id == MODEL).unwrap();
            eprintln!(
                "{}: {} · {} · {}",
                entry.title, entry.verdict_label, entry.speed, entry.placement
            );
            assert!(entry.loaded);
            assert_ne!(entry.placement, "All of it fits in the graphics memory.");
        }
        other => panic!("{other:?}"),
    }
    // And it answers, at a speed a person can use.
    let asked = Instant::now();
    match core.handle(proto::Request::SendMessage {
        text: "Say hello in five words.".into(),
    }) {
        proto::Response::Transcript { messages } => {
            let reply = messages
                .iter()
                .rev()
                .find(|m| m.role == proto::Role::Assistant)
                .unwrap();
            eprintln!(
                "answered in {:.1} s: {}",
                asked.elapsed().as_secs_f32(),
                reply.content.trim()
            );
            assert!(!reply.content.trim().is_empty());
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        core.handle(proto::Request::UnloadModel),
        proto::Response::Ok
    ));
}

#[test]
fn a_crashed_sidecar_is_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let (mut core, events) = core_with_fake_engine(dir.path());
    // The fake exits on its own after a second; the supervisor must bring it back.
    // SAFETY: this test is the only one that sets the variable, the fake
    // engine reads it once at spawn, and it is removed again below; no other
    // thread reads the environment meanwhile (edition 2024 makes this explicit).
    unsafe { std::env::set_var("FAKE_LLAMA_CRASH_AFTER_MS", "1000") };
    let result = core.handle(proto::Request::LoadModel { id: "tiny".into() });
    // SAFETY: as above.
    unsafe { std::env::remove_var("FAKE_LLAMA_CRASH_AFTER_MS") };
    assert!(matches!(result, proto::Response::Ok));

    wait_until(&core, "the first start", |s| s.running);
    // It will die at one second; the restart passes through loading again.
    let start = Instant::now();
    let mut saw_restart = false;
    while start.elapsed() < Duration::from_secs(15) {
        let noticed =
            events.lock().unwrap().iter().any(
                |e| matches!(e, proto::Event::Notice { text, .. } if text.contains("restarting")),
            );
        if noticed {
            saw_restart = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(saw_restart, "the supervisor must announce the restart");
    // And it comes back (the restarted child does not inherit the crash timer
    // because the variable is gone from Core's environment by then).
    wait_until(&core, "the restart to answer", |s| s.running);
    core.handle(proto::Request::UnloadModel);
}

#[test]
fn without_llama_server_loading_says_where_to_put_it() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = data_dir_with_model(dir.path());
    let mut cfg = Config::personal("tester");
    cfg.data_dir = Some(dir.path().to_path_buf());
    cfg.models_dir = Some(catalog);
    cfg.llama_server = Some(dir.path().join("nowhere").join("llama-server.exe"));
    let mut core = Core::new(cfg).unwrap();
    // Only if PATH does not happen to have a real one.
    if localspace_core::engine::find_binary(None, None).is_some() {
        return;
    }
    match core.handle(proto::Request::LoadModel { id: "tiny".into() }) {
        proto::Response::Error { message } => {
            assert!(message.contains("engines"), "{message}");
            assert!(message.contains("--llama-server"), "{message}");
        }
        other => panic!("expected a plain refusal, got {other:?}"),
    }
}

#[test]
fn an_airgapped_environment_refuses_to_download_and_points_at_import() {
    let dir = tempfile::tempdir().unwrap();
    let (mut core, _) = core_with_fake_engine(dir.path());
    // Like every setter it answers Ok; the new environment is broadcast.
    assert!(matches!(
        core.handle(proto::Request::SetNetworkMode {
            mode: proto::NetworkMode::Airgapped
        }),
        proto::Response::Ok
    ));
    match core.handle(proto::Request::DownloadModel { id: "tiny".into() }) {
        proto::Response::Error { message } => assert!(message.contains("import"), "{message}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}
