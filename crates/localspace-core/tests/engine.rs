//! The sidecar path end to end (architecture v2 §4.1), against the fake
//! `llama-server` this crate builds as a test double: a catalog model on disk,
//! `LoadModel` starts the process with the plan's flags, the supervisor sees
//! it come up, the router gets its worker, a chat turn goes through it, the
//! shell hears every state change, `UnloadModel` stops it — and a crash is
//! restarted.

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
