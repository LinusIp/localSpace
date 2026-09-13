//! The database: one redb file under the data directory (deployment §3.4)
//! holding the version DAG and its blobs, the record of every document Core
//! knows by name, and — from Phase A of Pilot 1 on — users, sessions,
//! workspaces and ACLs. A schema version in it says what shape the tables
//! have; Core brings an older file up to date at start, forward only, after
//! a copy has been taken.

use crate::conversations::Conversation;
use anyhow::{Context, Result};
use localspace_proto as proto;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The file's name under `<data>/db/`.
pub const FILE: &str = "localspace.redb";
/// Its name before 2026-09-12, when it held the DAG alone.
pub const V1_FILE: &str = "dag.redb";
/// The shape of the tables this code writes: 2 gave documents their records
/// and ids; 3 moved conversations and the ledger in, per user and workspace.
pub const SCHEMA_VERSION: u32 = 3;

/// Small key/value slots about the database itself.
const SETTINGS: TableDefinition<&str, &[u8]> = TableDefinition::new("settings");
/// Document id -> `DocumentRecord`: every document Core knows by name — a
/// harness's, an export, later an upload or a cached page.
const DOCUMENTS: TableDefinition<&str, &[u8]> = TableDefinition::new("documents");
/// `user ␟ workspace ␟ id` -> `Conversation` (deployment §5: a conversation
/// runs inside one workspace and is the user's own).
const CONVERSATIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("conversations");
/// `user ␟ workspace ␟ what` -> a small per-user, per-workspace value: the
/// current conversation, the agent's ledger.
const USER_STATE: TableDefinition<&str, &[u8]> = TableDefinition::new("user_state");
/// User id -> `UserRecord` (deployment §4; Pilot 1, Phase A).
const USERS: TableDefinition<&str, &[u8]> = TableDefinition::new("users");
/// Lower-cased email -> user id.
const USER_EMAILS: TableDefinition<&str, &str> = TableDefinition::new("user_emails");
/// blake3 of a session id -> `SessionRecord`. The id itself is only ever in
/// the cookie.
const SESSIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("sessions");
/// blake3 of a one-time token -> `InviteRecord`.
const INVITES: TableDefinition<&str, &[u8]> = TableDefinition::new("invites");
/// `account:<user id>` or `ip:<address>` -> `LockRecord`: failed logins and
/// the lock they earned, kept across a restart.
const LOCKOUTS: TableDefinition<&str, &[u8]> = TableDefinition::new("lockouts");
/// Workspace id -> `acl::Workspace` (deployment §5): its members and their
/// levels, and how agents write there.
const WORKSPACES: TableDefinition<&str, &[u8]> = TableDefinition::new("workspaces");

const SCHEMA_KEY: &str = "schema_version";
/// Between the parts of a scoped key: a character no id contains.
const SEP: char = '\u{1f}';

fn scoped(user: &str, workspace: &str, tail: &str) -> String {
    format!("{user}{SEP}{workspace}{SEP}{tail}")
}

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
    /// Set when the document was tightened below its workspace (deployment
    /// §6.1); otherwise it follows its workspace's members.
    #[serde(default)]
    pub acl: Option<crate::acl::Acl>,
}

/// A user of an organisation server (deployment §4.1–4.3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UserRecord {
    pub id: String,
    /// Lower-cased; the login name.
    pub email: String,
    pub name: String,
    pub roles: Vec<proto::UserRole>,
    /// `local` for an admin-created account, `oidc` once Phase C binds one.
    pub provider: String,
    /// The password's PHC string (argon2id), once the user has set one.
    pub password_hash: Option<String>,
    pub disabled: bool,
    pub created_ms: u64,
    pub last_login_ms: Option<u64>,
    pub password_set_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionRecord {
    pub user: String,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub ip: String,
    pub user_agent: String,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InviteRecord {
    /// The account the link is for; empty for the first administrator's,
    /// whose account is made when the link is used.
    pub user: String,
    pub created_ms: u64,
    pub expires_ms: u64,
    pub used_ms: Option<u64>,
    /// The first administrator's link (the fourth answer of 2026-09-13).
    #[serde(default)]
    pub first_admin: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LockRecord {
    /// Failures counted so far: consecutive for an account, within the
    /// window for an address.
    pub failures: u32,
    pub window_start_ms: u64,
    pub locked_until_ms: u64,
    /// Locks earned in a row; each doubles an account's lock.
    pub rounds: u32,
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
            // The copy carries the original's attributes; a read-only v1 file
            // must not leave the copy unwritable.
            let mut writable = std::fs::metadata(&path)
                .with_context(|| format!("reading {}", path.display()))?
                .permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            writable.set_readonly(false);
            std::fs::set_permissions(&path, writable)
                .with_context(|| format!("making {} writable", path.display()))?;
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
            txn.open_table(CONVERSATIONS)?;
            txn.open_table(USER_STATE)?;
            txn.open_table(USERS)?;
            txn.open_table(USER_EMAILS)?;
            txn.open_table(SESSIONS)?;
            txn.open_table(INVITES)?;
            txn.open_table(LOCKOUTS)?;
            txn.open_table(WORKSPACES)?;
        }
        txn.commit()?;
        Ok(())
    }

    // -- workspaces (deployment §5) -------------------------------------------

    pub fn put_workspace(&self, workspace: &crate::acl::Workspace) -> Result<()> {
        let encoded = serde_json::to_vec(workspace)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(WORKSPACES)?;
            t.insert(workspace.id.as_str(), encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_workspace(&self, id: &str) -> Result<Option<crate::acl::Workspace>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(WORKSPACES)?;
        match t.get(id)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    pub fn workspaces(&self) -> Result<Vec<crate::acl::Workspace>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(WORKSPACES)?;
        let mut out = Vec::new();
        for entry in t.iter()? {
            let (_, v) = entry?;
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    /// The workspace a user was last in, remembered across a restart.
    pub fn current_workspace(&self, user: &str) -> Result<Option<String>> {
        let key = scoped(user, "", "workspace");
        let txn = self.db.begin_read()?;
        let t = txn.open_table(USER_STATE)?;
        Ok(t.get(key.as_str())?
            .map(|g| String::from_utf8_lossy(g.value()).to_string()))
    }

    pub fn set_current_workspace(&self, user: &str, workspace: &str) -> Result<()> {
        let key = scoped(user, "", "workspace");
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(USER_STATE)?;
            t.insert(key.as_str(), workspace.as_bytes())?;
        }
        txn.commit()?;
        Ok(())
    }

    // -- users, sessions, invites, lockouts (deployment §4) -----------------

    pub fn put_user(&self, user: &UserRecord) -> Result<()> {
        let encoded = serde_json::to_vec(user)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(USERS)?;
            t.insert(user.id.as_str(), encoded.as_slice())?;
            let mut e = txn.open_table(USER_EMAILS)?;
            e.insert(user.email.as_str(), user.id.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_user(&self, id: &str) -> Result<Option<UserRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(USERS)?;
        match t.get(id)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    pub fn user_by_email(&self, email: &str) -> Result<Option<UserRecord>> {
        let id = {
            let txn = self.db.begin_read()?;
            let e = txn.open_table(USER_EMAILS)?;
            e.get(email)?.map(|g| g.value().to_string())
        };
        match id {
            Some(id) => self.get_user(&id),
            None => Ok(None),
        }
    }

    /// Every user, by id.
    pub fn users(&self) -> Result<Vec<UserRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(USERS)?;
        let mut out = Vec::new();
        for entry in t.iter()? {
            let (_, v) = entry?;
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    pub fn put_session(&self, key: &str, session: &SessionRecord) -> Result<()> {
        let encoded = serde_json::to_vec(session)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(SESSIONS)?;
            t.insert(key, encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_session(&self, key: &str) -> Result<Option<SessionRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(SESSIONS)?;
        match t.get(key)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    /// Every session of a user, keyed as stored.
    pub fn sessions_of(&self, user: &str) -> Result<Vec<(String, SessionRecord)>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(SESSIONS)?;
        let mut out = Vec::new();
        for entry in t.iter()? {
            let (k, v) = entry?;
            let record: SessionRecord = serde_json::from_slice(v.value())?;
            if record.user == user {
                out.push((k.value().to_string(), record));
            }
        }
        Ok(out)
    }

    pub fn put_invite(&self, key: &str, invite: &InviteRecord) -> Result<()> {
        let encoded = serde_json::to_vec(invite)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(INVITES)?;
            t.insert(key, encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn get_invite(&self, key: &str) -> Result<Option<InviteRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(INVITES)?;
        match t.get(key)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    /// Every one-time link, by its key.
    pub fn invites(&self) -> Result<Vec<(String, InviteRecord)>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(INVITES)?;
        let mut out = Vec::new();
        for entry in t.iter()? {
            let (k, v) = entry?;
            out.push((k.value().to_string(), serde_json::from_slice(v.value())?));
        }
        Ok(out)
    }

    pub fn get_lock(&self, key: &str) -> Result<Option<LockRecord>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(LOCKOUTS)?;
        match t.get(key)? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    pub fn put_lock(&self, key: &str, lock: &LockRecord) -> Result<()> {
        let encoded = serde_json::to_vec(lock)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(LOCKOUTS)?;
            t.insert(key, encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn clear_lock(&self, key: &str) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(LOCKOUTS)?;
            t.remove(key)?;
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

    // -- conversations, per user and workspace ------------------------------

    pub fn conversations(&self, user: &str, workspace: &str) -> Result<Vec<Conversation>> {
        let prefix = scoped(user, workspace, "");
        let txn = self.db.begin_read()?;
        let t = txn.open_table(CONVERSATIONS)?;
        let mut out = Vec::new();
        for entry in t.range::<&str>(prefix.as_str()..)? {
            let (k, v) = entry?;
            if !k.value().starts_with(&prefix) {
                break;
            }
            out.push(serde_json::from_slice(v.value())?);
        }
        Ok(out)
    }

    pub fn put_conversation(&self, user: &str, workspace: &str, c: &Conversation) -> Result<()> {
        let encoded = serde_json::to_vec(c)?;
        let key = scoped(user, workspace, &c.id);
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(CONVERSATIONS)?;
            t.insert(key.as_str(), encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
    }

    pub fn delete_conversation(&self, user: &str, workspace: &str, id: &str) -> Result<bool> {
        let key = scoped(user, workspace, id);
        let txn = self.db.begin_write()?;
        let removed = {
            let mut t = txn.open_table(CONVERSATIONS)?;
            t.remove(key.as_str())?.is_some()
        };
        txn.commit()?;
        Ok(removed)
    }

    pub fn current_conversation(&self, user: &str, workspace: &str) -> Result<Option<String>> {
        let key = scoped(user, workspace, "current");
        let txn = self.db.begin_read()?;
        let t = txn.open_table(USER_STATE)?;
        Ok(t.get(key.as_str())?
            .map(|g| String::from_utf8_lossy(g.value()).to_string()))
    }

    pub fn set_current_conversation(&self, user: &str, workspace: &str, id: &str) -> Result<()> {
        let key = scoped(user, workspace, "current");
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(USER_STATE)?;
            t.insert(key.as_str(), id.as_bytes())?;
        }
        txn.commit()?;
        Ok(())
    }

    // -- the agent's ledger, per user and workspace (plugin spec §18.1) -------

    pub fn ledger(&self, user: &str, workspace: &str) -> Result<Option<proto::Task>> {
        let key = scoped(user, workspace, "ledger");
        let txn = self.db.begin_read()?;
        let t = txn.open_table(USER_STATE)?;
        match t.get(key.as_str())? {
            Some(g) => Ok(Some(serde_json::from_slice(g.value())?)),
            None => Ok(None),
        }
    }

    pub fn put_ledger(&self, user: &str, workspace: &str, task: &proto::Task) -> Result<()> {
        let encoded = serde_json::to_vec(task)?;
        let key = scoped(user, workspace, "ledger");
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(USER_STATE)?;
            t.insert(key.as_str(), encoded.as_slice())?;
        }
        txn.commit()?;
        Ok(())
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
            acl: None,
        }
    }

    #[test]
    fn conversations_and_the_ledger_are_kept_per_user_and_workspace() {
        let store = Store::in_memory().unwrap();
        let c = |id: &str, title: &str| Conversation {
            id: id.into(),
            title: title.into(),
            created_ms: 1,
            updated_ms: 2,
            messages: Vec::new(),
        };
        store
            .put_conversation("anna", "ws_anna", &c("c_1", "one"))
            .unwrap();
        store
            .put_conversation("anna", "ws_anna", &c("c_2", "two"))
            .unwrap();
        store
            .put_conversation("anna", "ws_team", &c("c_1", "team"))
            .unwrap();
        store
            .put_conversation("ben", "ws_ben", &c("c_1", "ben"))
            .unwrap();
        let anna: Vec<String> = store
            .conversations("anna", "ws_anna")
            .unwrap()
            .into_iter()
            .map(|c| c.title)
            .collect();
        assert_eq!(anna, vec!["one", "two"]);
        assert_eq!(
            store.conversations("anna", "ws_team").unwrap()[0].title,
            "team"
        );
        assert_eq!(store.conversations("ben", "ws_ben").unwrap().len(), 1);
        assert!(store.conversations("carla", "ws_carla").unwrap().is_empty());

        assert!(store.delete_conversation("anna", "ws_anna", "c_1").unwrap());
        assert!(!store.delete_conversation("anna", "ws_anna", "c_1").unwrap());
        assert_eq!(store.conversations("anna", "ws_anna").unwrap().len(), 1);

        assert_eq!(store.current_conversation("anna", "ws_anna").unwrap(), None);
        store
            .set_current_conversation("anna", "ws_anna", "c_2")
            .unwrap();
        assert_eq!(
            store
                .current_conversation("anna", "ws_anna")
                .unwrap()
                .as_deref(),
            Some("c_2")
        );

        assert!(store.ledger("anna", "ws_anna").unwrap().is_none());
        let task = proto::Task {
            id: "t_1".into(),
            goal: "plan the launch".into(),
            ..Default::default()
        };
        store.put_ledger("anna", "ws_anna", &task).unwrap();
        assert_eq!(
            store.ledger("anna", "ws_anna").unwrap().unwrap().goal,
            "plan the launch"
        );
        assert!(store.ledger("anna", "ws_team").unwrap().is_none());
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
