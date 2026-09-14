//! Roles and tool exposure (deployment §4.3, §6.2; Pilot 1, Phase A): a
//! read-only account is shown only the tools that read and is refused the
//! others before any harness runs, whoever asks; a member who may not change
//! a board is shown none of its writing tools; and the same people, given
//! more, see more.

use localspace_core::{Caller, Config, Core, READ_ONLY_REASON};
use localspace_proto as proto;
use serde_json::json;
use std::path::PathBuf;

const WHITEBOARD: &str = "io.localspace.whiteboard";

fn repo_dir(name: &str, marker: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    dir.join(marker).exists().then_some(dir)
}

fn config() -> Option<Config> {
    let mut cfg = Config::organisation("operator");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    cfg.data_dir = None;
    Some(cfg)
}

fn with_role(name: &str, role: proto::UserRole) -> Caller {
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

fn add_member(
    core: &mut Core,
    who: &Caller,
    workspace: &str,
    user: &str,
    level: proto::AccessLevel,
) {
    match core.handle_as(
        who,
        proto::Request::SetMember {
            workspace: workspace.into(),
            principal: proto::Principal::User(user.into()),
            level,
        },
    ) {
        proto::Response::Workspaces(_) => {}
        other => panic!("SetMember failed: {other:?}"),
    }
}

#[test]
fn a_read_only_account_cannot_write_a_board_through_a_surface_even_its_own() {
    let Some(cfg) = config() else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let mut core = Core::new(cfg).expect("creating Core");
    let carla = with_role("carla", proto::UserRole::Viewer);
    // Her own personal workspace: she owns its documents, and still may not
    // change them, because her account is read-only wherever it is.
    let doc = match core.handle_as(
        &carla,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { doc, .. } => doc,
        other => panic!("GetDocJson failed: {other:?}"),
    };
    match core.handle_as(
        &carla,
        proto::Request::WriteDoc {
            harness: WHITEBOARD.into(),
            view: "web".into(),
            doc: proto::Json(json!({"shapes": []})),
            commit: true,
        },
    ) {
        proto::Response::Error { message } => assert_eq!(message, READ_ONLY_REASON),
        other => panic!("a viewer's write went through: {other:?}"),
    }
    match core.handle_as(
        &carla,
        proto::Request::DocSync {
            doc,
            peer: "w1".into(),
            message: vec![0],
        },
    ) {
        proto::Response::Error { message } => assert_eq!(message, READ_ONLY_REASON),
        other => panic!("a viewer's sync went through: {other:?}"),
    }
}

fn error_of(response: proto::Response) -> String {
    match response {
        proto::Response::Error { message } => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn only_an_administrator_touches_what_is_shared() {
    let Some(cfg) = config() else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let mut core = Core::new(cfg).expect("creating Core");
    let root = with_role("root", proto::UserRole::Admin);
    let mark = with_role("mark", proto::UserRole::Member);
    let shared: Vec<proto::Request> = vec![
        proto::Request::SetNetworkMode {
            mode: proto::NetworkMode::Airgapped,
        },
        proto::Request::SetHarnessEnabled {
            harness: WHITEBOARD.into(),
            enabled: false,
        },
        proto::Request::InstallHarness {
            path: "harnesses/whiteboard".into(),
        },
        proto::Request::ApproveInstall {
            harness: WHITEBOARD.into(),
            token: "t".into(),
        },
        proto::Request::UninstallHarness {
            harness: WHITEBOARD.into(),
        },
        proto::Request::DownloadModel { id: "tiny".into() },
        proto::Request::LoadModel { id: "tiny".into() },
        proto::Request::UnloadModel,
        proto::Request::ImportModel {
            path: "/nowhere/at/all".into(),
        },
        proto::Request::SelectModel {
            id: "http://localhost:9/v1|x".into(),
        },
        proto::Request::EngineLog { lines: 5 },
        proto::Request::PreviewContext { budget: 0 },
        proto::Request::RunEvals {
            harness: WHITEBOARD.into(),
        },
    ];
    for request in shared {
        let name = format!("{request:?}");
        let why = error_of(core.handle_as(&mark, request));
        assert_eq!(why, "Only an administrator can do that.", "{name}");
    }
    // The whiteboard is still there for everyone.
    match core.handle_as(&mark, proto::Request::GetEnvironment) {
        proto::Response::Environment(env) => {
            assert!(env.harnesses.iter().any(|h| h.id == WHITEBOARD), "{env:?}");
        }
        other => panic!("GetEnvironment failed: {other:?}"),
    }
    // The administrator is answered.
    assert!(matches!(
        core.handle_as(&root, proto::Request::EngineLog { lines: 5 }),
        proto::Response::EngineLog { .. }
    ));
}

#[test]
fn a_member_who_may_only_view_cannot_write_the_board_through_its_logic() {
    let Some(cfg) = config() else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let mut core = Core::new(cfg).expect("creating Core");
    let root = with_role("root", proto::UserRole::Admin);
    let anna = with_role("anna", proto::UserRole::Member);
    let carla = with_role("carla", proto::UserRole::Member);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna", proto::AccessLevel::Edit);
    add_member(&mut core, &root, &team, "carla", proto::AccessLevel::View);
    select(&mut core, &anna, &team, None);
    select(&mut core, &carla, &team, None);
    call(
        &mut core,
        &anna,
        "canvas.set_title",
        json!({"title": "Team's board"}),
    );

    // The surface's message that carries a whole document: the logic would
    // save it. With `view`, it is refused before the logic is asked.
    let payload = br#"{"doc":{"title":"pwned","shapes":[],"frames":[],"selection":[]}}"#.to_vec();
    let why = error_of(core.handle_as(
        &carla,
        proto::Request::HarnessEvent {
            harness: WHITEBOARD.into(),
            view: "web".into(),
            payload: payload.clone(),
        },
    ));
    assert!(why.contains("edit"), "{why}");
    match core.handle_as(
        &carla,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { json, .. } => assert_eq!(json.0["title"], "Team's board"),
        other => panic!("GetDocJson failed: {other:?}"),
    }
    // Anna, with `edit`, may.
    assert!(matches!(
        core.handle_as(
            &anna,
            proto::Request::HarnessEvent {
                harness: WHITEBOARD.into(),
                view: "web".into(),
                payload,
            },
        ),
        proto::Response::Ok
    ));
}

#[test]
fn history_undo_and_dropped_runs_stay_inside_what_the_caller_may_edit() {
    let Some(cfg) = config() else {
        eprintln!("skipping: the whiteboard is not built");
        return;
    };
    let mut core = Core::new(cfg).expect("creating Core");
    let root = with_role("root", proto::UserRole::Admin);
    let anna = with_role("anna", proto::UserRole::Member);
    let ben = with_role("ben", proto::UserRole::Member);
    let vera = with_role("vera", proto::UserRole::Viewer);
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "anna", proto::AccessLevel::Edit);
    select(&mut core, &anna, &team, None);
    let team_doc = match core.handle_as(
        &anna,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { doc, .. } => doc,
        other => panic!("GetDocJson failed: {other:?}"),
    };
    assert!(matches!(
        call(
            &mut core,
            &anna,
            "canvas.add_sticky",
            json!({"text": "Anna's"})
        ),
        proto::ToolOutcome::Ok { .. }
    ));

    // Ben, in his own workspace, sees none of Team's history and undoes nothing there.
    match core.handle_as(&ben, proto::Request::GetHistory { limit: 50 }) {
        proto::Response::History { commits } => {
            assert!(commits.iter().all(|c| c.doc != team_doc), "{commits:?}");
        }
        other => panic!("GetHistory failed: {other:?}"),
    }
    assert_eq!(
        error_of(core.handle_as(&ben, proto::Request::Undo)),
        "nothing to undo"
    );
    assert_eq!(
        error_of(core.handle_as(&ben, proto::Request::Redo)),
        "nothing to redo"
    );
    // Anna sees her commit, and Team's board still holds her note.
    match core.handle_as(&anna, proto::Request::GetHistory { limit: 50 }) {
        proto::Response::History { commits } => {
            assert!(commits.iter().any(|c| c.doc == team_doc), "{commits:?}");
        }
        other => panic!("GetHistory failed: {other:?}"),
    }
    match core.handle_as(
        &anna,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { json, .. } => {
            assert_eq!(json.0["shapes"].as_array().map(|a| a.len()), Some(1));
        }
        other => panic!("GetDocJson failed: {other:?}"),
    }
    // Anna's own undo reaches it.
    assert!(!matches!(
        core.handle_as(&anna, proto::Request::Undo),
        proto::Response::Error { .. }
    ));
    // A read-only account undoes and drops nothing.
    assert_eq!(
        error_of(core.handle_as(&vera, proto::Request::Undo)),
        "nothing to undo"
    );
    assert_eq!(
        error_of(core.handle_as(
            &vera,
            proto::Request::DropRun {
                run: "run_1".into()
            }
        )),
        READ_ONLY_REASON
    );
}

fn select(core: &mut Core, who: &Caller, workspace: &str, reason: Option<&str>) {
    match core.handle_as(
        who,
        proto::Request::SelectWorkspace {
            workspace: workspace.into(),
            reason: reason.map(str::to_string),
        },
    ) {
        proto::Response::Environment(_) => {}
        other => panic!("SelectWorkspace failed: {other:?}"),
    }
}

fn tools(core: &mut Core, who: &Caller) -> Vec<String> {
    match core.handle_as(who, proto::Request::GetActiveSet) {
        proto::Response::Active(set) => set.tools.into_iter().map(|t| t.name).collect(),
        other => panic!("GetActiveSet failed: {other:?}"),
    }
}

fn call(
    core: &mut Core,
    who: &Caller,
    tool: &str,
    params: serde_json::Value,
) -> proto::ToolOutcome {
    match core.handle_as(
        who,
        proto::Request::CallTool {
            tool: tool.into(),
            params: proto::Json(params),
        },
    ) {
        proto::Response::ToolResult(outcome) => outcome,
        other => panic!("CallTool {tool} failed: {other:?}"),
    }
}

fn shapes(core: &mut Core, who: &Caller) -> usize {
    match core.handle_as(
        who,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { json, .. } => {
            json.0["shapes"].as_array().map(|a| a.len()).unwrap_or(0)
        }
        other => panic!("GetDocJson failed: {other:?}"),
    }
}

fn has(list: &[String], name: &str) -> bool {
    list.iter().any(|t| t == name)
}

#[test]
fn a_read_only_account_is_shown_and_allowed_only_the_tools_that_read() {
    let Some(cfg) = config() else { return };
    let mut core = Core::new(cfg).unwrap();
    let root = with_role("root", proto::UserRole::Admin);
    let vera = with_role("vera", proto::UserRole::Viewer);
    let max = with_role("max", proto::UserRole::Member);

    // Both are `edit` members of the same workspace: the account's role is
    // what tells them apart.
    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "vera", proto::AccessLevel::Edit);
    add_member(&mut core, &root, &team, "max", proto::AccessLevel::Edit);
    select(&mut core, &vera, &team, None);
    select(&mut core, &max, &team, None);

    // Shown: only what reads, Core's own tools included.
    let veras = tools(&mut core, &vera);
    assert!(has(&veras, "canvas.list"), "{veras:?}");
    assert!(has(&veras, "canvas.zoom"), "{veras:?}");
    assert!(has(&veras, "find_capability"), "{veras:?}");
    for hidden in [
        "canvas.add_sticky",
        "canvas.delete",
        "task.plan",
        "task.note",
    ] {
        assert!(
            !has(&veras, hidden),
            "{hidden} shown to a read-only account: {veras:?}"
        );
    }
    let maxs = tools(&mut core, &max);
    assert!(has(&maxs, "canvas.add_sticky"), "{maxs:?}");
    assert!(has(&maxs, "task.plan"), "{maxs:?}");

    // Allowed: the same. A write is refused before the harness runs, with
    // one plain sentence, and a read goes through.
    match call(
        &mut core,
        &vera,
        "canvas.add_sticky",
        json!({"text": "from Vera", "fill": "yellow"}),
    ) {
        proto::ToolOutcome::Denied { reason } => assert_eq!(reason, READ_ONLY_REASON),
        other => panic!("a read-only account wrote: {other:?}"),
    }
    match call(&mut core, &vera, "task.note", json!({"text": "a note"})) {
        proto::ToolOutcome::Denied { reason } => assert_eq!(reason, READ_ONLY_REASON),
        other => panic!("a read-only account wrote the ledger: {other:?}"),
    }
    assert!(matches!(
        call(&mut core, &vera, "canvas.list", json!({})),
        proto::ToolOutcome::Ok { .. }
    ));
    assert!(matches!(
        call(
            &mut core,
            &max,
            "canvas.add_sticky",
            json!({"text": "from Max", "fill": "yellow"})
        ),
        proto::ToolOutcome::Ok { .. }
    ));
    assert_eq!(shapes(&mut core, &vera), 1, "she reads what he wrote");
    assert_eq!(shapes(&mut core, &max), 1);

    // The refusals are in the audit, by her, with the reason.
    let records = core.audit_log().records();
    let refused: Vec<&str> = records
        .iter()
        .filter(|r| r.actor.user == "vera" && r.event == "tool.call" && r.result == "denied")
        .map(|r| r.detail["tool"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        refused,
        vec!["canvas.add_sticky", "task.note"],
        "{refused:?}"
    );
    assert!(
        records
            .iter()
            .filter(|r| r.actor.user == "vera" && r.result == "denied")
            .all(|r| r.detail["why"] == "read-only account")
    );
}

#[test]
fn a_member_who_may_not_change_a_board_is_shown_none_of_its_writing_tools() {
    let Some(cfg) = config() else { return };
    let mut core = Core::new(cfg).unwrap();
    let root = with_role("root", proto::UserRole::Admin);
    let carla = with_role("carla", proto::UserRole::Member);
    let dan = with_role("dan", proto::UserRole::Member);

    let team = create(&mut core, &root, "Team");
    add_member(&mut core, &root, &team, "carla", proto::AccessLevel::View);
    add_member(&mut core, &root, &team, "dan", proto::AccessLevel::Edit);
    select(&mut core, &carla, &team, None);
    select(&mut core, &dan, &team, None);

    // Before the board exists, the workspace's level decides.
    let carlas = tools(&mut core, &carla);
    assert!(has(&carlas, "canvas.list"), "{carlas:?}");
    assert!(!has(&carlas, "canvas.add_sticky"), "{carlas:?}");
    assert!(
        has(&carlas, "task.plan"),
        "her account is not read-only: {carlas:?}"
    );
    let dans = tools(&mut core, &dan);
    assert!(has(&dans, "canvas.add_sticky"), "{dans:?}");

    // Once it exists, the document's own answer decides, and it agrees.
    assert!(matches!(
        call(
            &mut core,
            &dan,
            "canvas.add_sticky",
            json!({"text": "from Dan", "fill": "yellow"})
        ),
        proto::ToolOutcome::Ok { .. }
    ));
    assert_eq!(shapes(&mut core, &carla), 1);
    let carlas = tools(&mut core, &carla);
    assert!(!has(&carlas, "canvas.add_sticky"), "{carlas:?}");
    match call(
        &mut core,
        &carla,
        "canvas.add_sticky",
        json!({"text": "from Carla", "fill": "yellow"}),
    ) {
        proto::ToolOutcome::Denied { reason } => assert!(reason.contains("edit"), "{reason}"),
        other => panic!("a viewer of the board wrote to it: {other:?}"),
    }

    // Given `edit`, she sees the writing tools on her next request.
    add_member(&mut core, &root, &team, "carla", proto::AccessLevel::Edit);
    let carlas = tools(&mut core, &carla);
    assert!(has(&carlas, "canvas.add_sticky"), "{carlas:?}");

    // In her own workspace she always could.
    select(&mut core, &carla, "ws_carla", None);
    let carlas = tools(&mut core, &carla);
    assert!(has(&carlas, "canvas.add_sticky"), "{carlas:?}");

    // An administrator inside by break-glass has an owner's tools there.
    let boss = with_role("boss", proto::UserRole::Admin);
    let before = tools(&mut core, &boss);
    assert!(
        has(&before, "canvas.add_sticky"),
        "their own workspace: {before:?}"
    );
    select(
        &mut core,
        &boss,
        &team,
        Some("incident 7: checking the board"),
    );
    let inside = tools(&mut core, &boss);
    assert!(has(&inside, "canvas.add_sticky"), "{inside:?}");
}
