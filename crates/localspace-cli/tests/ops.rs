//! The operator's commands from the outside: `doctor` reports; `audit verify`
//! walks what the offline admin command wrote; a command that needs the data
//! says where to look.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_localspace");

/// Which build is under test, for the message when it does not start.
fn build_id() -> &'static str {
    option_env!("LOCALSPACE_BUILD_ID").unwrap_or("local build")
}

fn run(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(BIN)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("the binary ({}) did not run: {e}", build_id()));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn doctor_reports_the_machine_and_a_verdict() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("localspace.toml");
    std::fs::write(&config, "").unwrap();
    let (code, out, err) = run(&["doctor", "--config", config.to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("settings  "), "{out}");
    assert!(out.contains("machine   "), "{out}");
    assert!(out.contains("verdict   "), "{out}");
}

#[test]
fn audit_verify_walks_what_the_offline_command_wrote() {
    let data = tempfile::tempdir().unwrap();
    let dir = data.path().to_str().unwrap();
    let (code, _, err) = run(&["audit", "--data", dir, "verify"]);
    assert_eq!(code, 0, "{err}");

    let (code, _, err) = run(&["admin", "--data", dir, "bootstrap"]);
    assert_eq!(code, 0, "{err}");
    let (code, out, err) = run(&["audit", "--data", dir, "verify"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("1 record(s) in 1 file(s)"), "{out}");
    assert!(out.contains("the chain is intact"), "{out}");

    // A second record chains to the first; the first, edited on disk after
    // that, breaks the link and verify says so.
    let (code, _, err) = run(&["admin", "--data", dir, "bootstrap"]);
    assert_eq!(code, 0, "{err}");
    let (code, out, err) = run(&["audit", "--data", dir, "verify"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("2 record(s)"), "{out}");
    let audit_dir = data.path().join("audit");
    let file = std::fs::read_dir(&audit_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "jsonl"))
        .expect("the day's file");
    let text = std::fs::read_to_string(&file).unwrap();
    let edited = text.replacen("\"result\":\"ok\"", "\"result\":\"denied\"", 1);
    assert_ne!(edited, text);
    std::fs::write(&file, edited).unwrap();
    let (code, _, err) = run(&["audit", "--data", dir, "verify"]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("expected prev"), "{err}");
}

#[test]
fn a_command_that_needs_the_data_says_where_to_look() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("localspace.toml");
    std::fs::write(&config, "[server]\nbind = \"127.0.0.1:8443\"\n").unwrap();
    let (code, _, err) = run(&["audit", "--config", config.to_str().unwrap(), "verify"]);
    assert_ne!(code, 0);
    assert!(err.contains("[storage] root"), "{err}");
}

#[test]
fn evals_names_the_catalogs_when_the_harness_is_not_there() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = dir.path().join("catalog");
    std::fs::create_dir_all(&catalog).unwrap();
    let config = dir.path().join("localspace.toml");
    std::fs::write(
        &config,
        format!(
            "[harnesses]\ncatalogs = [{:?}]\n",
            catalog.to_string_lossy()
        ),
    )
    .unwrap();
    let (code, _, err) = run(&[
        "evals",
        "io.example.nothing",
        "--config",
        config.to_str().unwrap(),
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("no package `io.example.nothing`"), "{err}");
    assert!(err.contains("[harnesses] catalogs"), "{err}");
}
