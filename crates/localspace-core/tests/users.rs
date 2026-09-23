//! One Core, many identities (Pilot 1, Phase A): every request is made as a
//! caller, whose transcript, conversations, ledger, focus and personal
//! workspace are their own over one registry, one database and one DAG; the
//! events of a request go to its user, a document's change to everyone.

use localspace_core::{Caller, Config, Core, To};
use localspace_proto as proto;
use serde_json::json;
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

/// A Core with the whiteboard, its events kept with whom they were for.
fn core() -> Option<(Core, Events)> {
    let mut cfg = Config::personal("anna");
    cfg.harness_dir = Some(repo_dir("harnesses", "whiteboard/logic.wasm")?);
    cfg.catalog_dirs = vec![repo_dir("registry", "types/types.toml")?];
    let mut core = Core::new(cfg).expect("creating Core");
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    core.set_event_sink(Box::new(move |to, ev| sink.lock().unwrap().push((to, ev))));
    Some((core, events))
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

fn transcript(core: &mut Core, who: &Caller) -> Vec<proto::ChatMessage> {
    match core.handle_as(who, proto::Request::GetTranscript) {
        proto::Response::Transcript { messages } => messages,
        other => panic!("GetTranscript failed: {other:?}"),
    }
}

fn environment(core: &mut Core, who: &Caller) -> proto::EnvironmentState {
    match core.handle_as(who, proto::Request::GetEnvironment) {
        proto::Response::Environment(state) => state,
        other => panic!("GetEnvironment failed: {other:?}"),
    }
}

fn board(core: &mut Core, who: &Caller) -> (String, usize) {
    match core.handle_as(
        who,
        proto::Request::GetDocJson {
            harness: WHITEBOARD.into(),
        },
    ) {
        proto::Response::DocJson { doc, json, .. } => (
            doc,
            json.0["shapes"].as_array().map(|a| a.len()).unwrap_or(0),
        ),
        other => panic!("GetDocJson failed: {other:?}"),
    }
}

fn add_sticky(core: &mut Core, who: &Caller, text: &str) {
    match core.handle_as(
        who,
        proto::Request::CallTool {
            tool: "canvas.add_sticky".into(),
            params: proto::Json(json!({"text": text, "fill": "yellow"})),
        },
    ) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { .. }) => {}
        other => panic!("add_sticky failed: {other:?}"),
    }
}

#[test]
fn each_user_has_their_own_conversation_ledger_focus_and_board_over_one_environment() {
    let Some((mut core, events)) = core() else {
        return;
    };
    let anna = member("anna");
    let ben = member("ben");

    // Anna talks; Ben's transcript and conversations are untouched.
    core.handle_as(
        &anna,
        proto::Request::SendMessage {
            text: "Anna's question".into(),
            conversation: None,
        },
    );
    assert_eq!(
        transcript(&mut core, &anna).len(),
        2,
        "the question and the honest reply"
    );
    assert!(
        transcript(&mut core, &ben).is_empty(),
        "Ben sees nothing of Anna's"
    );
    match core.handle_as(&ben, proto::Request::ListConversations) {
        proto::Response::Conversations { list, .. } => {
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].messages, 0);
        }
        other => panic!("{other:?}"),
    }
    match core.handle_as(&anna, proto::Request::ListConversations) {
        proto::Response::Conversations { list, .. } => {
            assert_eq!(list[0].title, "Anna's question");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        transcript(&mut core, &anna).len(),
        2,
        "and Anna's is still hers"
    );

    // The same environment for both, each with their own focus and workspace.
    let anna_env = environment(&mut core, &anna);
    let ben_env = environment(&mut core, &ben);
    assert_eq!(anna_env.user, "anna");
    assert_eq!(ben_env.user, "ben");
    assert_eq!(
        ben_env.focus.as_deref(),
        Some(WHITEBOARD),
        "a new user starts on the first harness"
    );
    assert_eq!(
        anna_env
            .harnesses
            .iter()
            .map(|h| h.id.clone())
            .collect::<Vec<_>>(),
        ben_env
            .harnesses
            .iter()
            .map(|h| h.id.clone())
            .collect::<Vec<_>>(),
        "one registry"
    );
    assert_ne!(anna_env.workspace, ben_env.workspace, "personal workspaces");
    core.handle_as(&anna, proto::Request::SetFocus { harness: None });
    assert_eq!(environment(&mut core, &anna).focus, None);
    assert_eq!(
        environment(&mut core, &ben).focus.as_deref(),
        Some(WHITEBOARD),
        "Anna's focus is not Ben's"
    );

    // Each personal workspace has its own board; a sticky on Anna's is not on Ben's.
    core.handle_as(
        &anna,
        proto::Request::SetFocus {
            harness: Some(WHITEBOARD.into()),
        },
    );
    add_sticky(&mut core, &anna, "on Anna's board");
    let (anna_doc, anna_shapes) = board(&mut core, &anna);
    let (ben_doc, ben_shapes) = board(&mut core, &ben);
    assert_ne!(anna_doc, ben_doc, "a document per workspace");
    assert_eq!(anna_shapes, 1);
    assert_eq!(ben_shapes, 0);

    // Ben may not sync into Anna's board: he holds nothing on it.
    match core.handle_as(
        &ben,
        proto::Request::DocSync {
            doc: anna_doc.clone(),
            peer: "ben-frame".into(),
            message: Vec::new(),
        },
    ) {
        proto::Response::Error { message } => assert!(message.contains("needs"), "{message}"),
        other => panic!("Ben's sync into Anna's board was not refused: {other:?}"),
    }

    // The ledger is per user: Anna's export is not in Ben's.
    match core.handle_as(
        &anna,
        proto::Request::CallTool {
            tool: "canvas.export_outline".into(),
            params: proto::Json(json!({})),
        },
    ) {
        proto::Response::ToolResult(proto::ToolOutcome::Ok { .. }) => {}
        other => panic!("export_outline failed: {other:?}"),
    }
    match core.handle_as(&anna, proto::Request::GetTask) {
        proto::Response::Task(task) => assert_eq!(task.artifacts.len(), 1),
        other => panic!("{other:?}"),
    }
    match core.handle_as(&ben, proto::Request::GetTask) {
        proto::Response::Task(task) => assert!(task.artifacts.is_empty()),
        other => panic!("{other:?}"),
    }

    // Events went to their user; a document's change went to everyone.
    let taken = events.lock().unwrap();
    let to_anna = taken
        .iter()
        .filter(|(to, _)| *to == To::User("anna".into()))
        .count();
    let to_ben = taken
        .iter()
        .filter(|(to, _)| *to == To::User("ben".into()))
        .count();
    assert!(to_anna > 0);
    assert!(
        taken
            .iter()
            .filter(|(to, _)| *to == To::User("ben".into()))
            .all(|(_, ev)| matches!(ev, proto::Event::EnvironmentChanged(_))),
        "Ben got nothing but his own environment: {to_ben} event(s)"
    );
    assert!(
        taken
            .iter()
            .any(|(to, ev)| *to == To::All && matches!(ev, proto::Event::DocChanged { .. })),
        "the sticky's change was everyone's cue"
    );
    assert!(
        !taken.iter().any(|(to, ev)| {
            *to == To::User("ben".into())
                && matches!(
                    ev,
                    proto::Event::AssistantDone { .. }
                        | proto::Event::TurnChanged { .. }
                        | proto::Event::TaskChanged(_)
                )
        }),
        "Anna's turn and ledger never reached Ben"
    );
    drop(taken);

    // The audit names the caller: user, session, address, role.
    let records = core.audit_log().records();
    let last_call = records
        .iter()
        .rev()
        .find(|r| r.event == "tool.call")
        .expect("a tool call was audited");
    assert_eq!(last_call.actor.user, "anna");
    assert_eq!(last_call.actor.session, "s_anna");
    assert_eq!(last_call.actor.ip, "10.0.0.7");
    assert_eq!(last_call.actor.role, "member");
    assert_ne!(
        last_call.scope.conversation, "c_1",
        "the real conversation, not a placeholder"
    );
}

#[test]
fn the_local_user_of_a_personal_core_is_its_admin_and_handle_is_theirs() {
    let Some((mut core, _events)) = core() else {
        return;
    };
    let local = Caller::local("anna");
    assert_eq!(local.roles, vec![proto::UserRole::Admin]);
    assert_eq!(local.session, "local");
    // `handle` is the local user's `handle_as`: the same state either way.
    core.handle(proto::Request::SendMessage {
        text: "through handle".into(),
        conversation: None,
    });
    assert_eq!(transcript(&mut core, &local).len(), 2);
    let other = member("ben");
    assert!(transcript(&mut core, &other).is_empty());
    match core.handle(proto::Request::GetTranscript) {
        proto::Response::Transcript { messages } => {
            assert_eq!(messages.len(), 2, "back to the local user")
        }
        other => panic!("{other:?}"),
    }
}

fn admin(name: &str) -> Caller {
    Caller {
        roles: vec![proto::UserRole::Admin],
        ..member(name)
    }
}

fn users(core: &mut Core, who: &Caller) -> Vec<proto::UserInfo> {
    match core.handle_as(who, proto::Request::ListUsers) {
        proto::Response::Users(users) => users,
        other => panic!("ListUsers failed: {other:?}"),
    }
}

#[test]
fn an_administrator_manages_accounts_and_a_member_may_not() {
    let Some((mut core, _events)) = core() else {
        return;
    };
    let root = admin("root");
    let anna = member("anna");

    // A member is refused, and the refusal is audited.
    match core.handle_as(&anna, proto::Request::ListUsers) {
        proto::Response::Error { message } => {
            assert!(message.contains("administrator"), "{message}")
        }
        other => panic!("a member listed the users: {other:?}"),
    }
    assert!(users(&mut core, &root).is_empty(), "no accounts yet");

    // The admin makes Ben; the answer is his one-time link's token.
    let invite = match core.handle_as(
        &root,
        proto::Request::CreateUser {
            email: "Ben@Example.com".into(),
            name: "Ben".into(),
            roles: vec![proto::UserRole::Member],
        },
    ) {
        proto::Response::Invite(invite) => invite,
        other => panic!("CreateUser failed: {other:?}"),
    };
    assert_eq!(invite.email, "ben@example.com");
    assert_eq!(invite.token.len(), 64, "32 random bytes as hex");
    let listed = users(&mut core, &root);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].email, "ben@example.com");
    assert!(!listed[0].has_password, "until the link is used");
    assert_eq!(listed[0].roles, vec![proto::UserRole::Member]);
    assert!(matches!(
        core.handle_as(
            &anna,
            proto::Request::SetUserRoles {
                user: invite.user.clone(),
                roles: vec![proto::UserRole::Admin]
            }
        ),
        proto::Response::Error { .. }
    ));
    match core.handle_as(
        &root,
        proto::Request::SetUserRoles {
            user: invite.user.clone(),
            roles: vec![proto::UserRole::Viewer],
        },
    ) {
        proto::Response::Users(list) => assert_eq!(list[0].roles, vec![proto::UserRole::Viewer]),
        other => panic!("{other:?}"),
    }

    let records = core.audit_log().records();
    let events: Vec<(&str, &str, &str)> = records
        .iter()
        .map(|r| (r.event.as_str(), r.actor.user.as_str(), r.result.as_str()))
        .collect();
    assert!(
        events.contains(&("user.list", "anna", "denied")),
        "{events:?}"
    );
    assert!(
        events.contains(&("user.create", "root", "ok")),
        "{events:?}"
    );
    assert!(events.contains(&("user.roles", "root", "ok")), "{events:?}");
}

#[test]
fn signing_in_is_the_server_s_request_and_never_a_client_s() {
    let Some((mut core, _events)) = core() else {
        return;
    };
    let root = admin("root");
    let anna = member("anna");
    let system = Caller::system();
    let invite = match core.handle_as(
        &root,
        proto::Request::CreateUser {
            email: "carla@example.com".into(),
            name: "Carla".into(),
            roles: vec![proto::UserRole::Member],
        },
    ) {
        proto::Response::Invite(invite) => invite,
        other => panic!("CreateUser failed: {other:?}"),
    };

    // A client cannot sign anyone in through the API.
    match core.handle_as(
        &anna,
        proto::Request::Login {
            email: "carla@example.com".into(),
            password: "whatever it is".into(),
            ip: String::new(),
            user_agent: String::new(),
        },
    ) {
        proto::Response::Error { message } => assert_eq!(message, "not a client request"),
        other => panic!("a client signed in: {other:?}"),
    }
    // Nor may the system caller do anything else.
    assert!(matches!(
        core.handle_as(&system, proto::Request::GetEnvironment),
        proto::Response::Error { .. }
    ));

    // The link is checked without being spent, then spent on a password.
    match core.handle_as(
        &system,
        proto::Request::InviteStatus {
            token: invite.token.clone(),
        },
    ) {
        proto::Response::InviteStatus { valid, email, .. } => {
            assert!(valid);
            assert_eq!(email.as_deref(), Some("carla@example.com"));
        }
        other => panic!("{other:?}"),
    }
    let session = match core.handle_as(
        &system,
        proto::Request::SetPassword {
            token: invite.token.clone(),
            password: "a long enough password".into(),
            ip: "10.0.0.5".into(),
            user_agent: "test".into(),
            email: None,
            name: None,
        },
    ) {
        proto::Response::SignedIn { session, user, .. } => {
            assert_eq!(user.email, "carla@example.com");
            assert!(user.has_password);
            session
        }
        other => panic!("SetPassword failed: {other:?}"),
    };
    assert_eq!(session.len(), 64);
    match core.handle_as(
        &system,
        proto::Request::InviteStatus {
            token: invite.token.clone(),
        },
    ) {
        proto::Response::InviteStatus { valid, .. } => assert!(!valid, "spent"),
        other => panic!("{other:?}"),
    }

    // The wrong password and a wrong email get the same sentence.
    let wrong = |core: &mut Core, email: &str, password: &str| match core.handle_as(
        &system,
        proto::Request::Login {
            email: email.into(),
            password: password.into(),
            ip: "10.0.0.5".into(),
            user_agent: "test".into(),
        },
    ) {
        proto::Response::Error { message } => message,
        other => panic!("signed in with the wrong credentials: {other:?}"),
    };
    let a = wrong(&mut core, "carla@example.com", "not her password");
    let b = wrong(&mut core, "nobody@example.com", "not her password");
    assert_eq!(a, b);
    assert_eq!(a, "That email or password isn't right.");

    match core.handle_as(
        &system,
        proto::Request::Login {
            email: "carla@example.com".into(),
            password: "a long enough password".into(),
            ip: "10.0.0.5".into(),
            user_agent: "test".into(),
        },
    ) {
        proto::Response::SignedIn { user, .. } => assert_eq!(user.name, "Carla"),
        other => panic!("Login failed: {other:?}"),
    }
    assert!(matches!(
        core.handle_as(&system, proto::Request::Logout { session }),
        proto::Response::Ok
    ));

    let records = core.audit_log().records();
    let auth: Vec<(&str, &str, &str)> = records
        .iter()
        .filter(|r| r.event.starts_with("auth."))
        .map(|r| (r.event.as_str(), r.actor.ip.as_str(), r.result.as_str()))
        .collect();
    assert!(
        auth.contains(&("auth.password_set", "10.0.0.5", "ok")),
        "{auth:?}"
    );
    assert!(
        auth.contains(&("auth.failed", "10.0.0.5", "denied")),
        "{auth:?}"
    );
    assert!(auth.contains(&("auth.login", "10.0.0.5", "ok")), "{auth:?}");
    assert!(auth.iter().any(|(e, _, _)| *e == "auth.logout"), "{auth:?}");
    let failed = records.iter().find(|r| r.event == "auth.failed").unwrap();
    assert_eq!(failed.detail["reason"], "wrong_password");
}
