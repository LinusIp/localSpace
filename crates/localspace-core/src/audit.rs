//! Append-only, hash-chained audit log (deployment spec §10.1).
//!
//! Each record carries the blake3 of the previous one, so a deleted or edited
//! record breaks the chain and `verify` finds it. Records go to one file per
//! UTC day, `audit/YYYY-MM-DD.jsonl`, and the chain runs across the files: the
//! first record of a day carries the hash of the last record of the day
//! before. Core's thread computes the chain and hands each record to a writer
//! thread over a bounded channel, so a tool call does not wait on the disk —
//! unless the writer is `QUEUE` records behind, and then it waits rather than
//! forgets: the local file is the record, and §16.4's rule that the hot path
//! never waits is about the SIEM. `localspace audit verify` walks every file
//! in order.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub const GENESIS: &str = "genesis";

/// How many records the writer may be behind before an append waits for it.
const QUEUE: usize = 4096;

/// The one file the log was kept in before it was cut by day (2026-09-13).
/// Read first, never written again.
const LEGACY_FILE: &str = "audit.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub ts_ms: u64,
    pub id: String,
    /// blake3 of the previous record's canonical bytes, or `genesis`.
    pub prev: String,
    pub actor: Actor,
    pub scope: Scope,
    pub event: String,
    pub detail: serde_json::Value,
    pub result: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Actor {
    pub user: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub role: String,
    /// The reason an administrator gave to be in a workspace they are not a
    /// member of, on every record they make while there (deployment §6.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub break_glass: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scope {
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub conversation: String,
    #[serde(default)]
    pub document: String,
}

impl Record {
    /// The bytes the next record's `prev` hashes over. Excludes nothing: a change
    /// anywhere in the record breaks the chain.
    pub fn canonical(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    pub fn hash(&self) -> String {
        blake3::hash(&self.canonical()).to_hex().to_string()
    }
}

/// The UTC day a record belongs to, and the name of its file: `YYYY-MM-DD`.
pub fn day_of(ts_ms: u64) -> String {
    let (y, m, d) = civil_from_days((ts_ms / 86_400_000) as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days since 1970-01-01 to a proleptic Gregorian date, after Howard
/// Hinnant's `civil_from_days`. No calendar crate for one function.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_day_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(stem) = name.strip_suffix(".jsonl") else {
        return false;
    };
    stem.len() == 10
        && stem.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

/// The log's files, oldest first: the one file from before the daily cut,
/// if it is there, then one per day.
pub fn files_in_order(dir: &Path) -> Vec<PathBuf> {
    let mut days: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_day_file(p))
                .collect()
        })
        .unwrap_or_default();
    days.sort();
    let mut out = Vec::new();
    let legacy = dir.join(LEGACY_FILE);
    if legacy.exists() {
        out.push(legacy);
    }
    out.extend(days);
    out
}

/// The newest day that has a file, so a writer never opens an older one.
fn newest_day(dir: &Path) -> Option<String> {
    files_in_order(dir)
        .into_iter()
        .rev()
        .find(|p| is_day_file(p))
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
}

/// The last record on disk, from the newest file that has one.
fn last_record(dir: &Path) -> Result<Option<Record>> {
    for file in files_in_order(dir).into_iter().rev() {
        let mut records = read_records(&file)?;
        if let Some(last) = records.pop() {
            return Ok(Some(last));
        }
    }
    Ok(None)
}

/// The sequence number after a record's: its own plus one, or a count of
/// everything on disk if its id is not one of ours.
fn seq_after(dir: &Path, last: &Record) -> Result<u64> {
    if let Some(n) = last
        .id
        .strip_prefix('a')
        .and_then(|s| s.parse::<u64>().ok())
    {
        return Ok(n + 1);
    }
    Ok(read_all(dir)?.len() as u64)
}

enum Job {
    /// Boxed: a record is a few hundred bytes, a flush request sixteen.
    Write(Box<Record>),
    Flush(SyncSender<()>),
}

struct Writer {
    tx: Option<SyncSender<Job>>,
    handle: Option<JoinHandle<()>>,
    failures: Arc<AtomicU64>,
}

/// The writer thread: one file per day, opened on the first record of that
/// day, never an older one than the last it wrote to. A record it cannot
/// write is counted and reported; the chain in memory is unaffected.
fn run_writer(dir: PathBuf, rx: Receiver<Job>, floor: Option<String>, failures: Arc<AtomicU64>) {
    let mut open: Option<(String, std::fs::File)> = None;
    let mut floor = floor.unwrap_or_default();
    while let Ok(job) = rx.recv() {
        match job {
            Job::Write(record) => {
                let mut day = day_of(record.ts_ms);
                if day < floor {
                    day = floor.clone();
                }
                if open.as_ref().is_none_or(|(current, _)| *current != day) {
                    let path = dir.join(format!("{day}.jsonl"));
                    match std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)
                    {
                        Ok(file) => {
                            floor = day.clone();
                            open = Some((day, file));
                        }
                        Err(e) => {
                            failures.fetch_add(1, Ordering::Relaxed);
                            tracing::error!(
                                path = %path.display(),
                                error = %e,
                                "audit: the day's file could not be opened; record not written"
                            );
                            open = None;
                            continue;
                        }
                    }
                }
                let Some((_, file)) = open.as_mut() else {
                    continue;
                };
                let line = match serde_json::to_string(&record) {
                    Ok(line) => line,
                    Err(e) => {
                        failures.fetch_add(1, Ordering::Relaxed);
                        tracing::error!(error = %e, "audit: a record could not be serialised");
                        continue;
                    }
                };
                if let Err(e) = writeln!(file, "{line}") {
                    failures.fetch_add(1, Ordering::Relaxed);
                    tracing::error!(error = %e, "audit: a record could not be written");
                }
            }
            Job::Flush(reply) => {
                if let Some((_, file)) = open.as_mut() {
                    let _ = file.flush();
                }
                let _ = reply.send(());
            }
        }
    }
    if let Some((_, mut file)) = open {
        let _ = file.flush();
    }
}

pub struct AuditLog {
    dir: Option<PathBuf>,
    state: Mutex<State>,
    writer: Option<Writer>,
}

struct State {
    prev: String,
    seq: u64,
    /// The records of a log kept in memory only (tests, `--ephemeral`); a
    /// log on disk keeps nothing here.
    records: Vec<Record>,
}

impl AuditLog {
    pub fn in_memory() -> AuditLog {
        AuditLog {
            dir: None,
            state: Mutex::new(State {
                prev: GENESIS.into(),
                seq: 0,
                records: Vec::new(),
            }),
            writer: None,
        }
    }

    /// Open the log kept in a directory, continuing its chain from the last
    /// record on disk, and start its writer.
    pub fn open(dir: &Path) -> Result<AuditLog> {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let last = last_record(dir)?;
        let (prev, seq) = match &last {
            Some(record) => (record.hash(), seq_after(dir, record)?),
            None => (GENESIS.to_string(), 0),
        };
        let floor = newest_day(dir);
        let failures = Arc::new(AtomicU64::new(0));
        let (tx, rx) = sync_channel(QUEUE);
        let handle = std::thread::Builder::new()
            .name("audit-writer".into())
            .spawn({
                let dir = dir.to_path_buf();
                let failures = failures.clone();
                move || run_writer(dir, rx, floor, failures)
            })
            .context("starting the audit writer")?;
        Ok(AuditLog {
            dir: Some(dir.to_path_buf()),
            state: Mutex::new(State {
                prev,
                seq,
                records: Vec::new(),
            }),
            writer: Some(Writer {
                tx: Some(tx),
                handle: Some(handle),
                failures,
            }),
        })
    }

    pub fn append(
        &self,
        actor: Actor,
        scope: Scope,
        event: &str,
        detail: serde_json::Value,
        result: &str,
    ) -> Result<Record> {
        self.append_at(crate::dag::now_ms(), actor, scope, event, detail, result)
    }

    fn append_at(
        &self,
        ts_ms: u64,
        actor: Actor,
        scope: Scope,
        event: &str,
        detail: serde_json::Value,
        result: &str,
    ) -> Result<Record> {
        let record = {
            let mut st = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let record = Record {
                ts_ms,
                id: format!("a{:012}", st.seq),
                prev: st.prev.clone(),
                actor,
                scope,
                event: event.to_string(),
                detail,
                result: result.to_string(),
            };
            st.prev = record.hash();
            st.seq += 1;
            if self.writer.is_none() {
                st.records.push(record.clone());
            }
            record
        };
        if let Some(writer) = &self.writer
            && let Some(tx) = &writer.tx
        {
            tx.send(Job::Write(Box::new(record.clone())))
                .map_err(|_| anyhow::anyhow!("the audit writer has stopped"))?;
        }
        Ok(record)
    }

    /// Wait until every record appended so far has been handed to the disk.
    pub fn flush(&self) {
        if let Some(writer) = &self.writer
            && let Some(tx) = &writer.tx
        {
            let (reply, done) = sync_channel(1);
            if tx.send(Job::Flush(reply)).is_ok() {
                let _ = done.recv_timeout(Duration::from_secs(30));
            }
        }
    }

    /// Records the writer could not put on disk. Zero unless the disk failed.
    pub fn write_failures(&self) -> u64 {
        self.writer
            .as_ref()
            .map(|w| w.failures.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Records this log has chained, on disk and before.
    pub fn len(&self) -> usize {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).seq as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every record: the ones in memory, or everything on disk, written out
    /// first.
    pub fn records(&self) -> Vec<Record> {
        match &self.dir {
            None => self
                .state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .records
                .clone(),
            Some(dir) => {
                self.flush();
                match read_all(dir) {
                    Ok(records) => records,
                    Err(e) => {
                        tracing::error!(error = %e, "audit: the log could not be read back");
                        Vec::new()
                    }
                }
            }
        }
    }

    /// Walk the chain — across every file, for a log on disk. Returns how
    /// many records it holds.
    pub fn verify(&self) -> std::result::Result<usize, VerifyError> {
        match &self.dir {
            None => verify_chain(&self.state.lock().unwrap_or_else(|p| p.into_inner()).records),
            Some(dir) => {
                self.flush();
                verify_dir(dir).map(|report| report.records)
            }
        }
    }
}

impl Drop for AuditLog {
    /// The writer finishes what it was handed before the log goes.
    fn drop(&mut self) {
        if let Some(writer) = &mut self.writer {
            drop(writer.tx.take());
            if let Some(handle) = writer.handle.take() {
                let _ = handle.join();
            }
        }
    }
}

/// Where in the files a record is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub file: PathBuf,
    pub line: usize,
}

fn located(at: &Option<Location>) -> String {
    match at {
        Some(l) => format!(" at {}:{}", l.file.display(), l.line),
        None => String::new(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    /// A record's `prev` is not the hash of the record before it: one was
    /// edited, removed or inserted.
    #[error("record {index} ({id}){} expected prev {expected}, found {found}", located(.at))]
    Broken {
        index: usize,
        id: String,
        expected: String,
        found: String,
        at: Option<Location>,
    },
    #[error("{}:{line}: not an audit record: {message}", .file.display())]
    Malformed {
        file: PathBuf,
        line: usize,
        message: String,
    },
}

/// What a walk of the files found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub records: usize,
    pub files: usize,
    pub first_day: Option<String>,
    pub last_day: Option<String>,
}

pub fn verify_chain(records: &[Record]) -> std::result::Result<usize, VerifyError> {
    let mut prev = GENESIS.to_string();
    for (i, r) in records.iter().enumerate() {
        if r.prev != prev {
            return Err(VerifyError::Broken {
                index: i,
                id: r.id.clone(),
                expected: prev,
                found: r.prev.clone(),
                at: None,
            });
        }
        prev = r.hash();
    }
    Ok(records.len())
}

/// Walk every file of a log in order, one line at a time, and check each
/// record against the one before it, across files.
pub fn verify_dir(dir: &Path) -> std::result::Result<Report, VerifyError> {
    let mut prev = GENESIS.to_string();
    let mut report = Report::default();
    for file in files_in_order(dir) {
        report.files += 1;
        let text = std::fs::read_to_string(&file).map_err(|e| VerifyError::Malformed {
            file: file.clone(),
            line: 0,
            message: e.to_string(),
        })?;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: Record =
                serde_json::from_str(line).map_err(|e| VerifyError::Malformed {
                    file: file.clone(),
                    line: i + 1,
                    message: e.to_string(),
                })?;
            if record.prev != prev {
                return Err(VerifyError::Broken {
                    index: report.records,
                    id: record.id,
                    expected: prev,
                    found: record.prev,
                    at: Some(Location { file, line: i + 1 }),
                });
            }
            prev = record.hash();
            report.records += 1;
            let day = day_of(record.ts_ms);
            if report.first_day.is_none() {
                report.first_day = Some(day.clone());
            }
            report.last_day = Some(day);
        }
    }
    Ok(report)
}

pub fn read_records(path: &Path) -> Result<Vec<Record>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(line)
                .with_context(|| format!("{}:{}: malformed audit record", path.display(), i + 1))?,
        );
    }
    Ok(out)
}

/// Every record in a log's directory, oldest first.
pub fn read_all(dir: &Path) -> Result<Vec<Record>> {
    let mut out = Vec::new();
    for file in files_in_order(dir) {
        out.extend(read_records(&file)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 2026-09-12T00:00:00Z and the day after.
    const DAY_1: u64 = 20_708 * 86_400_000;
    const DAY_2: u64 = DAY_1 + 86_400_000;

    fn actor() -> Actor {
        Actor {
            user: "u_123".into(),
            session: "s_9ab".into(),
            ip: "10.1.4.22".into(),
            role: "member".into(),
            break_glass: None,
        }
    }

    fn day_file(dir: &Path, day: &str) -> PathBuf {
        dir.join(format!("{day}.jsonl"))
    }

    #[test]
    fn the_chain_verifies_after_many_records() {
        let log = AuditLog::in_memory();
        for i in 0..50 {
            log.append(
                actor(),
                Scope {
                    workspace: "ws_finance".into(),
                    ..Default::default()
                },
                "tool.call",
                json!({"tool": format!("canvas.add_shape_{i}")}),
                "ok",
            )
            .unwrap();
        }
        assert_eq!(log.verify().unwrap(), 50);
        assert_eq!(log.records()[0].prev, GENESIS);
    }

    #[test]
    fn editing_a_record_breaks_the_chain_at_the_next_link() {
        let log = AuditLog::in_memory();
        for _ in 0..5 {
            log.append(actor(), Scope::default(), "tool.call", json!({}), "ok")
                .unwrap();
        }
        let mut records = log.records();
        // Someone rewrites the third record's result.
        records[2].result = "denied".into();
        let err = verify_chain(&records).unwrap_err();
        match err {
            VerifyError::Broken { index, .. } => {
                assert_eq!(index, 3, "the break shows at the next link")
            }
            other => panic!("{other}"),
        }
    }

    #[test]
    fn deleting_a_record_breaks_the_chain() {
        let log = AuditLog::in_memory();
        for _ in 0..5 {
            log.append(actor(), Scope::default(), "login", json!({}), "ok")
                .unwrap();
        }
        let mut records = log.records();
        records.remove(2);
        assert!(verify_chain(&records).is_err());
    }

    #[test]
    fn days_are_named_in_utc() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(951_782_400_000), "2000-02-29");
        assert_eq!(day_of(DAY_1), "2026-09-12");
        assert_eq!(day_of(DAY_2 - 1), "2026-09-12");
        assert_eq!(day_of(DAY_2), "2026-09-13");
        assert_eq!(day_of(1_789_243_111_609), "2026-09-12");
    }

    #[test]
    fn a_log_reopened_from_disk_continues_its_chain() {
        let dir = tempfile::tempdir().unwrap();

        let a = AuditLog::open(dir.path()).unwrap();
        a.append(actor(), Scope::default(), "login", json!({}), "ok")
            .unwrap();
        a.append(actor(), Scope::default(), "tool.call", json!({}), "ok")
            .unwrap();
        drop(a);

        let b = AuditLog::open(dir.path()).unwrap();
        let third = b
            .append(actor(), Scope::default(), "logout", json!({}), "ok")
            .unwrap();
        assert_eq!(third.id, "a000000000002");
        assert_eq!(b.len(), 3);
        assert_eq!(b.verify().unwrap(), 3);
        assert_eq!(b.write_failures(), 0);
        drop(b);

        // And on disk: one file, today's.
        let files = files_in_order(dir.path());
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!(
            files[0],
            day_file(dir.path(), &day_of(crate::dag::now_ms()))
        );
        assert_eq!(verify_dir(dir.path()).unwrap().records, 3);
    }

    #[test]
    fn records_go_to_the_file_of_their_day_and_the_chain_runs_across_days() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        log.append_at(DAY_1, actor(), Scope::default(), "login", json!({}), "ok")
            .unwrap();
        log.append_at(
            DAY_1 + 1000,
            actor(),
            Scope::default(),
            "tool.call",
            json!({}),
            "ok",
        )
        .unwrap();
        log.append_at(DAY_2, actor(), Scope::default(), "logout", json!({}), "ok")
            .unwrap();
        log.flush();

        let first_day = read_records(&day_file(dir.path(), "2026-09-12")).unwrap();
        let second_day = read_records(&day_file(dir.path(), "2026-09-13")).unwrap();
        assert_eq!(first_day.len(), 2);
        assert_eq!(second_day.len(), 1);
        assert_eq!(
            second_day[0].prev,
            first_day[1].hash(),
            "the new day continues the old one's chain"
        );
        assert_eq!(
            verify_dir(dir.path()).unwrap(),
            Report {
                records: 3,
                files: 2,
                first_day: Some("2026-09-12".into()),
                last_day: Some("2026-09-13".into()),
            }
        );
        assert_eq!(log.verify().unwrap(), 3);
        assert_eq!(log.records().len(), 3);
    }

    #[test]
    fn a_record_edited_in_an_older_file_is_found_with_its_file_and_line() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        for ts in [DAY_1, DAY_1 + 1, DAY_2] {
            log.append_at(ts, actor(), Scope::default(), "login", json!({}), "ok")
                .unwrap();
        }
        drop(log);

        // The last record of the first day is rewritten in place.
        let path = day_file(dir.path(), "2026-09-12");
        let text = std::fs::read_to_string(&path).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        lines[1] = lines[1].replace("\"result\":\"ok\"", "\"result\":\"denied\"");
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();

        match verify_dir(dir.path()).unwrap_err() {
            VerifyError::Broken { index, at, .. } => {
                assert_eq!(
                    index, 2,
                    "the break shows at the next link, in the next file"
                );
                let at = at.expect("a location on disk");
                assert_eq!(at.file, day_file(dir.path(), "2026-09-13"));
                assert_eq!(at.line, 1);
            }
            other => panic!("{other}"),
        }

        // A line removed from the older file breaks the newer one's first link.
        let two = std::fs::read_to_string(&path).unwrap();
        let kept: Vec<&str> = two.lines().take(1).collect();
        std::fs::write(&path, format!("{}\n", kept.join("\n"))).unwrap();
        let fresh = AuditLog::open(dir.path()).unwrap();
        assert!(fresh.verify().is_err());
    }

    #[test]
    fn a_record_stamped_before_the_current_day_stays_in_the_current_file() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        log.append_at(DAY_2, actor(), Scope::default(), "login", json!({}), "ok")
            .unwrap();
        // The clock stepped back across midnight: no older file is opened,
        // so the files stay in chain order.
        log.append_at(
            DAY_1,
            actor(),
            Scope::default(),
            "tool.call",
            json!({}),
            "ok",
        )
        .unwrap();
        drop(log);
        let files = files_in_order(dir.path());
        assert_eq!(files, vec![day_file(dir.path(), "2026-09-13")]);
        assert_eq!(verify_dir(dir.path()).unwrap().records, 2);

        // And after a restart with the clock still behind.
        let again = AuditLog::open(dir.path()).unwrap();
        again
            .append_at(
                DAY_1 + 5,
                actor(),
                Scope::default(),
                "logout",
                json!({}),
                "ok",
            )
            .unwrap();
        drop(again);
        assert_eq!(files_in_order(dir.path()).len(), 1);
        assert_eq!(verify_dir(dir.path()).unwrap().records, 3);
    }

    #[test]
    fn the_file_from_before_the_daily_cut_is_read_first_and_continued_from() {
        let dir = tempfile::tempdir().unwrap();
        // What the previous binary left: one file, its chain from genesis.
        let old = AuditLog::in_memory();
        for _ in 0..4 {
            old.append(actor(), Scope::default(), "login", json!({}), "ok")
                .unwrap();
        }
        let legacy: Vec<String> = old
            .records()
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect();
        std::fs::write(
            dir.path().join(LEGACY_FILE),
            format!("{}\n", legacy.join("\n")),
        )
        .unwrap();
        let last_legacy = old.records()[3].hash();

        let log = AuditLog::open(dir.path()).unwrap();
        let next = log
            .append_at(
                DAY_2,
                actor(),
                Scope::default(),
                "tool.call",
                json!({}),
                "ok",
            )
            .unwrap();
        assert_eq!(next.prev, last_legacy);
        assert_eq!(next.id, "a000000000004");
        drop(log);

        let files = files_in_order(dir.path());
        assert_eq!(files.len(), 2, "{files:?}");
        assert!(files[0].ends_with(LEGACY_FILE));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(LEGACY_FILE))
                .unwrap()
                .lines()
                .count(),
            4,
            "the old file is never written again"
        );
        assert_eq!(verify_dir(dir.path()).unwrap().records, 5);
    }

    #[test]
    fn every_record_appended_lands_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        // Well past the queue's depth: the appends wait, none is lost.
        for i in 0..(QUEUE * 3) {
            log.append(
                actor(),
                Scope::default(),
                "tool.call",
                json!({"i": i}),
                "ok",
            )
            .unwrap();
        }
        log.flush();
        let report = verify_dir(dir.path()).unwrap();
        assert_eq!(report.records, QUEUE * 3);
        assert_eq!(log.write_failures(), 0);
    }

    #[test]
    fn a_line_that_is_not_a_record_is_reported_with_its_place() {
        let dir = tempfile::tempdir().unwrap();
        let log = AuditLog::open(dir.path()).unwrap();
        log.append_at(DAY_1, actor(), Scope::default(), "login", json!({}), "ok")
            .unwrap();
        drop(log);
        let path = day_file(dir.path(), "2026-09-12");
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str("not json\n");
        std::fs::write(&path, text).unwrap();
        match verify_dir(dir.path()).unwrap_err() {
            VerifyError::Malformed { file, line, .. } => {
                assert_eq!(file, path);
                assert_eq!(line, 2);
            }
            other => panic!("{other}"),
        }
    }
}
