//! The database: one redb file under the data directory (deployment §3.4)
//! holding the version DAG and its blobs, the record of every document Core
//! knows by name, and — from Phase A of Pilot 1 on — users, sessions,
//! workspaces and ACLs. A schema version in it says what shape the tables
//! have; Core brings an older file up to date at start, forward only, after
//! a copy has been taken.

use anyhow::{Context, Result};
use localspace_proto as proto;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The file's name under `<data>/db/`.
pub const FILE: &str = "localspace.redb";
/// Its name before 2026-09-12, when it held the DAG alone.
pub const V1_FILE: &str = "dag.redb";
/// The shape of the tables this code writes.
pub const SCHEMA_VERSION: u32 = 2;

/// Small key/value slots about the database itself.
const SETTINGS: TableDefinition<&str, &[u8]> = TableDefinition::new("settings");
/// Document id -> `DocumentRecord`: every document Core knows by name — a
/// harness's, an export, later an upload or a cached page.
const DOCUMENTS: TableDefinition<&str, &[u8]> = TableDefinition::new("documents");

const SCHEMA_KEY: &str = "schema_version";

/// What Core remembers about a document besides its history: enough to list
/// it, to know whose it is, and to serve a file without reading its bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DocumentRecord {
    pub title: String,
    pub kind: proto::DocKind,
    pub mime: String,
    /// A file's size; a harness's document has none to give.
    pub bytes: u64,
    /// blake3 of the bytes a file was created with; empty for a harness's document.
    pub hash: String,
    pub source: proto::DocumentSource,
    pub created_ms: u64,
    /// The workspace it belongs to (deployment §5).
    #[serde(default)]
    pub workspace: String,
    /// The harness whose document this is, for one a harness edits; a file
    /// of its own has none.
    #[serde(default)]
    pub harness: Option<String>,
    #[serde(default)]
    pub created_by: String,
}

#[derive(Clone)]
pub struct Store {
    db: Arc<Database>,
    path: Option<PathBuf>,
}

impl Store {
    /// The database under `data_dir`. A directory holding only the v1 file
    /// gets a copy of it under the new name and opens that, so the v1 file
    /// stays as the copy taken before migrating.
    pub fn open(data_dir: &Path) -> Result<Store> {
        let dir = data_dir.join("db");
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(FILE);
        let v1 = dir.join(V1_FILE);
        if !path.exists() && v1.exists() {
            std::fs::copy(&v1, &path)
                .with_context(|| format!("copying {} to {}", v1.display(), path.display()))?;
        }
        Store::open_file(&path)
    }

    /// A database at exactly this path; `open` is the usual door.
    pub fn open_file(path: &Path) -> Result<Store> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = Database::create(path).with_context(|| format!("opening {}", path.display()))?;
        let store = Store {
            db: Arc::new(db),
            path: Some(path.to_path_buf()),
        };
        store.init()?;
        Ok(store)
    }

    /// In memory, for tests and `--ephemeral` runs.
    pub fn in_memory() -> Result<Store> {
        let db = Database::builder().create_with_backend(redb::backends::InMemoryBackend::new())?;
        let store = Store {
            db: Arc::new(db),
            path: None,
        };
        store.init()?;
        Ok(store)
    }

    fn init(&self) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            txn.open_table(SETTINGS)?;
            txn.open_table(DOCUMENTS)?;
        }
        txn.commit()?;
        Ok(())
    }

    /// The handle the DAG and the other tables share.
    pub fn db(&self) -> Arc<Database> {
        self.db.clone()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The schema the file was last written with: 0 for a fresh file, and
    /// for one from before there was a version.
    pub fn schema_version(&self) -> Result<u32> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(SETTINGS)?;
        Ok(t.get(SCHEMA_KEY)?
            .and_then(|g| g.value().try_into().ok().map(u32::from_le_bytes))
            .unwrap_or(0))
    }

    pub fn set_schema_version(&self, version: u32) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(SETTINGS)?;
            t.insert(SCHEMA_KEY, version.to_le_bytes().as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    // -- documents ----------------------------------------------------------

    /// A new document's id.
    pub fn new_document_id() -> String {
        format!("doc_{}", uuid::Uuid::new_v4().simple())
    }

    pub fn put_document(&self, id: &str, record: &DocumentRecord) -> Result<()> {
        let encoded = serde_json::to_vec(record)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(DOCUMENTS)?;
            t.insert(id, encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOCUMENTS)?;
        match t.get(id)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    /// Every document, by id.
    pub fn documents(&self) -> Result<Vec<(proto::DocId, DocumentRecord)>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(DOCUMENTS)?;
        let mut out = Vec::new();
        for entry in t.iter()? {
            let (k, v) = entry?;
            out.push((k.value().to_string(), serde_json::from_slice(v.value())?));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(title: &str, workspace: &str) -> DocumentRecord {
        DocumentRecord {
            title: title.into(),
            kind: proto::DocKind::Crdt,
            mime: "application/json".into(),
            bytes: 0,
            hash: String::new(),
            source: proto::DocumentSource::Harness {
                harness: "io.localspace.whiteboard".into(),
            },
            created_ms: 1,
            workspace: workspace.into(),
            harness: Some("io.localspace.whiteboard".into()),
            created_by: "tester".into(),
        }
    }

    #[test]
    fn a_fresh_database_has_no_version_until_core_writes_one() {
        let store = Store::in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), 0);
        store.set_schema_version(SCHEMA_VERSION).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn documents_round_trip_and_an_old_record_reads_with_defaults() {
        let store = Store::in_memory().unwrap();
        let id = Store::new_document_id();
        assert!(id.starts_with("doc_") && id.len() == 36, "{id}");
        store
            .put_document(&id, &record("Board", "ws_tester"))
            .unwrap();
        assert_eq!(store.get_document(&id).unwrap().unwrap().title, "Board");
        assert_eq!(store.documents().unwrap().len(), 1);
        assert!(store.get_document("doc_nothing").unwrap().is_none());

        // A record written by 6.0, before workspace, harness and creator.
        let old = serde_json::json!({
            "title": "x.png", "kind": "blob", "mime": "image/png", "bytes": 3, "hash": "h",
            "source": {"export": {"harness": "io.localspace.whiteboard", "document": "d", "commit": "c"}},
            "created_ms": 5
        });
        let parsed: DocumentRecord = serde_json::from_value(old).unwrap();
        assert_eq!(parsed.workspace, "");
        assert_eq!(parsed.harness, None);
        assert_eq!(parsed.created_by, "");
    }

    #[test]
    fn a_directory_with_only_the_v1_file_gets_a_copy_under_the_new_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        {
            let v1 = Store::open_file(&db.join(V1_FILE)).unwrap();
            v1.put_document("legacy", &record("Old board", "")).unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.path(), Some(db.join(FILE).as_path()));
        assert!(
            db.join(V1_FILE).exists(),
            "the copy taken before migrating stays"
        );
        assert_eq!(
            store.get_document("legacy").unwrap().unwrap().title,
            "Old board"
        );
        assert_eq!(
            store.schema_version().unwrap(),
            0,
            "Core migrates and stamps the version"
        );
    }
}
