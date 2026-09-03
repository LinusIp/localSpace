//! Append-only, hash-chained audit log (deployment spec §10.1).
//!
//! Each record carries the blake3 of the previous one, so a deleted or edited
//! record breaks the chain and `verify` finds it. Records are written from a
//! bounded channel by a separate task in the server; the hot path never waits on
//! the SIEM (§16.4).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const GENESIS: &str = "genesis";

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

pub struct AuditLog {
    path: Option<PathBuf>,
    state: Mutex<State>,
}

struct State {
    prev: String,
    seq: u64,
    /// Kept in memory too, so `verify` works for an in-memory log and tests.
    records: Vec<Record>,
}

impl AuditLog {
    pub fn in_memory() -> AuditLog {
        AuditLog {
            path: None,
            state: Mutex::new(State {
                prev: GENESIS.into(),
                seq: 0,
                records: Vec::new(),
            }),
        }
    }

    /// Open (or create) a log file, continuing its chain.
    pub fn open(path: &Path) -> Result<AuditLog> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let records = if path.exists() {
            read_records(path)?
        } else {
            Vec::new()
        };
        let prev = records
            .last()
            .map(|r| r.hash())
            .unwrap_or_else(|| GENESIS.to_string());
        let seq = records.len() as u64;
        Ok(AuditLog {
            path: Some(path.to_path_buf()),
            state: Mutex::new(State { prev, seq, records }),
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
        let mut st = self.state.lock().unwrap();
        let record = Record {
            ts_ms: crate::dag::now_ms(),
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

        if let Some(path) = &self.path {
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?;
            writeln!(f, "{}", serde_json::to_string(&record)?)?;
        }
        st.records.push(record.clone());
        Ok(record)
    }

    pub fn len(&self) -> usize {
        self.state.lock().unwrap().records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn records(&self) -> Vec<Record> {
        self.state.lock().unwrap().records.clone()
    }

    /// Walk the chain. Returns the index of the first broken link.
    pub fn verify(&self) -> std::result::Result<usize, VerifyError> {
        verify_chain(&self.state.lock().unwrap().records)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("record {index} ({id}) expected prev {expected}, found {found}")]
    Broken {
        index: usize,
        id: String,
        expected: String,
        found: String,
    },
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
            });
        }
        prev = r.hash();
    }
    Ok(records.len())
}

pub fn read_records(path: &Path) -> Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn actor() -> Actor {
        Actor {
            user: "u_123".into(),
            session: "s_9ab".into(),
            ip: "10.1.4.22".into(),
            role: "member".into(),
        }
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
            VerifyError::Broken { index, .. } => assert_eq!(index, 3, "the break shows at the next link"),
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
    fn a_log_reopened_from_disk_continues_its_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let a = AuditLog::open(&path).unwrap();
        a.append(actor(), Scope::default(), "login", json!({}), "ok")
            .unwrap();
        a.append(actor(), Scope::default(), "tool.call", json!({}), "ok")
            .unwrap();
        drop(a);

        let b = AuditLog::open(&path).unwrap();
        b.append(actor(), Scope::default(), "logout", json!({}), "ok")
            .unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b.verify().unwrap(), 3);

        // And on disk.
        assert_eq!(verify_chain(&read_records(&path).unwrap()).unwrap(), 3);
    }
}
