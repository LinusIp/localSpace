//! `localspace admin` from the outside: the offline paths (the fourth answer
//! of 2026-09-13) mint the first administrator's link into the file, refuse
//! when accounts exist, and say when there is no such account to reset.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_localspace");

fn admin(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(BIN)
        .arg("admin")
        .args(args)
        .output()
        .expect("the binary runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn token_in(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.trim()
            .rsplit_once("/invite/")
            .map(|(_, t)| t.trim().to_string())
    })
}

#[test]
fn bootstrap_mints_the_first_administrator_s_link_into_the_file() {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().to_str().unwrap();
    let (code, out, err) = admin(&["--data", dir, "bootstrap"]);
    assert_eq!(code, 0, "{err}");
    let token = token_in(&out).expect("a link in the output");
    assert_eq!(token.len(), 64, "32 random bytes as hex: {token}");
    let file = Path::new(dir).join("first-admin-link.txt");
    let text = std::fs::read_to_string(&file).expect("the link is written for the service user");
    assert_eq!(token_in(&text).as_deref(), Some(token.as_str()));
    assert!(
        out.contains("put your server's address"),
        "no settings file, so no address: {out}"
    );

    // Again, while there is still nobody: a new link, the file replaced.
    let (code, out, err) = admin(&["--data", dir, "bootstrap"]);
    assert_eq!(code, 0, "{err}");
    let again = token_in(&out).unwrap();
    assert_ne!(again, token);
    assert_eq!(
        token_in(&std::fs::read_to_string(&file).unwrap()).as_deref(),
        Some(again.as_str())
    );
}

#[test]
fn with_a_settings_file_the_link_carries_the_public_address() {
    let data = tempfile::tempdir().unwrap();
    let config = data.path().join("localspace.toml");
    std::fs::write(
        &config,
        format!(
            "[server]\npublic_url = \"https://ai.example.test/\"\n[storage]\nroot = {:?}\n",
            data.path().join("root").to_string_lossy()
        ),
    )
    .unwrap();
    let (code, out, err) = admin(&["--config", config.to_str().unwrap(), "bootstrap"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("https://ai.example.test/invite/"), "{out}");
    assert!(!out.contains("put your server's address"), "{out}");
    assert!(
        data.path()
            .join("root")
            .join("first-admin-link.txt")
            .exists()
    );
}

#[test]
fn reset_password_names_an_account_that_is_not_there() {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().to_str().unwrap();
    let (code, _, err) = admin(&[
        "--data",
        dir,
        "reset-password",
        "--email",
        "Nobody@Example.com",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("no account for nobody@example.com"), "{err}");
}

#[test]
fn without_a_place_to_look_the_command_says_what_to_pass() {
    let data = tempfile::tempdir().unwrap();
    let config = data.path().join("localspace.toml");
    std::fs::write(&config, "[server]\nbind = \"127.0.0.1:8443\"\n").unwrap();
    let (code, _, err) = admin(&["--config", config.to_str().unwrap(), "bootstrap"]);
    assert_ne!(code, 0);
    assert!(err.contains("[storage] root"), "{err}");
    assert!(err.contains("--data"), "{err}");
}
