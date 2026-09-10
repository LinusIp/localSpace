//! Document storage. Core holds the authoritative document; harness logic sees a
//! JSON projection of it and hands back a new one.
//!
//! `doc = "crdt"` documents live in Automerge, so undo, granular diffs and replica
//! sync to a surface across a network all come for free. Core reconciles the JSON a
//! harness returns *into* the CRDT field by field rather than replacing the root, so
//! a two-word text edit stays a two-word change in the history and over the wire.
//!
//! `doc = "blob"` documents are opaque content-addressed bytes, snapshotted before
//! each write tool.

use anyhow::{Context, Result};
use automerge::transaction::Transactable;
use automerge::{AutoCommit, AutoSerde, ObjId, ObjType, Prop, ReadDoc, ScalarValue, Value};
use localspace_proto as proto;
use serde_json::{Map, Value as J};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
}

impl Changes {
    pub fn is_empty(self) -> bool {
        self.added == 0 && self.removed == 0 && self.changed == 0
    }

    /// The one-line delta the model sees in a tool result.
    pub fn summary(self) -> String {
        if self.is_empty() {
            return "no change".into();
        }
        let mut parts = Vec::new();
        if self.added > 0 {
            parts.push(format!("added {}", self.added));
        }
        if self.changed > 0 {
            parts.push(format!("changed {}", self.changed));
        }
        if self.removed > 0 {
            parts.push(format!("removed {}", self.removed));
        }
        format!("{} field(s)", parts.join(", "))
    }

    fn merge(&mut self, other: Changes) {
        self.added += other.added;
        self.removed += other.removed;
        self.changed += other.changed;
    }
}

pub enum Doc {
    Crdt(Box<AutoCommit>),
    Blob(Vec<u8>),
}

pub struct DocStore {
    docs: HashMap<proto::DocId, Doc>,
    kinds: HashMap<proto::DocId, proto::DocKind>,
}

impl Default for DocStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DocStore {
    pub fn new() -> Self {
        DocStore {
            docs: HashMap::new(),
            kinds: HashMap::new(),
        }
    }

    pub fn ensure(&mut self, doc: &str, kind: proto::DocKind) {
        if !self.docs.contains_key(doc) {
            let d = match kind {
                proto::DocKind::Crdt => Doc::Crdt(Box::new(AutoCommit::new())),
                proto::DocKind::Blob => Doc::Blob(Vec::new()),
            };
            self.docs.insert(doc.to_string(), d);
            self.kinds.insert(doc.to_string(), kind);
        }
    }

    pub fn kind(&self, doc: &str) -> Option<proto::DocKind> {
        self.kinds.get(doc).copied()
    }

    pub fn exists(&self, doc: &str) -> bool {
        self.docs.contains_key(doc)
    }

    /// JSON projection handed to harness logic and to surfaces.
    pub fn json(&mut self, doc: &str) -> Result<J> {
        match self.docs.get_mut(doc) {
            Some(Doc::Crdt(am)) => {
                Ok(serde_json::to_value(AutoSerde::from(am.as_ref())).unwrap_or(J::Null))
            }
            Some(Doc::Blob(bytes)) => Ok(serde_json::json!({
                "blob": true,
                "bytes": bytes.len(),
                "hash": blake3::hash(bytes).to_hex().to_string(),
            })),
            None => Ok(J::Null),
        }
    }

    /// Reconcile a whole-document JSON value into the CRDT, field by field.
    pub fn apply_json(&mut self, doc: &str, next: &J) -> Result<Changes> {
        let entry = self
            .docs
            .get_mut(doc)
            .with_context(|| format!("unknown document `{doc}`"))?;
        match entry {
            Doc::Crdt(am) => {
                let obj = next
                    .as_object()
                    .context("a crdt document's root must be a JSON object")?;
                let changes = reconcile_map(am.as_mut(), &ObjId::Root, obj)?;
                Ok(changes)
            }
            Doc::Blob(bytes) => {
                let encoded = serde_json::to_vec(next)?;
                let changed = *bytes != encoded;
                *bytes = encoded;
                Ok(Changes {
                    changed: usize::from(changed),
                    ..Default::default()
                })
            }
        }
    }

    pub fn put_blob(&mut self, doc: &str, bytes: Vec<u8>) -> Result<Changes> {
        match self.docs.get_mut(doc) {
            Some(Doc::Blob(b)) => {
                let changed = *b != bytes;
                *b = bytes;
                Ok(Changes {
                    changed: usize::from(changed),
                    ..Default::default()
                })
            }
            _ => anyhow::bail!("`{doc}` is not a blob document"),
        }
    }

    /// The JSON projection of a saved snapshot, without touching the live
    /// document. This is how an artifact pinned to a commit is read back: the
    /// handoff pins an exact version, and the consumer sees that version even
    /// if the producer's board has moved on since.
    pub fn json_of_snapshot(kind: proto::DocKind, bytes: &[u8]) -> Result<J> {
        match kind {
            proto::DocKind::Crdt => {
                if bytes.is_empty() {
                    return Ok(J::Object(Default::default()));
                }
                let am = AutoCommit::load(bytes).context("loading a crdt snapshot")?;
                Ok(serde_json::to_value(AutoSerde::from(&am)).unwrap_or(J::Null))
            }
            proto::DocKind::Blob => Ok(serde_json::json!({
                "blob": true,
                "bytes": bytes.len(),
                "hash": blake3::hash(bytes).to_hex().to_string(),
            })),
        }
    }

    /// Bytes for the DAG snapshot: the Automerge save form, or the blob itself.
    pub fn snapshot(&mut self, doc: &str) -> Result<Vec<u8>> {
        match self.docs.get_mut(doc) {
            Some(Doc::Crdt(am)) => Ok(am.save()),
            Some(Doc::Blob(b)) => Ok(b.clone()),
            None => Ok(Vec::new()),
        }
    }

    /// Restore from a DAG snapshot (undo, redo, run drop).
    pub fn restore(&mut self, doc: &str, bytes: Option<&[u8]>) -> Result<()> {
        let kind = self.kind(doc).unwrap_or(proto::DocKind::Crdt);
        let restored = match (kind, bytes) {
            (proto::DocKind::Crdt, Some(b)) if !b.is_empty() => {
                Doc::Crdt(Box::new(AutoCommit::load(b).context("loading crdt snapshot")?))
            }
            (proto::DocKind::Crdt, _) => Doc::Crdt(Box::new(AutoCommit::new())),
            (proto::DocKind::Blob, Some(b)) => Doc::Blob(b.to_vec()),
            (proto::DocKind::Blob, None) => Doc::Blob(Vec::new()),
        };
        self.docs.insert(doc.to_string(), restored);
        Ok(())
    }

    // -- replica sync -------------------------------------------------------

    /// Sync message for a replica (the browser Client's copy of the document).
    pub fn sync_message(
        &mut self,
        doc: &str,
        state: &mut automerge::sync::State,
    ) -> Option<Vec<u8>> {
        use automerge::sync::SyncDoc;
        match self.docs.get_mut(doc) {
            Some(Doc::Crdt(am)) => am
                .sync()
                .generate_sync_message(state)
                .map(|m| m.encode()),
            _ => None,
        }
    }

    /// Apply a sync message received from a replica. Returns whether the
    /// document changed: a message may carry only the replica's state.
    pub fn receive_sync(
        &mut self,
        doc: &str,
        state: &mut automerge::sync::State,
        message: &[u8],
    ) -> Result<bool> {
        use automerge::sync::SyncDoc;
        let msg = automerge::sync::Message::decode(message).context("decoding sync message")?;
        match self.docs.get_mut(doc) {
            Some(Doc::Crdt(am)) => {
                let before = am.get_heads();
                am.sync()
                    .receive_sync_message(state, msg)
                    .context("applying sync message")?;
                Ok(am.get_heads() != before)
            }
            _ => anyhow::bail!("`{doc}` is not a crdt document"),
        }
    }
}

/// What changed between two JSON projections of a document, counted the way
/// `apply_json` counts: for a commit made from a replica's changes.
pub fn diff_changes(before: &J, after: &J) -> Changes {
    let mut changes = Changes::default();
    diff_into(before, after, &mut changes);
    changes
}

fn diff_into(before: &J, after: &J, changes: &mut Changes) {
    match (before, after) {
        (J::Object(a), J::Object(b)) => {
            for (k, v) in b {
                match a.get(k) {
                    None => changes.added += 1,
                    Some(prev) => diff_into(prev, v, changes),
                }
            }
            for k in a.keys() {
                if !b.contains_key(k) {
                    changes.removed += 1;
                }
            }
        }
        (J::Array(a), J::Array(b)) => {
            let common = a.len().min(b.len());
            for i in 0..common {
                diff_into(&a[i], &b[i], changes);
            }
            changes.added += b.len().saturating_sub(a.len());
            changes.removed += a.len().saturating_sub(b.len());
        }
        (a, b) => {
            if a != b {
                changes.changed += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JSON -> Automerge reconciliation
// ---------------------------------------------------------------------------

fn reconcile_map(tx: &mut AutoCommit, obj: &ObjId, next: &Map<String, J>) -> Result<Changes> {
    let mut changes = Changes::default();

    let existing: Vec<String> = tx.keys(obj).collect();
    for key in &existing {
        if !next.contains_key(key) {
            tx.delete(obj, key.as_str())?;
            changes.removed += 1;
        }
    }

    for (key, value) in next {
        let current = current_of(tx, obj, Prop::Map(key.clone()))?;
        changes.merge(reconcile_prop(tx, obj, Prop::Map(key.clone()), current, value)?);
    }
    Ok(changes)
}

fn reconcile_list(tx: &mut AutoCommit, obj: &ObjId, next: &[J]) -> Result<Changes> {
    let mut changes = Changes::default();

    // Trim from the end first so indices stay valid.
    let mut len = tx.length(obj);
    while len > next.len() {
        tx.delete(obj, len - 1)?;
        len -= 1;
        changes.removed += 1;
    }

    for (i, value) in next.iter().enumerate() {
        if i < len {
            let current = current_of(tx, obj, Prop::Seq(i))?;
            changes.merge(reconcile_prop(tx, obj, Prop::Seq(i), current, value)?);
        } else {
            insert_json(tx, obj, i, value)?;
            changes.added += 1;
        }
    }
    Ok(changes)
}

/// An owned snapshot of what sits at a property right now.
///
/// Taken before any mutation, so the read borrow of the document ends before the
/// write begins.
enum Current {
    Absent,
    Map(ObjId),
    List(ObjId),
    Scalar(J),
    OtherObject,
}

fn current_of(tx: &AutoCommit, obj: &ObjId, prop: Prop) -> Result<Current> {
    Ok(match tx.get(obj, prop)? {
        None => Current::Absent,
        Some((Value::Object(ObjType::Map), id)) => Current::Map(id),
        Some((Value::Object(ObjType::List), id)) => Current::List(id),
        Some((Value::Object(_), _)) => Current::OtherObject,
        Some((Value::Scalar(s), _)) => Current::Scalar(scalar_to_json(&s)),
    })
}

fn reconcile_prop(
    tx: &mut AutoCommit,
    obj: &ObjId,
    prop: Prop,
    current: Current,
    next: &J,
) -> Result<Changes> {
    let mut changes = Changes::default();
    match (current, next) {
        // Same shape: recurse, so only the leaves that actually differ change.
        (Current::Map(id), J::Object(map)) => {
            changes.merge(reconcile_map(tx, &id, map)?);
        }
        (Current::List(id), J::Array(arr)) => {
            changes.merge(reconcile_list(tx, &id, arr)?);
        }
        (Current::Scalar(existing), leaf) if !leaf.is_object() && !leaf.is_array() => {
            if existing != *leaf {
                put_json(tx, obj, prop, next)?;
                changes.changed += 1;
            }
        }
        (Current::Absent, _) => {
            put_json(tx, obj, prop, next)?;
            changes.added += 1;
        }
        // Shape changed at this property (scalar -> object, map -> list, ...).
        (Current::OtherObject, _) | (Current::Map(_), _) | (Current::List(_), _) | (Current::Scalar(_), _) => {
            put_json(tx, obj, prop, next)?;
            changes.changed += 1;
        }
    }
    Ok(changes)
}

fn put_json(tx: &mut AutoCommit, obj: &ObjId, prop: Prop, value: &J) -> Result<()> {
    match value {
        J::Object(map) => {
            let id = tx.put_object(obj, prop, ObjType::Map)?;
            reconcile_map(tx, &id, map)?;
        }
        J::Array(arr) => {
            let id = tx.put_object(obj, prop, ObjType::List)?;
            for (i, v) in arr.iter().enumerate() {
                insert_json(tx, &id, i, v)?;
            }
        }
        J::Null => tx.put(obj, prop, ScalarValue::Null)?,
        J::Bool(b) => tx.put(obj, prop, *b)?,
        J::String(s) => tx.put(obj, prop, s.as_str())?,
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                tx.put(obj, prop, i)?
            } else if let Some(u) = n.as_u64() {
                tx.put(obj, prop, u)?
            } else {
                tx.put(obj, prop, n.as_f64().unwrap_or(0.0))?
            }
        }
    }
    Ok(())
}

fn insert_json(tx: &mut AutoCommit, obj: &ObjId, index: usize, value: &J) -> Result<()> {
    match value {
        J::Object(map) => {
            let id = tx.insert_object(obj, index, ObjType::Map)?;
            reconcile_map(tx, &id, map)?;
        }
        J::Array(arr) => {
            let id = tx.insert_object(obj, index, ObjType::List)?;
            for (i, v) in arr.iter().enumerate() {
                insert_json(tx, &id, i, v)?;
            }
        }
        J::Null => tx.insert(obj, index, ScalarValue::Null)?,
        J::Bool(b) => tx.insert(obj, index, *b)?,
        J::String(s) => tx.insert(obj, index, s.as_str())?,
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                tx.insert(obj, index, i)?
            } else if let Some(u) = n.as_u64() {
                tx.insert(obj, index, u)?
            } else {
                tx.insert(obj, index, n.as_f64().unwrap_or(0.0))?
            }
        }
    }
    Ok(())
}

fn scalar_to_json(s: &ScalarValue) -> J {
    match s {
        ScalarValue::Null => J::Null,
        ScalarValue::Boolean(b) => J::Bool(*b),
        ScalarValue::Str(s) => J::String(s.to_string()),
        ScalarValue::Int(i) => J::Number((*i).into()),
        ScalarValue::Uint(u) => J::Number((*u).into()),
        ScalarValue::Counter(c) => J::Number(i64::from(c).into()),
        ScalarValue::Timestamp(t) => J::Number((*t).into()),
        ScalarValue::F64(f) => serde_json::Number::from_f64(*f).map(J::Number).unwrap_or(J::Null),
        ScalarValue::Bytes(b) => J::String(format!("bytes:{}", b.len())),
        ScalarValue::Unknown { .. } => J::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_with(doc: &str, initial: J) -> DocStore {
        let mut s = DocStore::new();
        s.ensure(doc, proto::DocKind::Crdt);
        s.apply_json(doc, &initial).unwrap();
        s
    }

    #[test]
    fn json_round_trips_through_the_crdt() {
        let initial = json!({
            "shapes": [{"id": "s1", "kind": "rect", "x": 10, "text": "risk"}],
            "title": "Board"
        });
        let mut s = store_with("board", initial.clone());
        assert_eq!(s.json("board").unwrap(), initial);
    }

    #[test]
    fn an_unchanged_document_produces_no_changes() {
        let initial = json!({"a": 1, "b": {"c": "x"}, "d": [1, 2, 3]});
        let mut s = store_with("board", initial.clone());
        let changes = s.apply_json("board", &initial).unwrap();
        assert!(changes.is_empty(), "got {changes:?}");
        assert_eq!(changes.summary(), "no change");
    }

    #[test]
    fn editing_one_leaf_changes_exactly_one_field() {
        let mut s = store_with("board", json!({"a": 1, "b": {"c": "x", "e": 2}}));
        let changes = s
            .apply_json("board", &json!({"a": 1, "b": {"c": "y", "e": 2}}))
            .unwrap();
        assert_eq!(
            changes,
            Changes {
                added: 0,
                removed: 0,
                changed: 1
            }
        );
        assert_eq!(s.json("board").unwrap()["b"]["c"], "y");
    }

    #[test]
    fn adding_and_removing_list_items_is_counted_separately() {
        let mut s = store_with("board", json!({"shapes": [{"id": "s1"}, {"id": "s2"}]}));
        let changes = s
            .apply_json(
                "board",
                &json!({"shapes": [{"id": "s1"}, {"id": "s2"}, {"id": "s3"}]}),
            )
            .unwrap();
        assert_eq!(changes.added, 1);
        assert_eq!(changes.removed, 0);
        assert_eq!(changes.summary(), "added 1 field(s)");

        let changes = s.apply_json("board", &json!({"shapes": [{"id": "s1"}]})).unwrap();
        assert_eq!(changes.removed, 2);
    }

    #[test]
    fn snapshot_and_restore_preserve_the_document() {
        let mut s = store_with("board", json!({"shapes": [{"id": "s1", "x": 4}]}));
        let snap = s.snapshot("board").unwrap();
        s.apply_json("board", &json!({"shapes": []})).unwrap();
        assert_eq!(s.json("board").unwrap()["shapes"].as_array().unwrap().len(), 0);

        s.restore("board", Some(&snap)).unwrap();
        assert_eq!(s.json("board").unwrap()["shapes"][0]["id"], "s1");
    }

    #[test]
    fn restoring_nothing_yields_an_empty_document() {
        let mut s = store_with("board", json!({"a": 1}));
        s.restore("board", None).unwrap();
        assert_eq!(s.json("board").unwrap(), json!({}));
    }

    #[test]
    fn a_replica_catches_up_over_the_sync_protocol() {
        // This is the path a browser Client uses to hold a live copy of the doc.
        let mut core = store_with("board", json!({"shapes": [{"id": "s1"}], "title": "Board"}));
        let mut replica = DocStore::new();
        replica.ensure("board", proto::DocKind::Crdt);

        let mut core_state = automerge::sync::State::new();
        let mut replica_state = automerge::sync::State::new();

        // Pump messages until both sides go quiet.
        for _ in 0..10 {
            let mut moved = false;
            if let Some(m) = core.sync_message("board", &mut core_state) {
                replica.receive_sync("board", &mut replica_state, &m).unwrap();
                moved = true;
            }
            if let Some(m) = replica.sync_message("board", &mut replica_state) {
                core.receive_sync("board", &mut core_state, &m).unwrap();
                moved = true;
            }
            if !moved {
                break;
            }
        }

        assert_eq!(replica.json("board").unwrap(), core.json("board").unwrap());
        assert_eq!(replica.json("board").unwrap()["title"], "Board");
    }

    #[test]
    fn blob_documents_store_opaque_bytes() {
        let mut s = DocStore::new();
        s.ensure("scene", proto::DocKind::Blob);
        s.put_blob("scene", vec![1, 2, 3, 4]).unwrap();
        assert_eq!(s.snapshot("scene").unwrap(), vec![1, 2, 3, 4]);
        let projected = s.json("scene").unwrap();
        assert_eq!(projected["blob"], true);
        assert_eq!(projected["bytes"], 4);
    }

    #[test]
    fn a_diff_between_two_projections_counts_like_a_write_does() {
        let before = serde_json::json!({"title": "Board", "shapes": [{"id": "a", "x": 1}, {"id": "b", "x": 2}], "frames": []});
        let same = diff_changes(&before, &before);
        assert!(same.is_empty());
        let after = serde_json::json!({"title": "Plan", "shapes": [{"id": "a", "x": 5}, {"id": "b", "x": 2}, {"id": "c", "x": 3}], "frames": [], "extra": 1});
        let changes = diff_changes(&before, &after);
        assert_eq!((changes.added, changes.changed, changes.removed), (2, 2, 0), "{changes:?}");
        let fewer = serde_json::json!({"shapes": [{"id": "a", "x": 1}]});
        let changes = diff_changes(&before, &fewer);
        assert_eq!((changes.added, changes.changed, changes.removed), (0, 0, 3), "{changes:?}");
    }
}
