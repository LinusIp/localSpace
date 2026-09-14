//! Who is on a board (Pilot 1; the app screens' board): the people on it
//! see one another, the list follows arrivals and departures, a window that
//! falls silent is forgotten, another workspace's board is another place,
//! and a board one may not read cannot be announced.

use localspace_core::{Caller, Config, Core, To};
use localspace_proto as proto;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const WHITEBOARD: &str = "io.localspace.whiteboard";

type Events = Arc<Mutex<Vec<(To, proto::Event)>>>;

fn repo_dir(name: &str, marker: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    dir.join(marker).exists().then_some(dir)
}

/// An organisation Core with the whiteboard, its events kept with whom they were for.
fn core(presence_ttl_ms: u64) -> Option<(Core, Events)> {
    let mut cfg = Config::organisation("operator");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    cfg.data_dir = None;
    cfg.presence_ttl_ms = presence_ttl_ms;
    let mut core = Core::new(cfg).expect("creating Core");
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    core.set_event_sink(Box::new(move |to, ev| sink.lock().unwrap().push((to, ev))));
    Some((core, events))
}

fn caller(name: &str, role: proto::UserRole) -> Caller {
    Caller {
        user: name.into(),
        session: format!("s_{name}"),
        ip: "10.0.0.9".into(),
        roles: vec![role],
        groups: Vec::new(),
    }
}

fn create(core: &mut Core, who: &Caller, name: &str) -> String {
    match core.handle_as(who, proto::Request::CreateWorkspace { name: name.into() }) {
        proto::Response::Workspaces(list) => {
            list.into_iter()
                .find(|w| w.name == name)
                .expect("the new workspace is listed")
                .id
        }
        other => panic!("CreateWorkspace failed: {other:?}"),
    }
}

fn add_member(core: &mut Core, who: &Caller, workspace: &str, user: &str) {
    match core.handle_as(
        who,
        proto::Request::SetMember {
            workspace: workspace.into(),
            principal: proto::Principal::User(user.into()),
            level: proto::AccessLevel::Edit,
        },
    ) {
        proto::Response::Workspaces(_) => {}
        other => panic!("SetMember failed: {other:?}"),
    }
}

fn select(core: &mut Core, who: &Caller, workspace: &str) {
    match core.handle_as(
        who,
        proto::Request::SelectWorkspace {
            workspace: workspace.into(),
            reason: None,
        },
    ) {
        proto::Response::Environment(_) => {}
        other => panic!("SelectWorkspace failed: {other:?}"),
    }
}

fn announce(core: &mut Core, who: &Caller, peer: &str, board: Option<&str>) -> proto::Response {
    core.handle_as(
        who,
        proto::Request::Presence {
            board: board.map(str::to_string),
            peer: peer.into(),
        },
    )
}

/// The people in the last presence event sent to `user`, by name.
fn last_list(events: &Events, user: &str) -> Option<Vec<String>> {
    events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find_map(|(to, ev)| match (to, ev) {
            (To::User(u), proto::Event::Presence { people, .. }) if u == user => {
                Some(people.iter().map(|p| p.name.clone()).collect())
            }
            _ => None,
        })
}

fn presence_events_for(events: &Events, user: &str) -> usize {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|(to, ev)| matches!((to, ev), (To::User(u), proto::Event::Presence { .. }) if u == user))
        .count()
}

#[test]
fn the_people_on_a_board_see_one_another_arrive_and_leave() {
    let Some((mut core, events)) = core(45_000) else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let root = caller("root", proto::UserRole::Admin);
    let anna = caller("anna", proto::UserRole::Member);
    let bek = caller("bek", proto::UserRole::Member);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna");
    add_member(&mut core, &root, &team, "bek");
    select(&mut core, &anna, &team);
    select(&mut core, &bek, &team);

    assert!(matches!(
        announce(&mut core, &anna, "a1", Some(WHITEBOARD)),
        proto::Response::Ok
    ));
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&["anna".to_string()][..])
    );
    assert_eq!(
        last_list(&events, "bek"),
        None,
        "nobody tells Bek before he is there"
    );

    assert!(matches!(
        announce(&mut core, &bek, "b1", Some(WHITEBOARD)),
        proto::Response::Ok
    ));
    let both = ["anna".to_string(), "bek".to_string()];
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&both[..]),
        "Anna hears Bek arrive"
    );
    assert_eq!(
        last_list(&events, "bek").as_deref(),
        Some(&both[..]),
        "Bek hears who is there"
    );

    // Anna's second window changes nothing anyone sees but is not an error.
    let before = presence_events_for(&events, "bek");
    assert!(matches!(
        announce(&mut core, &anna, "a2", Some(WHITEBOARD)),
        proto::Response::Ok
    ));
    assert_eq!(last_list(&events, "bek").as_deref(), Some(&both[..]));
    assert!(presence_events_for(&events, "bek") >= before);

    // Bek leaves: Anna hears it, Bek hears nothing more.
    let bek_heard = presence_events_for(&events, "bek");
    assert!(matches!(
        announce(&mut core, &bek, "b1", None),
        proto::Response::Ok
    ));
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&["anna".to_string()][..])
    );
    assert_eq!(
        presence_events_for(&events, "bek"),
        bek_heard,
        "a person who left is not told about a board they are not on"
    );
}

#[test]
fn a_window_that_falls_silent_is_forgotten() {
    let Some((mut core, events)) = core(60) else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let root = caller("root", proto::UserRole::Admin);
    let anna = caller("anna", proto::UserRole::Member);
    let bek = caller("bek", proto::UserRole::Member);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna");
    add_member(&mut core, &root, &team, "bek");
    select(&mut core, &anna, &team);
    select(&mut core, &bek, &team);
    announce(&mut core, &anna, "a1", Some(WHITEBOARD));
    announce(&mut core, &bek, "b1", Some(WHITEBOARD));
    assert_eq!(last_list(&events, "anna").map(|l| l.len()), Some(2));

    // Bek's laptop closes; Anna's window keeps announcing.
    std::thread::sleep(std::time::Duration::from_millis(120));
    announce(&mut core, &anna, "a1", Some(WHITEBOARD));
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&["anna".to_string()][..])
    );
}

#[test]
fn a_window_is_its_person_s_and_nobody_else_leaves_with_its_name() {
    let Some((mut core, events)) = core(45_000) else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let root = caller("root", proto::UserRole::Admin);
    let anna = caller("anna", proto::UserRole::Member);
    let bek = caller("bek", proto::UserRole::Member);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna");
    add_member(&mut core, &root, &team, "bek");
    select(&mut core, &anna, &team);
    select(&mut core, &bek, &team);
    announce(&mut core, &anna, "w1", Some(WHITEBOARD));
    // Bek says a window named like Anna's has left: his own, not hers.
    announce(&mut core, &bek, "w1", None);
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&["anna".to_string()][..])
    );
    announce(&mut core, &bek, "w1", Some(WHITEBOARD));
    assert_eq!(
        last_list(&events, "anna").map(|l| l.len()),
        Some(2),
        "two people, two windows of the same name"
    );
}

#[test]
fn another_workspace_s_board_is_another_place_and_an_outsider_cannot_announce_it() {
    let Some((mut core, events)) = core(45_000) else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let root = caller("root", proto::UserRole::Admin);
    let anna = caller("anna", proto::UserRole::Member);
    let bek = caller("bek", proto::UserRole::Member);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna");
    select(&mut core, &anna, &team);
    // Bek stays in his personal workspace: the same harness, another board.
    announce(&mut core, &anna, "a1", Some(WHITEBOARD));
    announce(&mut core, &bek, "b1", Some(WHITEBOARD));
    assert_eq!(
        last_list(&events, "anna").as_deref(),
        Some(&["anna".to_string()][..])
    );
    assert_eq!(
        last_list(&events, "bek").as_deref(),
        Some(&["bek".to_string()][..])
    );

    // A board nobody may read cannot be announced; a bad window name neither.
    match announce(&mut core, &bek, "b1", Some("io.localspace.nothing")) {
        proto::Response::Error { message } => assert!(message.contains("no harness"), "{message}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    match announce(&mut core, &bek, "not a name!", Some(WHITEBOARD)) {
        proto::Response::Error { message } => {
            assert!(message.contains("letters or digits"), "{message}")
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}
