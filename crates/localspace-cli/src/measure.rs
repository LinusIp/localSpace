//! `localspace measure`: the measurement kit (docs/DECISIONS.md, 2026-10-07
//! and 2026-10-08). One command a tester runs on a machine of theirs,
//! unattended: it reads the machine, runs the message script and the speed
//! measurements against one model through Core itself, hosted in this
//! process on a fresh data folder with the app's downloaded models, and
//! writes one results file for them to send back. Nothing is sent anywhere.
//!
//! The script is the message script of the laptop test, message for message
//! (until 2026-10-08 a Node file, `scripts/message-script.mjs`, retired
//! the day the two tables agreed on the 7B), so that the results compare
//! with every run before it. The answers are read by a person: nothing is
//! scored here. What Core alone knows of an answer, its figures, comes by
//! plain calls on it (`InProcess::with`), never on the wire.

use crate::output::{err, out};
use anyhow::{Context, Result, bail};
use localspace_core::transport::{Backend, InProcess, Incoming};
use localspace_core::turns::AnswerMeasure;
use localspace_core::{Config, Core, ReadTime, VisionPlace, audit, engine, hardware};
use localspace_proto as proto;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(clap::Args)]
pub struct MeasureArgs {
    /// The model, by its catalog id (`--list` names them).
    #[arg(long, value_name = "ID", required_unless_present = "list")]
    model: Option<String>,
    /// Name the catalog's models, say which are on this computer, and stop.
    #[arg(long)]
    list: bool,
    /// Where the results go: a file, or a folder for a file named after the
    /// model and the day. The current folder without it.
    #[arg(long, value_name = "FILE OR DIR")]
    out: Option<PathBuf>,
    /// Where the model files are: the app's models folder without it.
    #[arg(long, value_name = "DIR")]
    models: Option<PathBuf>,
    /// A folder with an organisation's own `catalog.json`, as `serve`'s [models] dir.
    #[arg(long, value_name = "DIR")]
    catalog: Option<PathBuf>,
    /// The fresh data folder to run on; one under the temporary folder without it.
    #[arg(long, value_name = "DIR")]
    data: Option<PathBuf>,
    /// Download the model when it is not on this computer. Its files, their
    /// sizes and the room left are printed first either way, and the
    /// download waits for Enter.
    #[arg(long)]
    download: bool,
    /// Answer the question before a download, for an unattended run.
    #[arg(long)]
    yes: bool,
    /// Longest one answer may take, in seconds; one that overruns is stopped
    /// and written down as such, and the run goes on.
    #[arg(long, value_name = "SECONDS", default_value_t = 600)]
    answer_seconds: u64,
    /// Longest the whole run may take; when it runs out, the kit writes what
    /// it has and ends with an error naming the step it was in.
    #[arg(long, value_name = "MINUTES", default_value_t = 240)]
    max_minutes: u64,
    /// Run the whole script with thinking on as well, for a machine that is
    /// available longer.
    #[arg(long)]
    thinking_full: bool,
    /// Stop the Continue step's answer after this many words.
    #[arg(long, value_name = "WORDS", default_value_t = 30)]
    stop_after: usize,
    /// End with an error when a message gets no answer or Continue does not
    /// carry through: what CI runs against the test engine.
    #[arg(long)]
    check: bool,
}

/// Longest a model may take to come up.
const ENGINE_START: Duration = Duration::from_secs(600);
/// Longest the engine may take to say its version.
const VERSION_ANSWER: Duration = Duration::from_secs(10);
/// How long a stopped answer is given to end after its limit ran out.
const STOP_GRACE: Duration = Duration::from_secs(15);
/// A download that brings nothing for this long has stopped.
const DOWNLOAD_STALL: Duration = Duration::from_secs(1800);

const CONTINUE_TEXT: &str = "Explain in about 500 words how a lighthouse works and why lighthouses were built where they were.";
const PICTURE_TEXT: &str = "What shapes and colours are in this picture? One sentence.";
/// A blue circle on a red square, 64 by 64: what a model that reads images
/// is asked about, so that a person reading the answer knows what was shown.
const PICTURE: &[u8] = include_bytes!("../assets/measure-picture.png");

const LONG_TEXT: &str = "The town of Harrowfield sits where two rivers meet, and for most of its history it lived from the water. In 1847 a wooden footbridge was the only crossing, and the ferryman, a man called Tobias Wren, charged a penny a head. The railway arrived in 1869 and with it the first brick warehouses along the east bank. By 1890 the town had three mills, a brewery and a population of eleven thousand. The great flood of March 1912 carried away the footbridge, two of the mills and forty-one houses; nobody died, because the miller's daughter, Ada Pellow, saw the water rising at four in the morning and rang the chapel bell until the street was awake. The stone bridge that replaced the footbridge was opened in 1915 and still carries the main road. After the second war the mills closed one by one, the last in 1971, and the warehouses stood empty until the 1990s, when they were turned into flats and workshops. Today the town has about nineteen thousand people, a weekly market on Thursdays, and a small museum in the old brewery whose most visited exhibit is the chapel bell.";

/// One message of the script, or a few that depend on each other.
struct Item {
    name: &'static str,
    turns: Vec<String>,
    /// In the short thinking-on pass: one of the five messages where
    /// thinking could matter (docs/DECISIONS.md, 2026-10-08).
    thinking: bool,
}

fn item(name: &'static str, text: impl Into<String>) -> Item {
    Item {
        name,
        turns: vec![text.into()],
        thinking: false,
    }
}

fn thinking_item(name: &'static str, text: impl Into<String>) -> Item {
    Item {
        thinking: true,
        ..item(name, text)
    }
}

/// The message script, message for message and in its order
/// (docs/DECISIONS.md, 2026-09-19, 2026-09-24, 2026-10-07 and 2026-10-08):
/// the things ordinary people type on a Friday afternoon, ten of them in
/// Russian and in Uzbek, the market's languages.
fn script() -> Vec<Item> {
    vec![
        item("a greeting", "Hi there!"),
        item("what can you do", "What can you do?"),
        item("a factual question", "How far is the Moon from the Earth?"),
        item(
            "a short email",
            "Write a short, polite email to my landlord asking him to fix the leaking tap in the kitchen.",
        ),
        item(
            "something to summarise",
            format!("Summarise this in two sentences:\n\n{LONG_TEXT}"),
        ),
        item(
            "a question about a detail of a longer text",
            format!(
                "Read this and then answer: who rang the bell, and in which year?\n\n{LONG_TEXT}"
            ),
        ),
        thinking_item("arithmetic", "What is 17 times 24?"),
        thinking_item(
            "a word problem",
            "I have 3 boxes with 12 eggs in each, and I break 5 eggs. How many whole eggs do I have left?",
        ),
        item(
            "a translation",
            "Translate into German: Where is the nearest train station?",
        ),
        item(
            "make this shorter",
            "Make this shorter: I am writing to let you know that, due to circumstances which are unfortunately outside of my control, I will regrettably not be able to attend the meeting that has been scheduled for Thursday afternoon.",
        ),
        item(
            "a question in Spanish",
            "¿Cuál es la capital de Argentina y por qué es conocida?",
        ),
        item(
            "a question in Russian",
            "Какая столица Франции и чем она знаменита?",
        ),
        item(
            "a short email in Russian",
            "Привет! Напиши, пожалуйста, короткое вежливое письмо коллеге с просьбой прислать отчёт за сентябрь до пятницы.",
        ),
        thinking_item(
            "a reasoning question in Russian",
            "Поезд выходит в 9:40 и идёт 2 часа 35 минут. Во сколько он прибудет?",
        ),
        item(
            "make this shorter, in Russian",
            "Сократи: В связи с тем, что в настоящее время у нас отсутствует возможность предоставить вам запрошенные документы, просим вас подождать до следующей недели.",
        ),
        item(
            "an explanation in Russian",
            "Объясни простыми словами, что такое НДС и как он считается.",
        ),
        item(
            "something it cannot know, in Russian",
            "Какой сегодня курс доллара к суму?",
        ),
        item(
            "a short email in Uzbek (Latin)",
            "Salom! Ertaga soat 10 dagi uchrashuv haqida hamkasbimga qisqa eslatma xati yozib bering.",
        ),
        thinking_item(
            "a reasoning question in Uzbek (Latin)",
            "Bir kilogramm olma 12 000 so'm tursa, 2,5 kilogramm qancha turadi?",
        ),
        item(
            "an explanation in Uzbek (Latin)",
            "Oddiy so'zlar bilan tushuntiring: inflyatsiya nima va u narxlarga qanday ta'sir qiladi?",
        ),
        item(
            "a factual question in Uzbek (Cyrillic)",
            "Ўзбекистоннинг пойтахти қайси шаҳар ва у нимаси билан машҳур?",
        ),
        item(
            "make this shorter, in Uzbek (Cyrillic)",
            "Қуйидаги матнни қисқартиринг: Ҳурматли мижоз, сизга маълум қиламизки, буюртмангиз ҳозирги вақтда қайта ишланмоқда ва яқин кунларда етказиб берилади.",
        ),
        item(
            "something deliberately vague",
            "Can you help me with my thing?",
        ),
        item(
            "a packing list",
            "Give me a packing list for a three-day hiking trip in autumn.",
        ),
        item(
            "a little code",
            "Write a Python function that checks whether a number is prime.",
        ),
        thinking_item(
            "something it cannot know",
            "What is the weather like in Lisbon today?",
        ),
        item(
            "something only a Store tool can do",
            "Draw me a picture of a cat.",
        ),
        Item {
            name: "three turns, each depending on the one before",
            turns: vec![
                "Give me three ideas for a weekend trip from Berlin, one line each.".into(),
                "Tell me more about the second one.".into(),
                "Roughly what would that cost for two people?".into(),
            ],
            thinking: false,
        },
    ]
}

// ---------------------------------------------------------------------------
// Core, hosted here
// ---------------------------------------------------------------------------

/// Core on its own thread, asked by requests as the app asks it, and by
/// plain calls for what no request carries.
struct Kit {
    backend: InProcess,
    /// The words of each chat as they came, by chat.
    shown: HashMap<String, String>,
    /// When a download's progress was last said.
    progress_said: Instant,
    /// The models whose files, already in the folder, Core has looked at
    /// (its `checked` event): after that, an entry still not installed has
    /// files that are not the published ones.
    checked: HashSet<String>,
    /// Longest one answer may take (`--answer-seconds`).
    answer: Duration,
    /// When the whole run's limit runs out (`--max-minutes`), and the limit.
    deadline: Instant,
    max_minutes: u64,
}

impl Kit {
    fn new(core: Core, answer: Duration, max_minutes: u64) -> Kit {
        Kit {
            backend: InProcess::spawn(core),
            shown: HashMap::new(),
            progress_said: Instant::now(),
            checked: HashSet::new(),
            answer,
            deadline: Instant::now() + Duration::from_secs(max_minutes * 60),
            max_minutes,
        }
    }

    /// The whole run's limit, looked at in every wait: when it has run out,
    /// the error names the step, and the run writes what it has.
    fn within_limit(&self, step: &str) -> Result<()> {
        if Instant::now() >= self.deadline {
            bail!(
                "the {}-minute limit for the whole run ran out during {step}",
                self.max_minutes
            );
        }
        Ok(())
    }

    /// Everything that came from Core: the events are kept, the answers
    /// returned with their request's id.
    fn pump(&mut self) -> Vec<(u64, proto::Response)> {
        let mut answers = Vec::new();
        for incoming in self.backend.poll() {
            match incoming {
                Incoming::Response { id, response } => answers.push((id, response)),
                Incoming::Event(event) => self.heard(event),
            }
        }
        answers
    }

    fn heard(&mut self, event: proto::Event) {
        match event {
            proto::Event::AssistantDelta { conversation, text } => {
                self.shown.entry(conversation).or_default().push_str(&text);
            }
            // Core has looked at the files of a model that were already in
            // the folder, and the entry now says whether they count.
            proto::Event::ModelProgress { id, stage, .. } if stage == "checked" => {
                self.checked.insert(id);
            }
            // A download's progress, said every ten seconds and at its end.
            proto::Event::ModelProgress {
                done_bytes,
                total_bytes,
                stage,
                ..
            } if self.progress_said.elapsed() >= Duration::from_secs(10) || stage == "done" => {
                self.progress_said = Instant::now();
                err!("  {stage}: {} of {}", gb(done_bytes), gb(total_bytes));
            }
            _ => {}
        }
    }

    /// One request, answered. A turn is answered beside Core's queue, so
    /// nothing here waits on a model.
    fn call(&mut self, req: proto::Request) -> Result<proto::Response> {
        let id = self.backend.request(req);
        let began = Instant::now();
        loop {
            for (got, response) in self.pump() {
                if got == id {
                    return Ok(response);
                }
            }
            if began.elapsed() > self.answer {
                bail!("Core did not answer within {} s", self.answer.as_secs());
            }
            self.within_limit("a request to Core")?;
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A plain call on Core, on its thread.
    fn with<T: Send + 'static>(
        &self,
        call: impl FnOnce(&mut Core) -> T + Send + 'static,
    ) -> Result<T> {
        self.backend.with(call).context("Core's thread has ended")
    }

    fn entries(&mut self) -> Result<Vec<proto::ModelCatalogEntry>> {
        match self.call(proto::Request::ListModelCatalog)? {
            proto::Response::ModelCatalog { entries } => Ok(entries),
            proto::Response::Error { message } => bail!("the catalog could not be read: {message}"),
            other => bail!("unexpected answer to the catalog: {other:?}"),
        }
    }

    fn entry(&mut self, id: &str) -> Result<proto::ModelCatalogEntry> {
        self.entries()?
            .into_iter()
            .find(|e| e.id == id)
            .with_context(|| {
                format!("no model `{id}` in the catalog; `localspace measure --list` names them")
            })
    }

    fn new_conversation(&mut self) -> Result<String> {
        match self.call(proto::Request::NewConversation)? {
            proto::Response::Conversations { current, .. } => Ok(current),
            other => bail!("unexpected answer to a new chat: {other:?}"),
        }
    }

    fn transcript(&mut self) -> Result<Vec<proto::ChatMessage>> {
        match self.call(proto::Request::GetTranscript)? {
            proto::Response::Transcript { messages } => Ok(messages),
            other => bail!("unexpected answer to the transcript: {other:?}"),
        }
    }

    fn turns_empty(&mut self) -> Result<bool> {
        match self.call(proto::Request::ListTurns)? {
            proto::Response::Turns { list } => Ok(list.is_empty()),
            other => bail!("unexpected answer to the turns: {other:?}"),
        }
    }

    /// Waits until nothing is being written or waits to be; false when that
    /// took longer than an answer may, and then the answer in
    /// `conversation` is stopped, so that the next message is not queued
    /// behind it.
    fn answer_ends(&mut self, conversation: &str) -> Result<bool> {
        let began = Instant::now();
        while began.elapsed() < self.answer {
            if self.turns_empty()? {
                return Ok(true);
            }
            self.within_limit("an answer")?;
            std::thread::sleep(Duration::from_millis(250));
        }
        err!(
            "  the answer took longer than {} s: stopped",
            self.answer.as_secs()
        );
        self.call(proto::Request::CancelTurn {
            conversation: Some(conversation.to_string()),
        })?;
        let stopping = Instant::now();
        while stopping.elapsed() < STOP_GRACE && !self.turns_empty()? {
            std::thread::sleep(Duration::from_millis(250));
        }
        Ok(false)
    }

    fn engine(&self) -> Result<proto::EngineState> {
        self.with(|core| core.environment().engine)
    }

    /// Start the model, and wait until it answers: the seconds that took.
    fn load(&mut self, id: &str) -> Result<f32> {
        match self.call(proto::Request::LoadModel { id: id.to_string() })? {
            proto::Response::Ok => {}
            proto::Response::Error { message } => bail!("the model could not be loaded: {message}"),
            other => bail!("unexpected answer to the load: {other:?}"),
        }
        let began = Instant::now();
        loop {
            let state = self.engine()?;
            if state.running {
                return Ok(began.elapsed().as_secs_f32());
            }
            if !state.loading {
                bail!("the engine did not come up: {}", state.detail);
            }
            if began.elapsed() > ENGINE_START {
                bail!(
                    "the engine did not come up in {} minutes",
                    ENGINE_START.as_secs() / 60
                );
            }
            self.within_limit("the model's start")?;
            self.pump();
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn unload(&mut self) -> Result<()> {
        self.call(proto::Request::UnloadModel)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The runs
// ---------------------------------------------------------------------------

/// One message answered, with what Core measured of the answer.
struct Answered {
    text: String,
    seconds: f32,
    reply: String,
    tools: Vec<String>,
    error: Option<String>,
    measure: Option<AnswerMeasure>,
}

/// Sends one message by `send` in the chat on screen and reads the answer.
fn ask(
    kit: &mut Kit,
    conversation: &str,
    text: &str,
    send: impl FnOnce(&mut Kit) -> Result<proto::Response>,
) -> Result<Answered> {
    let asked = Instant::now();
    let mut error = match send(kit)? {
        proto::Response::Error { message } => Some(message),
        _ => None,
    };
    let ended = if error.is_some() {
        false
    } else {
        kit.answer_ends(conversation)?
    };
    let seconds = asked.elapsed().as_secs_f32();
    let messages = kit.transcript()?;
    // What this turn added: everything after the last message of the person.
    let since_user = messages
        .iter()
        .rposition(|m| m.role == proto::Role::User)
        .map_or(0, |i| i + 1);
    let since = &messages[since_user..];
    let reply = since
        .iter()
        .rev()
        .find(|m| m.role == proto::Role::Assistant)
        .map(|m| m.content.trim().to_string())
        .unwrap_or_default();
    let tools: Vec<String> = since
        .iter()
        .flat_map(|m| m.tool_calls.iter().map(|c| c.tool.clone()))
        .collect();
    if error.is_none() && !ended {
        error = Some(format!("no end within {} s; stopped", kit.answer.as_secs()));
    }
    let chat = conversation.to_string();
    let measure = kit.with(move |core| core.answer_measure(&chat))?;
    Ok(Answered {
        text: text.to_string(),
        seconds,
        reply,
        tools,
        error,
        measure,
    })
}

fn ask_in_words(kit: &mut Kit, conversation: &str, text: &str) -> Result<Answered> {
    let message = text.to_string();
    ask(kit, conversation, text, move |kit| {
        kit.call(proto::Request::SendMessage {
            text: message,
            conversation: None,
        })
    })
}

/// The Continue step: an answer stopped after a number of words, then
/// carried on, and the join written down for a person to read.
struct Carried {
    stopped_after: usize,
    kept: String,
    added: String,
    whole: Option<bool>,
    note: String,
}

fn carry_on(kit: &mut Kit, stop_after: usize) -> Result<Carried> {
    let chat = kit.new_conversation()?;
    kit.call(proto::Request::SendMessage {
        text: CONTINUE_TEXT.into(),
        conversation: None,
    })?;
    let asked = Instant::now();
    while asked.elapsed() < kit.answer {
        kit.pump();
        if words(kit.shown.get(&chat).map_or("", String::as_str)) >= stop_after {
            break;
        }
        if kit.turns_empty()? {
            break;
        }
        kit.within_limit("Continue")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut carried = Carried {
        stopped_after: words(kit.shown.get(&chat).map_or("", String::as_str)),
        kept: String::new(),
        added: String::new(),
        whole: None,
        note: String::new(),
    };
    kit.call(proto::Request::CancelTurn {
        conversation: Some(chat.clone()),
    })?;
    kit.answer_ends(&chat)?;
    let stopped = kit.transcript()?.pop();
    match stopped {
        Some(stopped) if stopped.stopped => {
            carried.kept = stopped.content;
            kit.call(proto::Request::ContinueAnswer {
                conversation: Some(chat.clone()),
            })?;
            if !kit.answer_ends(&chat)? {
                carried.note = format!("no end within {} s; stopped", kit.answer.as_secs());
            }
            if let Some(answer) = kit.transcript()?.pop() {
                carried.whole = Some(!answer.stopped);
                carried.added = match answer.content.strip_prefix(carried.kept.as_str()) {
                    Some(added) => added.to_string(),
                    None => answer.content.clone(),
                };
            }
        }
        _ => {
            carried.note =
                "the answer ended before it could be stopped: nothing to carry on".to_string();
        }
    }
    err!(
        "continue: stopped after {} words; {}",
        carried.stopped_after,
        if carried.note.is_empty() {
            format!("{} words added", words(&carried.added))
        } else {
            carried.note.clone()
        }
    );
    Ok(carried)
}

/// One pass of the script: with thinking off, on, or left to the model.
struct Pass {
    title: String,
    thinking: Option<bool>,
    items: Vec<(&'static str, Vec<Answered>)>,
    carried: Option<Carried>,
}

/// One pass, written into `progress` as it goes, so that a run that ends
/// early keeps every answer that came.
fn run_pass(
    kit: &mut Kit,
    progress: &mut Progress,
    title: &str,
    items: &[Item],
    thinking: Option<bool>,
    with_continue: bool,
    stop_after: usize,
) -> Result<()> {
    err!("{title}");
    kit.within_limit(title)?;
    kit.with(move |core| core.set_thinking(thinking))?;
    progress.passes.push(Pass {
        title: title.to_string(),
        thinking,
        items: Vec::new(),
        carried: None,
    });
    for item in items {
        let chat = kit.new_conversation()?;
        let mut turns = Vec::new();
        for text in &item.turns {
            let turn = ask_in_words(kit, &chat, text)?;
            err!(
                "{}: {:.1} s{}{}",
                item.name,
                turn.seconds,
                if turn.tools.is_empty() {
                    String::new()
                } else {
                    format!(", tools: {}", turn.tools.join(", "))
                },
                if turn.reply.is_empty() {
                    "  - NO REPLY"
                } else {
                    ""
                }
            );
            turns.push(turn);
        }
        if let Some(pass) = progress.passes.last_mut() {
            pass.items.push((item.name, turns));
        }
    }
    if with_continue {
        err!("continue");
        let carried = carry_on(kit, stop_after)?;
        if let Some(pass) = progress.passes.last_mut() {
            pass.carried = Some(carried);
        }
    }
    Ok(())
}

/// The picture, with the vision file where `place` says.
struct Pictured {
    place: &'static str,
    /// The seconds the model took to come up there, or why it did not.
    loaded: Result<f32, String>,
    answered: Option<Answered>,
}

fn picture_pass(kit: &mut Kit, id: &str, place: VisionPlace, reload: bool) -> Result<Pictured> {
    let where_ = match place {
        VisionPlace::Processor => "on the processor",
        VisionPlace::Card => "on the card",
    };
    err!("the picture, with the vision file {where_}");
    let loaded = if reload {
        kit.unload()?;
        kit.with(move |core| core.set_vision_place(place))?;
        kit.load(id).map_err(|e| format!("{e:#}"))
    } else {
        Ok(0.0)
    };
    if let Err(why) = &loaded {
        err!("  the model did not come up with the vision file {where_}: {why}");
        return Ok(Pictured {
            place: where_,
            loaded,
            answered: None,
        });
    }
    kit.with(|core| core.set_thinking(Some(false)))?;
    let chat = kit.new_conversation()?;
    let answered = ask(kit, &chat, PICTURE_TEXT, |kit| {
        kit.with(|core| core.send_picture(PICTURE_TEXT, PICTURE.to_vec(), None))
    })?;
    err!(
        "  {:.1} s{}",
        answered.seconds,
        match &answered.error {
            Some(e) => format!(", {e}"),
            None => String::new(),
        }
    );
    Ok(Pictured {
        place: where_,
        loaded,
        answered: Some(answered),
    })
}

// ---------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------

/// What the results file is written from, filled in as the run goes: a run
/// that ends early writes what it has (docs/DECISIONS.md, 2026-10-08).
struct Progress {
    id: String,
    day: String,
    machine: String,
    facts: Vec<String>,
    engine: String,
    data_dir: PathBuf,
    entry: Option<proto::ModelCatalogEntry>,
    files: Vec<String>,
    ready_after: Option<f32>,
    passes: Vec<Pass>,
    pictures: Vec<Pictured>,
    read_time: Option<Result<ReadTime, String>>,
    /// Why the run ended before its last step, if it did.
    ended: Option<String>,
}

pub fn run(args: MeasureArgs) -> Result<()> {
    let models_dir = match &args.models {
        Some(dir) => dir.clone(),
        None => localspace_core::default_data_dir()
            .map(|d| d.join("models"))
            .context("no models folder: pass --models <dir>")?,
    };
    std::fs::create_dir_all(&models_dir)
        .with_context(|| format!("creating {}", models_dir.display()))?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let day = audit::day_of(now_ms);
    let data_dir = match &args.data {
        Some(dir) => dir.clone(),
        None => std::env::temp_dir()
            .join("localspace-measure")
            .join(format!(
                "{}-{now_ms}",
                args.model.as_deref().unwrap_or("list")
            )),
    };
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating {}", data_dir.display()))?;

    let mut cfg = Config::personal(&localspace_server::whoami());
    cfg.data_dir = Some(data_dir.clone());
    cfg.model_files_dir = Some(models_dir.clone());
    cfg.models_dir = args.catalog.clone();
    cfg.show_hidden_models = true;
    let binary = engine::find_binary(cfg.llama_server.as_deref(), Some(&data_dir));
    let mut kit = Kit::new(
        Core::new(cfg)?,
        Duration::from_secs(args.answer_seconds.max(1)),
        args.max_minutes.max(1),
    );

    if args.list {
        for e in kit.entries()? {
            out!(
                "{:<32} {:<24} {}",
                e.id,
                e.title,
                if e.installed {
                    "on this computer"
                } else {
                    "not on this computer"
                }
            );
        }
        return Ok(());
    }
    let id = args.model.clone().context("--model <id> names the model")?;

    // The computer, as the app sees it: in the results, and said now.
    err!("reading the computer");
    let found = hardware::detect(binary.as_deref(), Some(&models_dir), &[]);
    err!("computer: {}", found.sentence());
    let engine_words = match &binary {
        Some(path) => format!(
            "{} ({})",
            engine_version(path),
            localspace_core::without_the_home(&path.display().to_string())
        ),
        None => {
            "not installed: put llama-server under <data>/engines/ or engine/ beside this program"
                .into()
        }
    };
    if binary.is_none() {
        bail!("{engine_words}");
    }

    let mut progress = Progress {
        id: id.clone(),
        day,
        machine: found.sentence(),
        facts: found.notes(),
        engine: engine_words,
        data_dir: data_dir.clone(),
        entry: None,
        files: Vec::new(),
        ready_after: None,
        passes: Vec::new(),
        pictures: Vec::new(),
        read_time: None,
        ended: None,
    };
    let outcome = steps(
        &mut kit,
        &args,
        &id,
        &models_dir,
        found.disk_free_mib,
        &mut progress,
    );
    if let Err(e) = &outcome {
        progress.ended = Some(format!("{e:#}"));
        err!("the run ended early: {e:#}");
    }
    let _ = kit.unload();

    err!("writing the results file");
    let text = report(&progress);
    let path = out_path(args.out.as_deref(), &id, &progress.day);
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    out!(
        "the run's folder, with the engine's log: {}",
        data_dir.display()
    );
    // The last line is the file to send back.
    out!("{}", path.display());

    let mut failed = outcome.is_err();
    if args.check {
        let (answered, of) = answered_count(&progress.passes);
        let carried = progress
            .passes
            .first()
            .and_then(|p| p.carried.as_ref())
            .is_some_and(|c| c.note.is_empty() && c.whole == Some(true));
        err!(
            "measure: {answered} of {of} messages answered; Continue {}",
            if carried {
                "carried through"
            } else {
                "did not carry through"
            }
        );
        failed |= answered < of || !carried;
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

/// The steps of a run, each said before it begins, into `progress` as they
/// come.
fn steps(
    kit: &mut Kit,
    args: &MeasureArgs,
    id: &str,
    models_dir: &Path,
    disk_free_mib: Option<u64>,
    progress: &mut Progress,
) -> Result<()> {
    err!("checking the model's files");
    let entry = present(kit, id, models_dir, args.download, args.yes, disk_free_mib)?;
    progress.files = files_of(&entry, models_dir);
    let vision = has_vision_file(&entry);
    err!("loading {}", entry.title);
    progress.entry = Some(entry);
    let ready_after = kit.load(id)?;
    progress.ready_after = Some(ready_after);
    err!("ready after {ready_after:.0} s");

    let items = script();
    let five: Vec<Item> = script().into_iter().filter(|i| i.thinking).collect();
    run_pass(
        kit,
        progress,
        "Thinking off, the whole script",
        &items,
        Some(false),
        true,
        args.stop_after,
    )?;
    run_pass(
        kit,
        progress,
        "Thinking on, the five messages where it could matter",
        &five,
        Some(true),
        false,
        args.stop_after,
    )?;
    if args.thinking_full {
        run_pass(
            kit,
            progress,
            "Thinking on, the whole script",
            &items,
            Some(true),
            true,
            args.stop_after,
        )?;
    }

    // The engine's reading of a few new tokens, with the cache warm.
    err!("the engine's reading");
    kit.within_limit("the engine's reading")?;
    let read_time = kit
        .with(|core| core.read_time())?
        .map_err(|e| format!("{e:#}"));
    if let Err(why) = &read_time {
        err!("  not measured: {why}");
    }
    progress.read_time = Some(read_time);

    // The picture, for a model with a vision file: where the model started
    // (the processor), then on the card.
    if vision {
        kit.within_limit("the picture")?;
        let on_the_processor = picture_pass(kit, id, VisionPlace::Processor, false)?;
        progress.pictures.push(on_the_processor);
        kit.within_limit("the picture on the card")?;
        let on_the_card = picture_pass(kit, id, VisionPlace::Card, true)?;
        progress.pictures.push(on_the_card);
    }
    Ok(())
}

/// The model's entry once its files are on this computer and checked:
/// waits for Core's look at files that are already in the folder (it looks
/// at every model's files in turn, and a large folder takes minutes),
/// downloads when asked to and the person has pressed Enter (or `--yes`
/// answered for them), and otherwise says what is missing and stops.
fn present(
    kit: &mut Kit,
    id: &str,
    models_dir: &Path,
    download: bool,
    yes: bool,
    disk_free_mib: Option<u64>,
) -> Result<proto::ModelCatalogEntry> {
    let began = Instant::now();
    let mut asked_for = false;
    let mut said_checking = false;
    let mut last_progress = Instant::now();
    loop {
        kit.within_limit(if asked_for {
            "the download"
        } else {
            "the model's files"
        })?;
        let entry = kit.entry(id)?;
        if entry.installed {
            return Ok(entry);
        }
        // Every file of the entry is in the folder, whole: Core's look at
        // them is pending or under way, or it found them not to be the
        // published ones.
        let here = entry.files.iter().all(|f| {
            models_dir.join(f).is_file() && !models_dir.join(format!("{f}.part")).exists()
        });
        let stage = entry.download.as_ref().map(|d| d.stage.clone());
        match stage.as_deref() {
            Some(stage) if stage.starts_with("failed") => {
                bail!("the download of {} failed: {stage}", entry.title)
            }
            Some("verifying") | Some("queued") | Some("checked") => {}
            Some(stage) if stage.starts_with("downloading") => last_progress = Instant::now(),
            _ if asked_for => {}
            _ if here && kit.checked.contains(id) => bail!(
                "the files of {} in {} are not the published ones (their SHA-256 differs); \
                 move them away and run again with --download",
                entry.title,
                models_dir.display()
            ),
            _ if here => {
                if !said_checking {
                    said_checking = true;
                    err!(
                        "  the files are here; waiting for Core's check of them (it checks every model's files in the folder, in turn)"
                    );
                }
            }
            _ if began.elapsed() < Duration::from_secs(5) => {}
            // Not on this computer, or stopped part-way.
            _ => {
                let needs = entry
                    .bytes
                    .saturating_sub(entry.download.as_ref().map_or(0, |d| d.done_bytes));
                err!(
                    "{} is not on this computer: {} ({}) still to fetch into {}; {}",
                    entry.title,
                    entry.files.join(", "),
                    gb(needs),
                    models_dir.display(),
                    match disk_free_mib {
                        Some(mib) => format!(
                            "{:.1} GB free there now, {:.1} GB afterwards",
                            mib as f64 / 1024.0,
                            mib as f64 / 1024.0 - needs as f64 / (1u64 << 30) as f64
                        ),
                        None => "the free space there could not be read".to_string(),
                    }
                );
                if !entry.no_room.is_empty() {
                    bail!("{}", entry.no_room);
                }
                if !download {
                    bail!("run again with --download to fetch it");
                }
                // The tester's bandwidth and disk: nothing is fetched until
                // they say so (docs/DECISIONS.md, 2026-10-08).
                if !yes
                    && !enter_pressed("Press Enter to download it, or close this window to stop.")?
                {
                    bail!("nobody answered; run again with --yes to download without asking");
                }
                match kit.call(proto::Request::DownloadModel { id: id.to_string() })? {
                    proto::Response::Error { message } => bail!("{message}"),
                    _ => err!("downloading"),
                }
                asked_for = true;
                last_progress = Instant::now();
            }
        }
        if asked_for && last_progress.elapsed() > DOWNLOAD_STALL {
            bail!(
                "the download brought nothing for {} minutes",
                DOWNLOAD_STALL.as_secs() / 60
            );
        }
        kit.pump();
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Asks, and waits for a line: true when one came, false when the input
/// ended without one (no terminal, or the window closed).
fn enter_pressed(question: &str) -> Result<bool> {
    err!("{question}");
    let mut line = String::new();
    let read = std::io::stdin()
        .read_line(&mut line)
        .context("reading the answer")?;
    Ok(read > 0)
}

fn has_vision_file(entry: &proto::ModelCatalogEntry) -> bool {
    entry.files.iter().any(|f| f.contains("mmproj"))
}

/// The entry's files with the digests Core found them to have, from the
/// record it keeps beside the files (`verified.json`).
fn files_of(entry: &proto::ModelCatalogEntry, models_dir: &Path) -> Vec<String> {
    let verified: serde_json::Value = std::fs::read_to_string(models_dir.join("verified.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    entry
        .files
        .iter()
        .map(|file| {
            let stamp = &verified[file];
            match (stamp["bytes"].as_u64(), stamp["sha256"].as_str()) {
                (Some(bytes), Some(sha)) => format!("`{file}`, {bytes} bytes, SHA-256 `{sha}`"),
                _ => format!("`{file}`, not checked"),
            }
        })
        .collect()
}

/// The engine's own word on its version: `llama-server --version`, given
/// ten seconds. An engine that serves instead of answering (the test
/// engine did, once, and the kit waited for ever) is ended, and the version
/// goes down as not known.
fn engine_version(binary: &Path) -> String {
    use std::process::Stdio;
    let mut child = match localspace_core::child::command(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return format!("version not known: {e}"),
    };
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if began.elapsed() < VERSION_ANSWER => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return format!(
                    "version not known: no answer to --version in {} s",
                    VERSION_ANSWER.as_secs()
                );
            }
            Err(e) => return format!("version not known: {e}"),
        }
    }
    let Ok(output) = child.wait_with_output() else {
        return "version not known".to_string();
    };
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    text.lines()
        .find(|line| line.contains("version"))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "version not known".to_string())
}

/// Where the results go: into `out` when it is a folder, or names one (no
/// extension; made if it is not there), else the file it names; the
/// current folder without it.
fn out_path(out: Option<&Path>, id: &str, day: &str) -> PathBuf {
    let name = format!("localSpace-measure-{id}-{day}.md");
    match out {
        Some(path) if path.is_dir() || path.extension().is_none() => {
            let _ = std::fs::create_dir_all(path);
            path.join(name)
        }
        Some(path) => path.to_path_buf(),
        None => PathBuf::from(name),
    }
}

fn words(s: &str) -> usize {
    s.split_whitespace().count()
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64)
}

fn answered_count(passes: &[Pass]) -> (usize, usize) {
    let turns: Vec<&Answered> = passes
        .iter()
        .flat_map(|p| p.items.iter().flat_map(|(_, turns)| turns.iter()))
        .collect();
    let answered = turns
        .iter()
        .filter(|t| t.error.is_none() && !t.reply.is_empty())
        .count();
    (answered, turns.len())
}

// ---------------------------------------------------------------------------
// The results file
// ---------------------------------------------------------------------------

/// A table cell: no bar, no line break.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ⏎ ")
}

fn seconds(d: Option<Duration>) -> String {
    match d {
        Some(d) => format!("{:.1} s", d.as_secs_f32()),
        None => "none".to_string(),
    }
}

/// What the measure says of the thinking before an answer.
fn thinking_cell(measure: Option<&AnswerMeasure>) -> String {
    let Some(m) = measure else {
        return "not measured".to_string();
    };
    let came = match (m.thinking_words, m.thinking_began, m.thinking_ended) {
        (0, _, _) => "none".to_string(),
        (n, Some(began), Some(ended)) => format!(
            "{n} words, from {:.1} s to {:.1} s",
            began.as_secs_f32(),
            ended.as_secs_f32()
        ),
        (n, _, _) => format!("{n} words"),
    };
    match m.thinking_asked {
        Some(false) => format!("off: {came}"),
        Some(true) => format!("on: {came}"),
        None => format!("the model's default: {came}"),
    }
}

/// The engine's own figures for the step.
fn figures_cell(measure: Option<&AnswerMeasure>) -> String {
    let Some(timings) = measure.and_then(|m| m.timings) else {
        return "no figures".to_string();
    };
    let rate = if timings.predicted_ms > 0.0 {
        format!(
            " at {:.1} a second",
            f64::from(timings.predicted_n) / (timings.predicted_ms / 1000.0)
        )
    } else {
        String::new()
    };
    format!(
        "read {} in {:.1} s; wrote {}{rate}",
        timings.prompt_n,
        timings.prompt_ms / 1000.0,
        timings.predicted_n
    )
}

fn table(lines: &mut Vec<String>, items: &[(&'static str, Vec<Answered>)]) {
    lines.push("| Message | Time | First word | Thinking | Read / wrote | Tools reached for | What came back |".into());
    lines.push("|---|---|---|---|---|---|---|".into());
    for (name, turns) in items {
        for (i, t) in turns.iter().enumerate() {
            let label = if turns.len() > 1 {
                format!("{name} ({}): {}", i + 1, t.text)
            } else if t.text.chars().count() > 90 {
                format!("{name}: {}…", t.text.chars().take(90).collect::<String>())
            } else {
                format!("{name}: {}", t.text)
            };
            let back = match (&t.error, t.reply.is_empty()) {
                (Some(error), _) => format!("**{error}**"),
                (None, true) => "**no reply**".to_string(),
                (None, false) => t.reply.clone(),
            };
            lines.push(format!(
                "| {} | {:.1} s | {} | {} | {} | {} | {} |",
                cell(&label),
                t.seconds,
                seconds(t.measure.as_ref().and_then(|m| m.first_word)),
                thinking_cell(t.measure.as_ref()),
                figures_cell(t.measure.as_ref()),
                if t.tools.is_empty() {
                    "none".to_string()
                } else {
                    t.tools.join(", ")
                },
                cell(&back)
            ));
        }
    }
}

fn continue_lines(lines: &mut Vec<String>, carried: &Carried) {
    lines.push(format!(
        "**Continue**: \"{CONTINUE_TEXT}\" stopped after {} words, then carried on.",
        carried.stopped_after
    ));
    if !carried.note.is_empty() {
        lines.push(format!("**{}**", carried.note));
        return;
    }
    let last = carried
        .kept
        .split_whitespace()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ");
    let next = carried
        .added
        .split_whitespace()
        .take(16)
        .collect::<Vec<_>>()
        .join(" ");
    // An answer that begins again says its opening words a second time.
    let opening = carried
        .kept
        .split_whitespace()
        .take(6)
        .collect::<Vec<_>>()
        .join(" ");
    let began_again = !opening.is_empty()
        && carried
            .added
            .split_whitespace()
            .take(60)
            .collect::<Vec<_>>()
            .join(" ")
            .contains(&opening);
    lines.push(format!(
        "The join: \"…{}\" ‖ \"{}…\". {} {} Whether the engine said the handed words again is in the run's log: a line \"continue: the engine did not begin its reply …\" means it did not, and nothing was dropped.",
        cell(&last),
        cell(&next),
        if began_again {
            "**It began its answer again.**"
        } else {
            "It did not begin again."
        },
        if carried.whole == Some(true) {
            "The answer ended whole."
        } else {
            "**The answer did not end whole.**"
        }
    ));
}

fn report(run: &Progress) -> String {
    let mut lines = Vec::new();
    let title = run
        .entry
        .as_ref()
        .map_or_else(|| run.id.clone(), |e| e.title.clone());
    lines.push(format!("### {title}"));
    lines.push(String::new());
    let ready = match (run.ready_after, &run.entry) {
        (Some(secs), _) => format!(" Ready after {secs:.0} s."),
        (None, Some(_)) => " The model did not come up.".to_string(),
        (None, None) => " The model was not reached.".to_string(),
    };
    let says = match &run.entry {
        Some(e) => format!(
            " The app says of it here: \"{}{}\"; {}",
            e.verdict_label,
            if e.speed.is_empty() {
                String::new()
            } else {
                format!(" · {}", e.speed)
            },
            e.placement
        ),
        None => String::new(),
    };
    lines.push(format!(
        "`{}`, through Core on a fresh data folder, by `localspace measure` ({}) on {}.{ready}{says}",
        run.id,
        crate::BUILD_ID,
        run.day
    ));
    if let Some(why) = &run.ended {
        lines.push(String::new());
        lines.push(format!(
            "**The run ended early: {why}.** What came before it is below."
        ));
    }
    lines.push(String::new());
    lines.push(format!("- Computer: {}", run.machine));
    for note in &run.facts {
        lines.push(format!("  - {note}"));
    }
    lines.push(format!("- Engine: {}", run.engine));
    lines.push(format!(
        "- Files: {}",
        if run.files.is_empty() {
            "not looked at".to_string()
        } else {
            run.files.join("; ")
        }
    ));
    lines.push(format!(
        "- The run's folder: `{}`",
        localspace_core::without_the_home(&run.data_dir.display().to_string())
    ));
    lines.push(String::new());
    for pass in &run.passes {
        lines.push(format!("#### {}", pass.title));
        lines.push(String::new());
        lines.push(format!(
            "Thinking {}.",
            match pass.thinking {
                Some(true) => "asked on",
                Some(false) => "asked off",
                None => "left to the model",
            }
        ));
        lines.push(String::new());
        table(&mut lines, &pass.items);
        lines.push(String::new());
        if let Some(carried) = &pass.carried {
            continue_lines(&mut lines, carried);
            lines.push(String::new());
        }
    }
    if !run.pictures.is_empty() {
        lines.push("#### The picture: a blue circle on a red square, 64 by 64".into());
        lines.push(String::new());
        lines.push(
            "| The vision file | Came up | Time | First word | Read / wrote | What came back |"
                .into(),
        );
        lines.push("|---|---|---|---|---|---|".into());
        for p in &run.pictures {
            let (came_up, time, first, figures, back) = match (&p.loaded, &p.answered) {
                (Err(why), _) => (
                    format!("no: {}", cell(why)),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                ),
                (Ok(secs), Some(a)) => (
                    if *secs > 0.0 {
                        format!("yes, after {secs:.0} s")
                    } else {
                        "yes".to_string()
                    },
                    format!("{:.1} s", a.seconds),
                    seconds(a.measure.as_ref().and_then(|m| m.first_word)),
                    figures_cell(a.measure.as_ref()),
                    match (&a.error, a.reply.is_empty()) {
                        (Some(error), _) => format!("**{}**", cell(error)),
                        (None, true) => "**no reply**".to_string(),
                        (None, false) => cell(&a.reply),
                    },
                ),
                (Ok(_), None) => (
                    "yes".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "-".to_string(),
                    "**not asked**".to_string(),
                ),
            };
            lines.push(format!(
                "| {} | {came_up} | {time} | {first} | {figures} | {back} |",
                p.place
            ));
        }
        lines.push(String::new());
    }
    lines.push("#### The engine's reading, with its cache warm".into());
    lines.push(String::new());
    match &run.read_time {
        Some(Ok(read)) => {
            lines.push(format!(
                "The prompt's stable part, held in the engine's cache since the warm-up: {} tokens read anew in {:.2} s. The engine counts what it read anew, not what it had.",
                read.stable_tokens,
                read.stable_ms / 1000.0
            ));
            for about in [10u32, 30, 100] {
                let reads: Vec<_> = read.samples.iter().filter(|s| s.about == about).collect();
                if reads.is_empty() {
                    continue;
                }
                let mut tokens: Vec<u32> = reads.iter().map(|s| s.tokens).collect();
                let mut ms: Vec<f64> = reads.iter().map(|s| s.ms).collect();
                tokens.sort_unstable();
                ms.sort_by(|a, b| a.total_cmp(b));
                lines.push(format!(
                    "About {about} new tokens: read {} tokens in {:.2} s (the median of {}; each {}).",
                    tokens[tokens.len() / 2],
                    ms[ms.len() / 2] / 1000.0,
                    reads.len(),
                    reads
                        .iter()
                        .map(|s| format!("{}: {:.2} s", s.tokens, s.ms / 1000.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        Some(Err(why)) => lines.push(format!("Not measured: {why}")),
        None => lines.push("Not reached.".to_string()),
    }
    lines.push(String::new());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_is_the_message_scripts_message_for_message() {
        let items = script();
        assert_eq!(items.len(), 28);
        assert_eq!(items.iter().map(|i| i.turns.len()).sum::<usize>(), 30);
        // The five messages of the short thinking-on pass: the sum, the word
        // problem, the weather, the Russian and the Uzbek reasoning questions.
        let five: Vec<&str> = items
            .iter()
            .filter(|i| i.thinking)
            .map(|i| i.name)
            .collect();
        assert_eq!(
            five,
            [
                "arithmetic",
                "a word problem",
                "a reasoning question in Russian",
                "a reasoning question in Uzbek (Latin)",
                "something it cannot know"
            ]
        );
        // Five in Russian and five in Uzbek were added, the Uzbek in both
        // alphabets.
        assert_eq!(
            items.iter().filter(|i| i.name.contains("Russian")).count(),
            6
        );
        assert_eq!(items.iter().filter(|i| i.name.contains("Uzbek")).count(), 5);
        assert_eq!(
            items.iter().filter(|i| i.name.contains("Cyrillic")).count(),
            2
        );
    }

    #[test]
    fn the_results_file_is_named_after_the_model_and_the_day_in_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            out_path(Some(dir.path()), "qwen3.5-4b-q4_k_m", "2026-10-08"),
            dir.path()
                .join("localSpace-measure-qwen3.5-4b-q4_k_m-2026-10-08.md")
        );
        assert_eq!(
            out_path(Some(Path::new("results.md")), "m", "2026-10-08"),
            PathBuf::from("results.md")
        );
        // A folder that is not there yet is one all the same, and is made.
        let fresh = dir.path().join("results");
        assert_eq!(
            out_path(Some(&fresh), "m", "2026-10-08"),
            fresh.join("localSpace-measure-m-2026-10-08.md")
        );
        assert!(fresh.is_dir());
        assert_eq!(
            out_path(None, "m", "2026-10-08"),
            PathBuf::from("localSpace-measure-m-2026-10-08.md")
        );
    }

    #[test]
    fn the_cells_say_what_the_measure_holds_and_never_break_a_table() {
        assert_eq!(cell("a | b\nc"), "a \\| b ⏎ c");
        assert_eq!(thinking_cell(None), "not measured");
        let mut measure = AnswerMeasure {
            thinking_asked: Some(false),
            ..Default::default()
        };
        assert_eq!(thinking_cell(Some(&measure)), "off: none");
        measure.thinking_asked = Some(true);
        measure.thinking_words = 120;
        measure.thinking_began = Some(Duration::from_millis(400));
        measure.thinking_ended = Some(Duration::from_millis(8100));
        assert_eq!(
            thinking_cell(Some(&measure)),
            "on: 120 words, from 0.4 s to 8.1 s"
        );
        measure.thinking_asked = None;
        assert!(thinking_cell(Some(&measure)).starts_with("the model's default: 120 words"));
        assert_eq!(figures_cell(Some(&measure)), "no figures");
        measure.timings = Some(localspace_core::model::Timings {
            prompt_n: 23,
            prompt_ms: 1200.0,
            predicted_n: 85,
            predicted_ms: 5950.0,
        });
        assert_eq!(
            figures_cell(Some(&measure)),
            "read 23 in 1.2 s; wrote 85 at 14.3 a second"
        );
    }

    /// A run that ends early (a limit, an engine that did not come up)
    /// still writes what it has, and says why it ended.
    #[test]
    fn a_run_that_ended_early_reports_what_it_has_and_why() {
        let progress = Progress {
            id: "m".into(),
            day: "2026-10-08".into(),
            machine: "a laptop".into(),
            facts: Vec::new(),
            engine: "version: test".into(),
            data_dir: PathBuf::from("run"),
            entry: None,
            files: Vec::new(),
            ready_after: None,
            passes: vec![Pass {
                title: "Thinking off, the whole script".into(),
                thinking: Some(false),
                items: vec![(
                    "a greeting",
                    vec![Answered {
                        text: "Hi there!".into(),
                        seconds: 1.5,
                        reply: "Hello!".into(),
                        tools: Vec::new(),
                        error: None,
                        measure: None,
                    }],
                )],
                carried: None,
            }],
            pictures: Vec::new(),
            read_time: None,
            ended: Some("the 8-minute limit for the whole run ran out during an answer".into()),
        };
        let text = report(&progress);
        assert!(text.contains(
            "**The run ended early: the 8-minute limit for the whole run ran out during an answer.**"
        ));
        assert!(text.contains("| a greeting: Hi there! | 1.5 s |"), "{text}");
        assert!(text.contains("The model was not reached."));
        assert!(text.contains("Not reached."));
        assert_eq!(answered_count(&progress.passes), (1, 1));
    }
    /// The picture is a PNG the kit carries: a model is asked about the
    /// same picture everywhere.
    #[test]
    fn the_picture_is_a_png() {
        assert_eq!(
            &PICTURE[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        assert!(PICTURE.len() < 4096);
    }
}
