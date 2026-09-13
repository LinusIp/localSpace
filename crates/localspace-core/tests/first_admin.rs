//! The first administrator (the fourth answer of 2026-09-13): a link the
//! server mints while there are no accounts at all, asking for a name, an
//! email and a password since it knows none; single-use; minted again only
//! while there is still nobody, which kills the earlier link; and the file it
//! was written to goes when it is used.

use localspace_core::{Caller, Config, Core, identity};
use localspace_proto as proto;
use std::path::Path;

fn core(data: &Path) -> Core {
    let mut cfg = Config::organisation("operator");
    cfg.data_dir = Some(data.to_path_buf());
    Core::new(cfg).unwrap()
}

fn mint(core: &mut Core) -> proto::Response {
    core.handle_as(&Caller::system(), proto::Request::Bootstrap)
}

fn status(core: &mut Core, token: &str) -> (bool, bool, Option<String>) {
    match core.handle_as(
        &Caller::system(),
        proto::Request::InviteStatus {
            token: token.into(),
        },
    ) {
        proto::Response::InviteStatus {
            valid,
            first_admin,
            email,
            ..
        } => (valid, first_admin, email),
        other => panic!("InviteStatus failed: {other:?}"),
    }
}

fn accept(
    core: &mut Core,
    token: &str,
    password: &str,
    email: Option<&str>,
    name: Option<&str>,
) -> proto::Response {
    core.handle_as(
        &Caller::system(),
        proto::Request::SetPassword {
            token: token.into(),
            password: password.into(),
            ip: "10.0.0.1".into(),
            user_agent: "test".into(),
            email: email.map(str::to_string),
            name: name.map(str::to_string),
        },
    )
}

#[test]
fn the_first_administrator_comes_from_a_link_that_asks_who_they_are() {
    let data = tempfile::tempdir().unwrap();
    let mut core = core(data.path());

    let first = match mint(&mut core) {
        proto::Response::Invite(invite) => invite,
        other => panic!("{other:?}"),
    };
    assert!(first.first_admin);
    assert!(first.email.is_empty() && first.user.is_empty());
    assert_eq!(status(&mut core, &first.token), (true, true, None));

    // Minted again while there is still nobody: the earlier link dies, so
    // only the newest — the one in the file and the log — opens.
    let second = match mint(&mut core) {
        proto::Response::Invite(invite) => invite,
        other => panic!("{other:?}"),
    };
    assert!(!status(&mut core, &first.token).0, "replaced");
    assert_eq!(status(&mut core, &second.token), (true, true, None));

    // Whoever minted it writes the file; Core deletes it when the link is used.
    let path = identity::write_first_admin_link(
        data.path(),
        &format!("https://ai.example.test/invite/{}", second.token),
    )
    .unwrap();
    assert_eq!(path, identity::first_admin_link_path(data.path()));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(&second.token), "{text}");
    assert!(text.contains("24 hours"), "{text}");

    // Without a name and an email: refused, the link still live.
    match accept(
        &mut core,
        &second.token,
        "a root password of length",
        None,
        None,
    ) {
        proto::Response::Error { message } => assert!(message.contains("name"), "{message}"),
        other => panic!("{other:?}"),
    }
    match accept(
        &mut core,
        &second.token,
        "short",
        Some("root@example.com"),
        Some("Root"),
    ) {
        proto::Response::Error { message } => assert!(message.contains("12"), "{message}"),
        other => panic!("{other:?}"),
    }
    assert!(status(&mut core, &second.token).0);

    // With them: signed in as the administrator, the link spent, the file gone.
    let admin = match accept(
        &mut core,
        &second.token,
        "a root password of length",
        Some("Root@Example.com"),
        Some("  Root  "),
    ) {
        proto::Response::SignedIn { user, session, .. } => {
            assert!(!session.is_empty());
            user
        }
        other => panic!("{other:?}"),
    };
    assert_eq!(admin.email, "root@example.com");
    assert_eq!(admin.name, "Root");
    assert!(admin.roles.contains(&proto::UserRole::Admin));
    assert!(!path.exists(), "the file goes when the link is used");
    assert!(!status(&mut core, &second.token).0, "single use");

    // Nobody else comes in this way.
    match mint(&mut core) {
        proto::Response::Error { message } => assert!(message.contains("already"), "{message}"),
        other => panic!("{other:?}"),
    }

    // The audit has the link and the administrator it became.
    let records = core.audit_log().records();
    assert_eq!(
        records
            .iter()
            .filter(|r| r.event == "user.first_admin_link")
            .count(),
        2
    );
    let made = records
        .iter()
        .find(|r| r.event == "user.first_admin")
        .expect("the first administrator is audited");
    assert_eq!(made.actor.user, admin.id);
    assert_eq!(made.detail["email"], "root@example.com");
}

#[test]
fn a_user_s_link_is_not_the_first_administrator_s() {
    let data = tempfile::tempdir().unwrap();
    let mut core = core(data.path());
    let token = match mint(&mut core) {
        proto::Response::Invite(invite) => invite.token,
        other => panic!("{other:?}"),
    };
    let admin = match accept(
        &mut core,
        &token,
        "a root password of length",
        Some("root@example.com"),
        Some("Root"),
    ) {
        proto::Response::SignedIn { user, .. } => user,
        other => panic!("{other:?}"),
    };

    // The administrator makes a member; her link knows who she is and takes
    // a password alone, ignoring any name or email sent with it.
    let root = Caller {
        user: admin.id.clone(),
        session: "s_root".into(),
        ip: "10.0.0.1".into(),
        roles: vec![proto::UserRole::Admin],
        groups: Vec::new(),
    };
    let anna = match core.handle_as(
        &root,
        proto::Request::CreateUser {
            email: "anna@example.com".into(),
            name: "Anna".into(),
            roles: vec![proto::UserRole::Member],
        },
    ) {
        proto::Response::Invite(invite) => invite,
        other => panic!("{other:?}"),
    };
    assert!(!anna.first_admin);
    assert_eq!(
        status(&mut core, &anna.token),
        (true, false, Some("anna@example.com".into()))
    );
    match accept(
        &mut core,
        &anna.token,
        "anna has a long password",
        Some("someone-else@example.com"),
        Some("Not Anna"),
    ) {
        proto::Response::SignedIn { user, .. } => {
            assert_eq!(user.email, "anna@example.com");
            assert_eq!(user.name, "Anna");
            assert!(!user.roles.contains(&proto::UserRole::Admin));
        }
        other => panic!("{other:?}"),
    }
}
