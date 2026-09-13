//! `localspace serve` from the outside: what it refuses before a socket
//! opens, with the one message each refusal carries (Pilot 1, the answers of
//! 2026-09-13), and that plaintext off loopback is served only when asked
//! for by name, loudly.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_localspace");

/// Which build is under test, for the message when it does not start.
fn build_id() -> &'static str {
    option_env!("LOCALSPACE_BUILD_ID").unwrap_or("local build")
}

fn write(dir: &Path, text: &str) -> std::path::PathBuf {
    let path = dir.join("localspace.toml");
    std::fs::write(&path, text).unwrap();
    path
}

/// Run `serve` to its end: what it printed, and how it ended.
fn serve(config: &Path, extra: &[&str]) -> (i32, String, String) {
    let output = Command::new(BIN)
        .arg("serve")
        .arg("--config")
        .arg(config)
        .args(extra)
        .output()
        .unwrap_or_else(|e| panic!("the binary ({}) did not run: {e}", build_id()));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A server that must not outlive its test.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn native_tls_refuses_to_start_and_names_the_supported_path() {
    let dir = tempfile::tempdir().unwrap();
    let config = write(
        dir.path(),
        r#"
[server]
bind = "127.0.0.1:0"
tls = { cert = "/etc/localspace/tls/fullchain.pem", key = "/etc/localspace/tls/privkey.pem" }
"#,
    );
    let (code, _, err) = serve(&config, &["--allow-below-floor"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("TLS termination is not built in yet"), "{err}");
    assert!(err.contains("behind-proxy"), "{err}");
    assert!(err.contains("trusted_proxies"), "{err}");
}

#[test]
fn a_key_this_release_does_not_honour_refuses_to_start_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let config = write(
        dir.path(),
        "[audit]\nsink = [\"local\"]\nretention_days = 730\n",
    );
    let (code, _, err) = serve(&config, &["--allow-below-floor"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("`audit.retention_days`"), "{err}");
    assert!(err.contains("not enforced in this release"), "{err}");
}

#[test]
fn plaintext_off_loopback_is_refused_unless_asked_for_by_name_and_then_logged() {
    let dir = tempfile::tempdir().unwrap();
    let config = write(
        dir.path(),
        r#"
[server]
bind = "0.0.0.0:0"
public_url = "http://ai.example.test"
"#,
    );
    let (code, _, err) = serve(&config, &["--allow-below-floor"]);
    assert_eq!(code, 2, "{err}");
    assert!(
        err.contains("refuses to serve plaintext off loopback"),
        "{err}"
    );
    assert!(err.contains("--insecure"), "{err}");

    // Asked for by name, it starts, and says so where the operator reads.
    let child = Command::new(BIN)
        .arg("serve")
        .arg("--config")
        .arg(&config)
        .args(["--allow-below-floor", "--insecure", "--personal"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("the binary ({}) did not run: {e}", build_id()));
    let mut running = Running(child);
    let stdout = running.0.stdout.take().expect("stdout is piped");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let started = Instant::now();
    let mut seen = Vec::new();
    let mut warned = false;
    while started.elapsed() < Duration::from_secs(90) {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => {
                warned |= line.contains("INSECURE") && line.contains("plaintext");
                seen.push(line);
                if warned {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(Some(status)) = running.0.try_wait() {
                    panic!("the server ended with {status}: {seen:?}");
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(warned, "no loud line about plaintext: {seen:?}");
}

#[test]
fn a_token_needs_personal_mode() {
    let dir = tempfile::tempdir().unwrap();
    let config = write(dir.path(), "");
    let (code, _, err) = serve(&config, &["--token", "t"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--personal"), "{err}");
}

#[test]
fn a_missing_settings_file_named_on_the_command_line_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (code, _, err) = serve(&dir.path().join("absent.toml"), &["--allow-below-floor"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("absent.toml"), "{err}");
}
