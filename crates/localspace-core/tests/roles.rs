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
