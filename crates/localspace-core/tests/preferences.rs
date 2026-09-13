//! What the shell remembers with the user (the shell answers of 2026-09-12:
//! the rail's state lives server-side): a fixed set of keys, per user,
//! across a restart; anything else refused.

use localspace_core::{Caller, Config, Core};
use localspace_proto as proto;
use std::path::Path;

fn core(data: &Path) -> Core {
    let mut cfg = Config::organisation("operator");
    cfg.data_dir = Some(data.to_path_buf());
    Core::new(cfg).unwrap()
}

fn member(name: &str) -> Caller {
    Caller {
        user: name.into(),
        session: format!("s_{name}"),
        ip: String::new(),
        roles: vec![proto::UserRole::Member],
        groups: Vec::new(),
    }
}

fn values(core: &mut Core, who: &Caller) -> std::collections::BTreeMap<String, String> {
    match core.handle_as(who, proto::Request::GetPreferences) {
        proto::Response::Preferences { values } => values,
        other => panic!("GetPreferences failed: {other:?}"),
    }
}

#[test]
fn the_rail_is_remembered_per_user_across_a_restart_and_only_known_keys_are_kept() {
    let data = tempfile::tempdir().unwrap();
    let anna = member("anna");
    let ben = member("ben");
    {
        let mut core = core(data.path());
        assert!(
            values(&mut core, &anna).is_empty(),
            "nothing remembered yet"
        );
        match core.handle_as(
            &anna,
            proto::Request::SetPreference {
                key: "rail_collapsed".into(),
                value: "true".into(),
            },
        ) {
            proto::Response::Preferences { values } => assert_eq!(
                values.get("rail_collapsed").map(String::as_str),
                Some("true")
            ),
            other => panic!("{other:?}"),
        }
        assert!(values(&mut core, &ben).is_empty(), "Ben's rail is his own");
        match core.handle_as(
            &anna,
            proto::Request::SetPreference {
                key: "favourite_colour".into(),
                value: "green".into(),
            },
        ) {
            proto::Response::Error { message } => {
                assert!(message.contains("favourite_colour"), "{message}")
            }
            other => panic!("an unknown key was kept: {other:?}"),
        }
        match core.handle_as(
            &anna,
            proto::Request::SetPreference {
                key: "rail_collapsed".into(),
                value: "x".repeat(201),
            },
        ) {
            proto::Response::Error { message } => assert!(message.contains("200"), "{message}"),
            other => panic!("an oversized value was kept: {other:?}"),
        }
    }
    let mut core = core(data.path());
    assert_eq!(
        values(&mut core, &anna)
            .get("rail_collapsed")
            .map(String::as_str),
        Some("true")
    );
    assert!(values(&mut core, &ben).is_empty());
}
