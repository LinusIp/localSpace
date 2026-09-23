//! Answers written beside Core's queue (1.6 and 1.7 of the plan after Test
//! A; docs/DECISIONS.md, 2026-09-23), through the transport the server runs
//! Core behind and the fake engine, which streams as the stub asks: slowly,
//! falling silent, dying mid-answer, or splitting Russian letters between
//! two writes.

use localspace_core::hardware::{Backend, Gpu, GpuListing, Hardware, Vendor};
use localspace_core::profile::Machine;
use localspace_core::stream::Silence;
use localspace_core::transport::{Hub, Outgoing};
use localspace_core::{Caller, Config, Core, To};
use localspace_proto as proto;
use localspace_proto::TurnState;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FAKE: &str = env!("CARGO_BIN_EXE_fake_llama_server");

fn person(user: &str) -> Caller {
    Caller {
        user: user.into(),
        session: format!("session-{user}"),
        ip: "127.0.0.1".into(),
        roles: vec![proto::UserRole::Admin],
        groups: Vec::new(),
    }
}

/// A laptop with a 4 GB card, so that the model is planned and started.
fn laptop() -> (Machine, Hardware) {
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

/// Core behind the server's transport, with one model started on the fake
/// engine, whose answers come as `stub` says.
struct Bench {
    hub: Hub,
    seen: Mutex<Vec<(To, proto::Event)>>,
    answers: Mutex<Vec<(u64, proto::Response)>>,
    _dir: tempfile::TempDir,
}

impl Bench {
    fn new(stub: &str, slots: usize, silence: Silence) -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("models")).unwrap();
        std::fs::write(root.join("models").join("tiny.gguf"), stub).unwrap();
        let catalog = root.join("catalog");
        std::fs::create_dir_all(&catalog).unwrap();
        let entry = serde_json::json!({"version": 1, "models": [{
            "id": "tiny", "title": "Tiny", "params_b": 0.1, "bytes": stub.len(), "context_len": 2048,
            "repo": "example/tiny", "files": ["tiny.gguf"],
            "tensor": {"core_bytes": 16, "routed_expert_bytes": 0, "layers": 2, "moe": null,
                       "kv_bytes_per_token_fp16": 256}
        }]});
        std::fs::write(catalog.join("catalog.json"), entry.to_string()).unwrap();
        let (machine, hardware) = laptop();
        let mut cfg = Config::personal("anna");
        cfg.machine = machine;
        cfg.hardware = Some(hardware);
        cfg.data_dir = Some(root.to_path_buf());
        cfg.models_dir = Some(catalog);
        cfg.llama_server = Some(PathBuf::from(FAKE));
        cfg.slots = slots;
        cfg.silence = silence;
        let mut core = Core::new(cfg).expect("creating Core");
        assert!(matches!(
            core.handle(proto::Request::LoadModel { id: "tiny".into() }),
            proto::Response::Ok
        ));
        let began = Instant::now();
        while !core.environment().engine.running {
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "the engine never answered"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        Bench {
            hub: Hub::spawn(core),
            seen: Mutex::default(),
            answers: Mutex::default(),
            _dir: dir,
        }
    }

    fn collect(&self) {
        for out in self.hub.poll() {
            match out {
                Outgoing::Response { id, response } => {
                    self.answers.lock().unwrap().push((id, response));
                }
                Outgoing::Event { to, event } => self.seen.lock().unwrap().push((to, event)),
            }
        }
    }

    fn ask(&self, who: &Caller, request: proto::Request) -> proto::Response {
        let id = self.hub.request_as(who, request);
        let began = Instant::now();
        loop {
            self.collect();
            let mut answers = self.answers.lock().unwrap();
            if let Some(at) = answers.iter().position(|(answered, _)| *answered == id) {
                return answers.remove(at).1;
            }
            drop(answers);
            assert!(
                began.elapsed() < Duration::from_secs(20),
                "no answer to request {id}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// Every event `user` has been sent so far.
    fn events_of(&self, user: &str) -> Vec<proto::Event> {
        self.collect();
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(to, _)| *to == To::User(user.into()) || *to == To::All)
            .map(|(_, event)| event.clone())
            .collect()
    }

    fn wait_for(&self, user: &str, what: &str, found: impl Fn(&[proto::Event]) -> bool) {
        let began = Instant::now();
        loop {
            if found(&self.events_of(user)) {
                return;
            }
            assert!(
                began.elapsed() < Duration::from_secs(30),
                "waited 30 s for {what}: {:#?}",
                self.events_of(user)
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Where `user`'s answer in `conversation` went, in order.
    fn states(&self, user: &str, conversation: &str) -> Vec<TurnState> {
        self.events_of(user)
            .iter()
            .filter_map(|e| match e {
                proto::Event::TurnChanged {
                    conversation: c,
                    state,
                } if c == conversation => Some(*state),
                _ => None,
            })
            .collect()
    }

    fn words_to(&self, user: &str, conversation: &str) -> String {
        self.events_of(user)
            .iter()
            .filter_map(|e| match e {
                proto::Event::AssistantDelta {
                    conversation: c,
                    text,
                } if c == conversation => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn current_chat(&self, who: &Caller) -> String {
        match self.ask(who, proto::Request::ListConversations) {
            proto::Response::Conversations { current, .. } => current,
            other => panic!("{other:?}"),
        }
    }

    fn send(&self, who: &Caller, text: &str) -> String {
        let chat = self.current_chat(who);
        match self.ask(
            who,
            proto::Request::SendMessage {
                text: text.into(),
                conversation: Some(chat.clone()),
            },
        ) {
            proto::Response::Transcript { .. } => chat,
            other => panic!("sending: {other:?}"),
        }
    }

    fn transcript(&self, who: &Caller) -> Vec<proto::ChatMessage> {
        match self.ask(who, proto::Request::GetTranscript) {
            proto::Response::Transcript { messages } => messages,
            other => panic!("{other:?}"),
        }
    }

    fn ended(&self, user: &str, conversation: &str, how: TurnState) {
        self.wait_for(user, &format!("{how:?} in {conversation}"), |events| {
            events.iter().any(|e| {
                matches!(e, proto::Event::TurnChanged { conversation: c, state } if c == conversation && *state == how)
            })
        });
    }
}

const SLOWLY: &str = "streams slowly";
const TWENTY_WORDS: &str = "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12 word13 word14 word15 word16 word17 word18 word19 word20 ";

/// The lock Test A's testers met: while an answer is written, Core answers
/// everything else, at once.
#[test]
fn a_request_is_answered_while_an_answer_is_being_written() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let anna = person("anna");
    let chat = bench.send(&anna, "Tell me something long.");
    bench.wait_for("anna", "the first word", |events| {
        events
            .iter()
            .any(|e| matches!(e, proto::Event::AssistantDelta { .. }))
    });

    let asked = Instant::now();
    assert!(matches!(
        bench.ask(&anna, proto::Request::ListConversations),
        proto::Response::Conversations { .. }
    ));
    assert!(
        asked.elapsed() < Duration::from_millis(1000),
        "answered in {:?}",
        asked.elapsed()
    );
    assert_eq!(bench.states("anna", &chat), [TurnState::Writing]);

    bench.ended("anna", &chat, TurnState::Done);
    assert_eq!(
        bench.transcript(&anna).last().unwrap().content,
        TWENTY_WORDS
    );
}

/// Stop while the model writes: the connection is closed at once, and what
/// came is kept, marked as stopped.
#[test]
fn a_stop_mid_answer_keeps_what_came_marked_as_stopped() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let anna = person("anna");
    let chat = bench.send(&anna, "Tell me something long.");
    bench.wait_for("anna", "three words", |events| {
        events
            .iter()
            .filter(|e| matches!(e, proto::Event::AssistantDelta { .. }))
            .count()
            >= 3
    });

    let stopped_at = Instant::now();
    assert!(matches!(
        bench.ask(&anna, proto::Request::CancelTurn { conversation: None }),
        proto::Response::Ok
    ));
    bench.ended("anna", &chat, TurnState::Stopped);
    assert!(
        stopped_at.elapsed() < Duration::from_secs(2),
        "{:?}",
        stopped_at.elapsed()
    );

    let shown = bench.words_to("anna", &chat);
    let answer = bench.transcript(&anna).last().unwrap().clone();
    assert_eq!(answer.role, proto::Role::Assistant);
    assert!(answer.stopped, "{answer:?}");
    assert_eq!(answer.content, shown, "exactly what was shown is kept");
    assert!(
        answer.content.starts_with("word1 word2 word3 "),
        "{}",
        answer.content
    );
    assert!(
        answer.content.len() < TWENTY_WORDS.len(),
        "{}",
        answer.content
    );
    // Nothing more comes after the stop.
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(bench.words_to("anna", &chat), shown);
}

/// One answer at a time for a person: a message in a second chat waits, says
/// so, and starts by itself.
#[test]
fn a_second_chat_waits_its_turn_and_starts_by_itself() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let anna = person("anna");
    let first = bench.send(&anna, "The first question.");
    bench.ask(&anna, proto::Request::NewConversation);
    let second = bench.send(&anna, "The second question.");
    assert_eq!(
        bench.states("anna", &second),
        [TurnState::WaitsForAnotherChat]
    );

    bench.ended("anna", &first, TurnState::Done);
    bench.ended("anna", &second, TurnState::Done);
    assert_eq!(
        bench.states("anna", &second),
        [
            TurnState::WaitsForAnotherChat,
            TurnState::Writing,
            TurnState::Done
        ]
    );
    // Each answer is in its own chat.
    assert_eq!(
        bench.transcript(&anna).last().unwrap().content,
        TWENTY_WORDS
    );
    bench.ask(
        &anna,
        proto::Request::SelectConversation { id: first.clone() },
    );
    let answers = bench
        .transcript(&anna)
        .iter()
        .filter(|m| m.role == proto::Role::Assistant)
        .count();
    assert_eq!(answers, 1);
}

/// With two slots two people are answered at once; one person's second chat
/// waits all the same, a slot being someone else's turn.
#[test]
fn two_people_are_answered_at_once_where_the_engine_has_two_slots() {
    let bench = Bench::new(SLOWLY, 2, Silence::ANSWER);
    let (anna, ben) = (person("anna"), person("ben"));
    let annas = bench.send(&anna, "Anna's question.");
    let bens = bench.send(&ben, "Ben's question.");
    assert_eq!(bench.states("anna", &annas), [TurnState::Writing]);
    assert_eq!(bench.states("ben", &bens), [TurnState::Writing]);

    bench.ask(&anna, proto::Request::NewConversation);
    let annas_second = bench.send(&anna, "Anna's other question.");
    assert_eq!(
        bench.states("anna", &annas_second),
        [TurnState::WaitsForAnotherChat]
    );

    bench.ended("anna", &annas, TurnState::Done);
    bench.ended("ben", &bens, TurnState::Done);
    bench.ended("anna", &annas_second, TurnState::Done);
    assert_eq!(bench.words_to("ben", &bens), TWENTY_WORDS);
    assert!(bench.events_of("ben").iter().all(|e| match e {
        proto::Event::AssistantDelta { conversation, .. } => conversation == &bens,
        _ => true,
    }));
}

/// With one slot, a second person waits for the model, and is told it is
/// the model they wait for, not another chat of theirs.
#[test]
fn with_one_slot_a_second_person_waits_for_the_model() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let (anna, ben) = (person("anna"), person("ben"));
    let annas = bench.send(&anna, "Anna's question.");
    let bens = bench.send(&ben, "Ben's question.");
    assert_eq!(bench.states("ben", &bens), [TurnState::WaitsForTheModel]);
    bench.ended("anna", &annas, TurnState::Done);
    bench.ended("ben", &bens, TurnState::Done);
    assert_eq!(
        bench.states("ben", &bens),
        [
            TurnState::WaitsForTheModel,
            TurnState::Writing,
            TurnState::Done
        ]
    );
}

/// Nobody stops, or writes into, another person's chat.
#[test]
fn one_person_cannot_stop_another_persons_answer() {
    let bench = Bench::new(SLOWLY, 2, Silence::ANSWER);
    let (anna, ben) = (person("anna"), person("ben"));
    let annas = bench.send(&anna, "Anna's question.");
    bench.wait_for("anna", "the first word", |events| {
        events
            .iter()
            .any(|e| matches!(e, proto::Event::AssistantDelta { .. }))
    });

    // Ben names Anna's chat: there is no answer of his there to stop.
    assert!(matches!(
        bench.ask(
            &ben,
            proto::Request::CancelTurn {
                conversation: Some(annas.clone())
            }
        ),
        proto::Response::Ok
    ));
    // Nor can he write into it.
    assert!(matches!(
        bench.ask(
            &ben,
            proto::Request::SendMessage {
                text: "mine now".into(),
                conversation: Some(annas.clone()),
            }
        ),
        proto::Response::Error { .. }
    ));

    bench.ended("anna", &annas, TurnState::Done);
    let answer = bench.transcript(&anna).last().unwrap().clone();
    assert_eq!(
        answer.content, TWENTY_WORDS,
        "Anna's answer went to its end"
    );
    assert!(!answer.stopped);
    assert!(
        bench.states("ben", &annas).is_empty(),
        "Ben heard nothing of it"
    );
}

/// The person moves to another chat while the answer is written: it lands
/// in the chat it answers, not the one on screen.
#[test]
fn an_answer_lands_in_its_own_chat_after_the_person_switched() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let anna = person("anna");
    let asked_in = bench.send(&anna, "Tell me something long.");
    bench.wait_for("anna", "the first word", |events| {
        events
            .iter()
            .any(|e| matches!(e, proto::Event::AssistantDelta { .. }))
    });
    bench.ask(&anna, proto::Request::NewConversation);
    let now_on_screen = bench.current_chat(&anna);
    assert_ne!(now_on_screen, asked_in);

    bench.ended("anna", &asked_in, TurnState::Done);
    assert!(bench.transcript(&anna).is_empty(), "the new chat is empty");
    bench.ask(
        &anna,
        proto::Request::SelectConversation {
            id: asked_in.clone(),
        },
    );
    let there = bench.transcript(&anna);
    assert_eq!(there.len(), 2, "{there:#?}");
    assert_eq!(there[1].content, TWENTY_WORDS);
    assert_eq!(bench.words_to("anna", &asked_in), TWENTY_WORDS);
    assert!(bench.words_to("anna", &now_on_screen).is_empty());
}

fn a_cut_line(events: &[proto::Event]) -> Option<String> {
    events.iter().find_map(|e| match e {
        proto::Event::TraceLine { text }
            if text.starts_with("answer:") && text.contains("cut:") =>
        {
            Some(text.clone())
        }
        _ => None,
    })
}

const QUICK: Silence = Silence {
    before_first_word: Duration::from_secs(1),
    between_words: Duration::from_secs(1),
};

/// A model that never begins: the answer ends by itself when the silence
/// allowed before the first word runs out, and the log says so with the
/// prompt's length.
#[test]
fn silence_before_the_first_word_ends_the_answer_and_is_logged() {
    let bench = Bench::new("stalls before the first piece", 1, QUICK);
    let anna = person("anna");
    let chat = bench.send(&anna, "Hello?");
    bench.ended("anna", &chat, TurnState::Cut);
    let line = a_cut_line(&bench.events_of("anna")).expect("the log line of the cut");
    assert!(line.contains("before its first word"), "{line}");
    assert!(line.contains("prompt of about"), "{line}");
    let answer = bench.transcript(&anna).last().unwrap().clone();
    assert!(answer.stopped && answer.content.is_empty(), "{answer:?}");
}

/// A model that stops writing part-way: what came is kept, and the log says
/// the silence came after its last word.
#[test]
fn silence_between_words_ends_the_answer_and_keeps_the_words() {
    let bench = Bench::new("stalls after 3 pieces", 1, QUICK);
    let anna = person("anna");
    let chat = bench.send(&anna, "Hello?");
    bench.ended("anna", &chat, TurnState::Cut);
    let line = a_cut_line(&bench.events_of("anna")).expect("the log line of the cut");
    assert!(line.contains("after its last word"), "{line}");
    assert!(line.contains("prompt of about"), "{line}");
    let answer = bench.transcript(&anna).last().unwrap().clone();
    assert_eq!(answer.content, "word1 word2 word3 ");
    assert!(answer.stopped);
}

/// The engine goes away mid-answer: what came is kept, and it is a cut, not
/// a failure to reach the model.
#[test]
fn an_engine_that_goes_away_mid_answer_leaves_what_came() {
    let bench = Bench::new("dies after 2 pieces", 1, Silence::ANSWER);
    let anna = person("anna");
    let chat = bench.send(&anna, "Hello?");
    bench.ended("anna", &chat, TurnState::Cut);
    let answer = bench.transcript(&anna).last().unwrap().clone();
    assert_eq!(answer.content, "word1 word2 ");
    assert!(answer.stopped);
    let line = a_cut_line(&bench.events_of("anna")).expect("the log line of the cut");
    assert!(line.contains("ended before the answer did"), "{line}");
}

/// Russian text, every letter's bytes arriving in separate reads here and
/// there: the answer is whole, and so is every piece shown.
#[test]
fn a_russian_answer_split_inside_its_letters_arrives_whole() {
    let bench = Bench::new("writes in Russian", 1, Silence::ANSWER);
    let anna = person("anna");
    let chat = bench.send(&anna, "Привет?");
    bench.ended("anna", &chat, TurnState::Done);
    assert_eq!(bench.words_to("anna", &chat), "Привет, мир! Как дела?");
    assert_eq!(
        bench.transcript(&anna).last().unwrap().content,
        "Привет, мир! Как дела?"
    );
}

/// When the transport goes, Core's thread ends and Core with it, and its
/// engine is stopped: Core holds its way back into its own queue weakly.
#[test]
fn dropping_the_transport_ends_core_and_its_engine() {
    let bench = Bench::new(SLOWLY, 1, Silence::ANSWER);
    let anna = person("anna");
    let port = match bench.ask(&anna, proto::Request::GetEnvironment) {
        proto::Response::Environment(env) => env
            .engine
            .detail
            .split("127.0.0.1:")
            .nth(1)
            .and_then(|rest| {
                rest.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse::<u16>()
                    .ok()
            })
            .expect("the engine's port"),
        other => panic!("{other:?}"),
    };
    let health = format!("http://127.0.0.1:{port}/health");
    assert!(ureq::get(&health).call().is_ok());
    drop(bench);
    let began = Instant::now();
    while ureq::get(&health).call().is_ok() {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "the engine outlived Core"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
