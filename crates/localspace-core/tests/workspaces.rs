//! Workspaces (deployment §5–6; Pilot 1, Phase A): a shared workspace is one
//! board for its members and none for anyone else; a document can be
//! tightened below its workspace and never loosened beyond it; an
//! administrator who is not a member goes in with a reason the audit keeps;
//! and all of it — members, levels, the workspace a user was in — survives a
//! restart.

use localspace_core::{Caller, Config, Core};
use localspace_proto as proto;
use serde_json::json;
use std::path::{Path, PathBuf};

const WHITEBOARD: &str = "io.localspace.whiteboard";

fn repo_dir(name: &str, marker: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name);
    dir.join(marker).exists().then_some(dir)
}

fn config(data: Option<&Path>) -> Option<Config> {
    let mut cfg = Config::organisation("operator");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    cfg.data_dir = data.map(Path::to_path_buf);
    Some(cfg)
}

fn member(name: &str) -> Caller {
    Caller {
        user: name.into(),
        session: format!("s_{name}"),
        ip: "10.0.0.7".into(),
        roles: vec![proto::UserRole::Member],
        groups: Vec::new(),
    }
}

fn admin(name: &str) -> Caller {
    Caller {
        roles: vec![proto::UserRole::Admin],
        ..member(name)
    }
}

fn workspaces(core: &mut Core, who: &Caller) -> Vec<proto::WorkspaceInfo> {
    match core.handle_as(who, proto::Request::ListWorkspaces) {
        proto::Response::Workspaces(list) => list,
        other => panic!("ListWorkspaces failed: {other:?}"),
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

fn set_member(
    core: &mut Core,
    who: &Caller,
    workspace: &str,
    user: &str,
    level: proto::AccessLevel,
) -> proto::Response {
    core.handle_as(
        who,
        proto::Request::SetMember {
            workspace: workspace.into(),
            principal: proto::Principal::User(user.into()),
            level,
        },
    )
}

fn select(core: &mut Core, who: &Caller, workspace: &str, reason: Option<&str>) -> proto::Response {
    core.handle_as(
        who,
        proto::Request::SelectWorkspace {
            workspace: workspace.into(),
            reason: reason.map(str::to_string),
        },
    )
}

fn environment(core: &mut Core, who: &Caller) -> proto::EnvironmentState {
    match core.handle_as(who, proto::Request::GetEnvironment) {
        proto::Response::Environment(state) => state,
        other => panic!("GetEnvironment failed: {other:?}"),
    }
}

fn board(core: &mut Core, who: &Caller) -> Result<(String, usize), String> {
    match core.handle_as(
        who,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { doc, json, .. } => Ok((
            doc,
            json.0["shapes"].as_array().map(|a| a.len()).unwrap_or(0),
        )),
        proto::Response::Error { message } => Err(message),
        other => panic!("GetDocJson failed: {other:?}"),
    }
}

fn add_sticky(core: &mut Core, who: &Caller, text: &str) -> proto::ToolOutcome {
    match core.handle_as(
        who,
        proto::Request::CallTool {
            tool: "canvas.add_sticky".into(),
            params: proto::Json(json!({"text": text, "fill": "yellow"})),
        },
    ) {
        proto::Response::ToolResult(outcome) => outcome,
        other => panic!("add_sticky failed: {other:?}"),
    }
}

#[test]
fn a_shared_workspace_is_one_board_for_its_members_and_none_for_others() {
    let Some(cfg) = config(None) else { return };
    let mut core = Core::new(cfg).unwrap();
    let root = admin("root");
    let anna = member("anna");
    let ben = member("ben");
    let carla = member("carla");
    let dave = member("dave");

    // A member may not make one; an administrator owns the one they make.
    assert!(matches!(
        core.handle_as(
            &anna,
            proto::Request::CreateWorkspace {
                name: "Nope".into()
            }
        ),
        proto::Response::Error { .. }
    ));
    let team = create(&mut core, &root, "Team");
    let mine = workspaces(&mut core, &root);
    let team_info = mine.iter().find(|w| w.id == team).unwrap();
    assert_eq!(team_info.mine, Some(proto::AccessLevel::Owner));
    assert_eq!(
        team_info.agent_writes,
        proto::AgentWrites::Proposal,
        "agents propose in shared workspaces"
    );
    assert!(
        workspaces(&mut core, &anna).iter().all(|w| w.id != team),
        "not hers yet"
    );

    // Members with levels; a member cannot add members, an owner can.
    assert!(matches!(
        set_member(&mut core, &anna, &team, "ben", proto::AccessLevel::Edit),
        proto::Response::Error { .. }
    ));
    assert!(matches!(
        set_member(&mut core, &root, &team, "anna", proto::AccessLevel::Owner),
        proto::Response::Workspaces(_)
    ));
    assert!(
        matches!(
            set_member(&mut core, &anna, &team, "ben", proto::AccessLevel::Edit),
            proto::Response::Workspaces(_)
        ),
        "Anna owns it now"
    );
    assert!(matches!(
        set_member(&mut core, &anna, &team, "carla", proto::AccessLevel::View),
        proto::Response::Workspaces(_)
    ));
    let listed = workspaces(&mut core, &ben)
        .into_iter()
        .find(|w| w.id == team)
        .expect("Ben sees Team");
    assert_eq!(listed.mine, Some(proto::AccessLevel::Edit));
    assert_eq!(
        listed.members.len(),
        4,
        "root, anna, ben, carla: {:?}",
        listed.members
    );

    // One board for all its members: what Anna puts on it, Ben sees.
    match select(&mut core, &anna, &team, None) {
        proto::Response::Environment(env) => {
            assert_eq!(env.workspace, "Team");
            assert_eq!(env.workspace_id, team);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        select(&mut core, &ben, &team, None),
        proto::Response::Environment(_)
    ));
    let (annas_doc, _) = board(&mut core, &anna).unwrap();
    let (bens_doc, before) = board(&mut core, &ben).unwrap();
    assert_eq!(annas_doc, bens_doc, "the workspace's one board");
    assert_eq!(before, 0);
    assert!(matches!(
        add_sticky(&mut core, &anna, "from Anna"),
        proto::ToolOutcome::Ok { .. }
    ));
    assert_eq!(
        board(&mut core, &ben).unwrap().1,
        1,
        "Ben sees Anna's sticky"
    );
    assert!(
        workspaces(&mut core, &ben)
            .iter()
            .find(|w| w.id == team)
            .unwrap()
            .current
    );

    // A viewer reads and may not write; a stranger may not even go in.
    assert!(matches!(
        select(&mut core, &carla, &team, None),
        proto::Response::Environment(_)
    ));
    assert_eq!(board(&mut core, &carla).unwrap().1, 1);
    match add_sticky(&mut core, &carla, "from Carla") {
        proto::ToolOutcome::Denied { reason } => assert!(reason.contains("edit"), "{reason}"),
        other => panic!("a viewer wrote: {other:?}"),
    }
    match select(&mut core, &dave, &team, None) {
        proto::Response::Error { message } => {
            assert!(message.contains("not a member"), "{message}")
        }
        other => panic!("a stranger went in: {other:?}"),
    }
    assert_ne!(
        board(&mut core, &dave).unwrap().0,
        annas_doc,
        "Dave is still on his own board"
    );

    // Break-glass: an administrator who is not a member needs a reason,
    // and the audit keeps it.
    let boss = admin("boss");
    match select(&mut core, &boss, &team, None) {
        proto::Response::Error { message } => assert!(message.contains("reason"), "{message}"),
        other => panic!("an admin went in without a reason: {other:?}"),
    }
    assert!(matches!(
        select(
            &mut core,
            &boss,
            &team,
            Some("incident 42: reviewing the board")
        ),
        proto::Response::Environment(_)
    ));
    assert_eq!(board(&mut core, &boss).unwrap().0, annas_doc);
    let glass = core
        .audit_log()
        .records()
        .into_iter()
        .rev()
        .find(|r| r.event == "workspace.break_glass")
        .expect("break-glass is audited");
    assert_eq!(glass.actor.user, "boss");
    assert_eq!(glass.scope.workspace, team);
    assert_eq!(glass.detail["reason"], "incident 42: reviewing the board");

    // Removing a member ends their way in.
    assert!(matches!(
        core.handle_as(
            &anna,
            proto::Request::RemoveMember {
                workspace: team.clone(),
                principal: proto::Principal::User("ben".into())
            }
        ),
        proto::Response::Workspaces(_)
    ));
    match board(&mut core, &ben) {
        Err(message) => assert!(message.contains("needs"), "{message}"),
        Ok(_) => panic!("Ben still reads the board"),
    }
}

#[test]
fn a_document_can_be_tightened_but_never_loosened_beyond_its_workspace() {
    let Some(cfg) = config(None) else { return };
    let mut core = Core::new(cfg).unwrap();
    let root = admin("root");
    let anna = member("anna");
    let ben = member("ben");
    let carla = member("carla");
    let team = create(&mut core, &root, "Team");
    for (who, level) in [
        ("anna", proto::AccessLevel::Edit),
        ("ben", proto::AccessLevel::Edit),
        ("carla", proto::AccessLevel::View),
    ] {
        assert!(matches!(
            set_member(&mut core, &root, &team, who, level),
            proto::Response::Workspaces(_)
        ));
    }
    for who in [&anna, &ben, &carla] {
        assert!(matches!(
            select(&mut core, who, &team, None),
            proto::Response::Environment(_)
        ));
    }
    let (doc, _) = board(&mut core, &anna).unwrap();
    assert!(matches!(
        add_sticky(&mut core, &ben, "before"),
        proto::ToolOutcome::Ok { .. }
    ));

    // Anna, with edit, may not change who opens the document; root may.
    let tighten =
        |core: &mut Core, who: &Caller, members: Option<Vec<(&str, proto::AccessLevel)>>| {
            core.handle_as(
                who,
                proto::Request::SetDocumentAccess {
                    doc: doc.clone(),
                    members: members.map(|m| {
                        m.into_iter()
                            .map(|(u, level)| proto::Member {
                                principal: proto::Principal::User(u.into()),
                                level,
                            })
                            .collect()
                    }),
                },
            )
        };
    assert!(matches!(
        tighten(
            &mut core,
            &anna,
            Some(vec![("anna", proto::AccessLevel::Edit)])
        ),
        proto::Response::Error { .. }
    ));

    // Tightened to Anna alone: Ben may not write any more, nor read.
    assert!(matches!(
        tighten(
            &mut core,
            &root,
            Some(vec![
                ("root", proto::AccessLevel::Owner),
                ("anna", proto::AccessLevel::Edit)
            ])
        ),
        proto::Response::Ok
    ));
    assert!(matches!(
        add_sticky(&mut core, &ben, "after"),
        proto::ToolOutcome::Denied { .. }
    ));
    assert!(board(&mut core, &ben).is_err());
    assert_eq!(board(&mut core, &anna).unwrap().1, 1);

    // Never loosened: Carla holds view in the workspace and cannot be given edit here.
    match tighten(
        &mut core,
        &root,
        Some(vec![
            ("root", proto::AccessLevel::Owner),
            ("carla", proto::AccessLevel::Edit),
        ]),
    ) {
        proto::Response::Error { message } => {
            assert!(message.contains("cannot give more"), "{message}")
        }
        other => panic!("the document was loosened: {other:?}"),
    }
    // Nor given to someone the workspace does not have at all.
    match tighten(
        &mut core,
        &root,
        Some(vec![("dave", proto::AccessLevel::View)]),
    ) {
        proto::Response::Error { message } => assert!(message.contains("does not"), "{message}"),
        other => panic!("{other:?}"),
    }

    // Cleared: the workspace's members hold what they hold again.
    assert!(matches!(
        tighten(&mut core, &root, None),
        proto::Response::Ok
    ));
    assert!(matches!(
        add_sticky(&mut core, &ben, "again"),
        proto::ToolOutcome::Ok { .. }
    ));
    let records = core.audit_log().records();
    let access: Vec<&str> = records
        .iter()
        .filter(|r| r.event == "document.access")
        .map(|r| r.result.as_str())
        .collect();
    assert_eq!(access, vec!["denied", "ok", "ok"], "{access:?}");
}

#[test]
fn workspaces_members_and_the_workspace_a_user_was_in_survive_a_restart() {
    let data = tempfile::tempdir().unwrap();
    let Some(cfg) = config(Some(data.path())) else {
        return;
    };
    let root = admin("root");
    let ben = member("ben");
    let (team, doc) = {
        let mut core = Core::new(cfg).unwrap();
        let team = create(&mut core, &root, "Team");
        assert!(matches!(
            set_member(&mut core, &root, &team, "ben", proto::AccessLevel::Edit),
            proto::Response::Workspaces(_)
        ));
        assert!(matches!(
            select(&mut core, &ben, &team, None),
            proto::Response::Environment(_)
        ));
        assert!(matches!(
            add_sticky(&mut core, &ben, "kept"),
            proto::ToolOutcome::Ok { .. }
        ));
        let (doc, _) = board(&mut core, &ben).unwrap();
        (team, doc)
    };
    let mut core = Core::new(config(Some(data.path())).unwrap()).unwrap();
    let listed = workspaces(&mut core, &ben);
    let found = listed.iter().find(|w| w.id == team).expect("Team survived");
    assert_eq!(found.name, "Team");
    assert_eq!(found.mine, Some(proto::AccessLevel::Edit));
    assert!(found.current, "Ben is back where he was");
    assert!(
        listed
            .iter()
            .any(|w| w.personal_to.as_deref() == Some("ben")),
        "and his personal workspace is there"
    );
    assert_eq!(environment(&mut core, &ben).workspace_id, team);
    let (again, shapes) = board(&mut core, &ben).unwrap();
    assert_eq!(again, doc);
    assert_eq!(shapes, 1);
}

#[test]
fn a_personal_workspace_takes_no_members() {
    let Some(cfg) = config(None) else { return };
    let mut core = Core::new(cfg).unwrap();
    let root = admin("root");
    let anna = member("anna");
    let personal = environment(&mut core, &anna).workspace_id;
    assert_eq!(personal, "ws_anna");
    match set_member(&mut core, &root, &personal, "ben", proto::AccessLevel::Edit) {
        proto::Response::Error { message } => assert!(message.contains("personal"), "{message}"),
        other => panic!("a personal workspace took a member: {other:?}"),
    }
    match set_member(&mut core, &anna, &personal, "ben", proto::AccessLevel::Edit) {
        proto::Response::Error { .. } => {}
        other => panic!("{other:?}"),
    }
}
