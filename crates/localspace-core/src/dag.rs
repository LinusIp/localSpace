//! The version DAG and the content-addressed blob store.
//!
//! Every agent-initiated write is a commit. Undo is Core-level and uniform:
//! DAG revert, regardless of harness. An agent run is one branch, so
//! "reject the agent's changes" is a branch drop, not 40 undos.

use anyhow::{bail, Context, Result};
use localspace_proto as proto;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::Path;
use std::sync::Arc;

const COMMITS: TableDefinition<&str, &[u8]> = TableDefinition::new("commits");
/// Append order -> commit id. Gives history a stable ordering.
const ORDER: TableDefinition<u64, &str> = TableDefinition::new("order");
/// blake3 hash -> document bytes at that commit.
const BLOBS: TableDefinition<&str, &[u8]> = TableDefinition::new("blobs");
/// Small key/value slots: head pointer per document, redo stack.
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

pub struct Dag {
    db: Arc<Database>,
}

/// What a revert asks the caller to restore.
#[derive(Debug, Clone)]
pub struct Revert {
    pub doc: proto::DocId,
    /// Document bytes to restore. `None` means "before any commit" — an empty doc.
    pub bytes: Option<Vec<u8>>,
    pub undone: Vec<proto::CommitId>,
    pub summary: String,
}

impl Dag {
    pub fn open(path: &Path) -> Result<Dag> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = Database::create(path).with_context(|| format!("opening {}", path.display()))?;
        let dag = Dag { db: Arc::new(db) };
        dag.init_tables()?;
        Ok(dag)
    }

    /// In-memory DAG, used by tests and by `--ephemeral` runs.
    pub fn in_memory() -> Result<Dag> {
        let db = Database::builder().create_with_backend(redb::backends::InMemoryBackend::new())?;
        let dag = Dag { db: Arc::new(db) };
        dag.init_tables()?;
        Ok(dag)
    }

    fn init_tables(&self) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            txn.open_table(COMMITS)?;
            txn.open_table(ORDER)?;
            txn.open_table(BLOBS)?;
            txn.open_table(META)?;
        }
        txn.commit()?;
        Ok(())
    }

    // -- blobs --------------------------------------------------------------

    pub fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        let hash = blake3::hash(bytes).to_hex().to_string();
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(BLOBS)?;
            t.insert(hash.as_str(), bytes)?;
        }
        txn.commit()?;
        Ok(hash)
    }

    pub fn get_blob(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(BLOBS)?;
        Ok(t.get(hash)?.map(|g| g.value().to_vec()))
    }

    // -- commits ------------------------------------------------------------

    /// Record a mutation. Returns the commit id.
    #[allow(clippy::too_many_arguments)]
    pub fn commit(
        &self,
        doc: &str,
        harness: &str,
        tool: &str,
        params: proto::Json,
        doc_bytes: &[u8],
        diff_summary: &str,
        author: proto::Author,
        run: Option<String>,
    ) -> Result<proto::Commit> {
        let doc_hash = self.put_blob(doc_bytes)?;
        let parent = self.head(doc)?;
        let id = format!("c{}", &blake3::hash(
            format!("{doc}{tool}{doc_hash}{:?}{}", parent, now_ms()).as_bytes()
        ).to_hex()[..12]);

        let commit = proto::Commit {
            id: id.clone(),
            parent,
            doc: doc.to_string(),
            harness: harness.to_string(),
            tool: tool.to_string(),
            params,
            doc_hash,
            diff_summary: diff_summary.to_string(),
            author,
            at_ms: now_ms(),
            run,
        };

        let encoded = serde_json::to_vec(&commit)?;
        let txn = self.db.begin_write()?;
        {
            let mut c = txn.open_table(COMMITS)?;
            c.insert(commit.id.as_str(), encoded.as_slice())?;

            let mut o = txn.open_table(ORDER)?;
            let next = o.last()?.map(|(k, _)| k.value() + 1).unwrap_or(0);
            o.insert(next, commit.id.as_str())?;

            let mut m = txn.open_table(META)?;
            m.insert(head_key(doc).as_str(), commit.id.as_bytes())?;
            // A new commit invalidates the redo stack for this document.
            m.remove(redo_key(doc).as_str())?;
        }
        txn.commit()?;
        Ok(commit)
    }

    pub fn head(&self, doc: &str) -> Result<Option<proto::CommitId>> {
        let txn = self.db.begin_read()?;
        let m = txn.open_table(META)?;
        Ok(m.get(head_key(doc).as_str())?
            .map(|g| String::from_utf8_lossy(g.value()).to_string()))
    }

    pub fn get_commit(&self, id: &str) -> Result<Option<proto::Commit>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(COMMITS)?;
        match t.get(id)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    /// Newest first.
    pub fn history(&self, limit: usize) -> Result<Vec<proto::Commit>> {
        let txn = self.db.begin_read()?;
        let o = txn.open_table(ORDER)?;
        let c = txn.open_table(COMMITS)?;
        let mut out = Vec::new();
        for entry in o.range::<u64>(..)?.rev() {
            let (_, id) = entry?;
            if let Some(g) = c.get(id.value())? {
                out.push(serde_json::from_slice::<proto::Commit>(g.value())?);
            }
            if out.len() >= limit {
                break;
            }
        }
        Ok(out)
    }

    /// The document that most recently received a commit — what Undo acts on.
    pub fn last_touched_doc(&self) -> Result<Option<proto::DocId>> {
        Ok(self.history(1)?.into_iter().next().map(|c| c.doc))
    }

    // -- undo / redo / branch drop ------------------------------------------

    /// Move `doc`'s head to its parent and hand back the bytes to restore.
    pub fn undo(&self, doc: &str) -> Result<Revert> {
        let Some(head_id) = self.head(doc)? else {
            bail!("nothing to undo");
        };
        let head = self
            .get_commit(&head_id)?
            .with_context(|| format!("commit {head_id} missing from the DAG"))?;

        let bytes = match &head.parent {
            Some(p) => {
                let parent = self
                    .get_commit(p)?
                    .with_context(|| format!("parent {p} missing from the DAG"))?;
                self.get_blob(&parent.doc_hash)?
            }
            None => None,
        };

        let txn = self.db.begin_write()?;
        {
            let mut m = txn.open_table(META)?;
            match &head.parent {
                Some(p) => {
                    m.insert(head_key(doc).as_str(), p.as_bytes())?;
                }
                None => {
                    m.remove(head_key(doc).as_str())?;
                }
            }
            // Push onto the redo stack.
            let mut redo: Vec<String> = m
                .get(redo_key(doc).as_str())?
                .map(|g| serde_json::from_slice(g.value()).unwrap_or_default())
                .unwrap_or_default();
            redo.push(head_id.clone());
            let encoded = serde_json::to_vec(&redo)?;
            m.insert(redo_key(doc).as_str(), encoded.as_slice())?;
        }
        txn.commit()?;

        Ok(Revert {
            doc: doc.to_string(),
            bytes,
            undone: vec![head_id],
            summary: format!("undid: {}", head.diff_summary),
        })
    }

    pub fn redo(&self, doc: &str) -> Result<Revert> {
        let txn = self.db.begin_write()?;
        let restored;
        {
            let mut m = txn.open_table(META)?;
            let mut redo: Vec<String> = m
                .get(redo_key(doc).as_str())?
                .map(|g| serde_json::from_slice(g.value()).unwrap_or_default())
                .unwrap_or_default();
            let Some(id) = redo.pop() else {
                drop(m);
                txn.abort()?;
                bail!("nothing to redo");
            };
            m.insert(head_key(doc).as_str(), id.as_bytes())?;
            let encoded = serde_json::to_vec(&redo)?;
            m.insert(redo_key(doc).as_str(), encoded.as_slice())?;
            restored = id;
        }
        txn.commit()?;

        let commit = self
            .get_commit(&restored)?
            .with_context(|| format!("commit {restored} missing from the DAG"))?;
        Ok(Revert {
            doc: doc.to_string(),
            bytes: self.get_blob(&commit.doc_hash)?,
            undone: vec![restored],
            summary: format!("redid: {}", commit.diff_summary),
        })
    }

    /// Drop an entire agent run: restore each document it touched to the state
    /// it had before the run's first commit. One action, not N undos.
    pub fn drop_run(&self, run: &str) -> Result<Vec<Revert>> {
        let all = self.history(usize::MAX)?; // newest first
        let in_run: Vec<&proto::Commit> = all
            .iter()
            .filter(|c| c.run.as_deref() == Some(run))
            .collect();
        if in_run.is_empty() {
            bail!("no commits belong to run `{run}`");
        }

        let mut docs: Vec<String> = in_run.iter().map(|c| c.doc.clone()).collect();
        docs.sort();
        docs.dedup();

        let mut reverts = Vec::new();
        for doc in docs {
            // Oldest commit of this run within this document — by position in
            // the append order, never by timestamp: a fast machine lands many
            // commits inside one millisecond, and the wrong "first" restores the
            // wrong parent. `in_run` is newest-first, so the oldest is at the end.
            let first = in_run
                .iter()
                .rev()
                .find(|c| c.doc == doc)
                .expect("doc came from in_run");
            let bytes = match &first.parent {
                Some(p) => match self.get_commit(p)? {
                    Some(parent) => self.get_blob(&parent.doc_hash)?,
                    None => None,
                },
                None => None,
            };
            let undone: Vec<String> = in_run
                .iter()
                .filter(|c| c.doc == doc)
                .map(|c| c.id.clone())
                .collect();

            let txn = self.db.begin_write()?;
            {
                let mut m = txn.open_table(META)?;
                match &first.parent {
                    Some(p) => {
                        m.insert(head_key(&doc).as_str(), p.as_bytes())?;
                    }
                    None => {
                        m.remove(head_key(&doc).as_str())?;
                    }
                }
                m.remove(redo_key(&doc).as_str())?;
            }
            txn.commit()?;

            reverts.push(Revert {
                doc: doc.clone(),
                bytes,
                summary: format!("dropped run {run}: {} commits", undone.len()),
                undone,
            });
        }
        Ok(reverts)
    }
}

fn head_key(doc: &str) -> String {
    format!("head:{doc}")
}

fn redo_key(doc: &str) -> String {
    format!("redo:{doc}")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dag: &Dag, doc: &str, body: &str, run: Option<&str>) -> proto::Commit {
        dag.commit(
            doc,
            "io.localspace.whiteboard",
            "canvas.add_shape",
            proto::Json(serde_json::json!({"kind": "rect"})),
            body.as_bytes(),
            "added 1 shape",
            proto::Author::Agent,
            run.map(|r| r.to_string()),
        )
        .unwrap()
    }

    #[test]
    fn commits_chain_and_history_is_newest_first() {
        let dag = Dag::in_memory().unwrap();
        let a = write(&dag, "board", "state-a", None);
        let b = write(&dag, "board", "state-b", None);
        assert_eq!(b.parent.as_deref(), Some(a.id.as_str()));
        assert_eq!(dag.head("board").unwrap().as_deref(), Some(b.id.as_str()));

        let hist = dag.history(10).unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].id, b.id);
    }

    #[test]
    fn undo_restores_the_parent_state_and_redo_puts_it_back() {
        let dag = Dag::in_memory().unwrap();
        write(&dag, "board", "state-a", None);
        let b = write(&dag, "board", "state-b", None);

        let rev = dag.undo("board").unwrap();
        assert_eq!(rev.bytes.as_deref(), Some(b"state-a".as_slice()));
        assert_ne!(dag.head("board").unwrap().as_deref(), Some(b.id.as_str()));

        let redo = dag.redo("board").unwrap();
        assert_eq!(redo.bytes.as_deref(), Some(b"state-b".as_slice()));
        assert_eq!(dag.head("board").unwrap().as_deref(), Some(b.id.as_str()));
    }

    #[test]
    fn undo_past_the_first_commit_yields_an_empty_document() {
        let dag = Dag::in_memory().unwrap();
        write(&dag, "board", "state-a", None);
        let rev = dag.undo("board").unwrap();
        assert!(rev.bytes.is_none());
        assert!(dag.head("board").unwrap().is_none());
        assert!(dag.undo("board").is_err());
    }

    #[test]
    fn dropping_a_run_is_one_action_not_forty_undos() {
        let dag = Dag::in_memory().unwrap();
        write(&dag, "board", "before-run", None);
        for i in 0..40 {
            write(&dag, "board", &format!("run-state-{i}"), Some("run-1"));
        }
        assert_eq!(dag.history(100).unwrap().len(), 41);

        let reverts = dag.drop_run("run-1").unwrap();
        assert_eq!(reverts.len(), 1);
        assert_eq!(reverts[0].bytes.as_deref(), Some(b"before-run".as_slice()));
        assert_eq!(reverts[0].undone.len(), 40);
    }

    #[test]
    fn a_run_spanning_two_documents_reverts_both() {
        let dag = Dag::in_memory().unwrap();
        write(&dag, "board", "board-before", None);
        write(&dag, "notes", "notes-before", None);
        write(&dag, "board", "board-during", Some("r2"));
        write(&dag, "notes", "notes-during", Some("r2"));

        let mut reverts = dag.drop_run("r2").unwrap();
        reverts.sort_by(|a, b| a.doc.cmp(&b.doc));
        assert_eq!(reverts.len(), 2);
        assert_eq!(reverts[0].bytes.as_deref(), Some(b"board-before".as_slice()));
        assert_eq!(reverts[1].bytes.as_deref(), Some(b"notes-before".as_slice()));
    }

    #[test]
    fn a_new_commit_clears_the_redo_stack() {
        let dag = Dag::in_memory().unwrap();
        write(&dag, "board", "a", None);
        write(&dag, "board", "b", None);
        dag.undo("board").unwrap();
        write(&dag, "board", "c", None);
        assert!(dag.redo("board").is_err());
    }

    #[test]
    fn blobs_are_content_addressed() {
        let dag = Dag::in_memory().unwrap();
        let h1 = dag.put_blob(b"same").unwrap();
        let h2 = dag.put_blob(b"same").unwrap();
        assert_eq!(h1, h2);
        assert_eq!(dag.get_blob(&h1).unwrap().as_deref(), Some(b"same".as_slice()));
    }
}
