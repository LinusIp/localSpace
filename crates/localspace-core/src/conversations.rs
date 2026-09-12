//! Conversations (architecture v2 §8): the chat harness keeps several, and
//! the transcript Core works on is the current one. They live in the
//! database, per user and workspace (Pilot 1, Phase A, answer 5); this is
//! the working set for one user in one workspace, written through on every
//! change, so nothing is lost between a turn and a restart.

use crate::store;
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Where conversations were kept before the database held them: one JSON
/// file under the data directory, imported once by the v3 migration.
pub const LEGACY_FILE: &str = "conversations.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub messages: Vec<proto::ChatMessage>,
}

/// The shape of the legacy file.
#[derive(Debug, Default, Deserialize)]
struct LegacyFile {
    #[serde(default)]
    current: String,
    #[serde(default)]
    conversations: Vec<Conversation>,
}

/// The conversations of the legacy file, and which was current, if the
/// file is there and readable.
pub fn read_legacy_file(path: &Path) -> Option<(Vec<Conversation>, String)> {
    let text = std::fs::read_to_string(path).ok()?;
    let file: LegacyFile = serde_json::from_str(&text).ok()?;
    Some((file.conversations, file.current))
}

pub struct Store {
    pub current: String,
    pub conversations: Vec<Conversation>,
    db: store::Store,
    user: String,
    workspace: String,
}

impl Store {
    /// The working set of `user` in `workspace`, with at least one
    /// conversation, so there is always a current transcript.
    pub fn load(db: &store::Store, user: &str, workspace: &str, now_ms: u64) -> Store {
        let mut conversations = db.conversations(user, workspace).unwrap_or_default();
        conversations.sort_by(|a, b| {
            a.created_ms
                .cmp(&b.created_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        let current = db
            .current_conversation(user, workspace)
            .unwrap_or_default()
            .unwrap_or_default();
        let mut store = Store {
            current,
            conversations,
            db: db.clone(),
            user: user.to_string(),
            workspace: workspace.to_string(),
        };
        if store.conversations.is_empty() {
            store.start(now_ms);
        }
        if store.current_index().is_none() {
            let last = store
                .conversations
                .last()
                .map(|c| c.id.clone())
                .unwrap_or_default();
            store.set_current(last);
        }
        store
    }

    fn persist(&self, conversation: &Conversation) {
        if let Err(e) = self
            .db
            .put_conversation(&self.user, &self.workspace, conversation)
        {
            tracing::warn!("conversation {} not written: {e:#}", conversation.id);
        }
    }

    fn set_current(&mut self, id: String) {
        self.current = id;
        if let Err(e) = self
            .db
            .set_current_conversation(&self.user, &self.workspace, &self.current)
        {
            tracing::warn!("current conversation not written: {e:#}");
        }
    }

    pub fn current_index(&self) -> Option<usize> {
        self.conversations.iter().position(|c| c.id == self.current)
    }

    pub fn current(&self) -> Option<&Conversation> {
        self.current_index().map(|i| &self.conversations[i])
    }

    /// A new, empty conversation, made current.
    pub fn start(&mut self, now_ms: u64) -> &Conversation {
        let id = format!("c_{now_ms}_{}", self.conversations.len() + 1);
        let conversation = Conversation {
            id: id.clone(),
            title: "New chat".into(),
            created_ms: now_ms,
            updated_ms: now_ms,
            messages: Vec::new(),
        };
        self.persist(&conversation);
        self.conversations.push(conversation);
        self.set_current(id);
        self.conversations.last().expect("just pushed")
    }

    /// Write the transcript back into the current conversation.
    pub fn record(&mut self, messages: &[proto::ChatMessage], now_ms: u64) {
        let Some(i) = self.current_index() else {
            return;
        };
        let c = &mut self.conversations[i];
        let changed =
            c.messages.len() != messages.len() || c.messages.is_empty() && !messages.is_empty();
        if changed {
            c.updated_ms = now_ms;
        }
        c.messages = messages.to_vec();
        if c.title == "New chat"
            && let Some(first) = c.messages.iter().find(|m| m.role == proto::Role::User)
        {
            c.title = title_from(&first.content);
        }
        let snapshot = c.clone();
        self.persist(&snapshot);
    }

    pub fn select(&mut self, id: &str) -> Option<&Conversation> {
        let i = self.conversations.iter().position(|c| c.id == id)?;
        self.set_current(id.to_string());
        Some(&self.conversations[i])
    }

    /// Remove one. If it was current, the most recent other one becomes
    /// current, or a new one if none is left.
    pub fn delete(&mut self, id: &str, now_ms: u64) -> bool {
        let before = self.conversations.len();
        self.conversations.retain(|c| c.id != id);
        if self.conversations.len() == before {
            return false;
        }
        if let Err(e) = self.db.delete_conversation(&self.user, &self.workspace, id) {
            tracing::warn!("conversation {id} not deleted: {e:#}");
        }
        if self.current == id {
            match self
                .conversations
                .iter()
                .max_by_key(|c| c.updated_ms)
                .map(|c| c.id.clone())
            {
                Some(next) => self.set_current(next),
                None => {
                    self.start(now_ms);
                }
            }
        }
        true
    }

    pub fn rename(&mut self, id: &str, title: &str) -> bool {
        match self.conversations.iter_mut().find(|c| c.id == id) {
            Some(c) => {
                c.title = title.trim().chars().take(80).collect();
                let snapshot = c.clone();
                self.persist(&snapshot);
                true
            }
            None => false,
        }
    }

    /// Bring the legacy file's conversations in, once: an empty placeholder
    /// this set started with gives way to them, and the file's current one
    /// becomes current.
    pub fn import(&mut self, conversations: Vec<Conversation>, current: String) {
        if conversations.is_empty() {
            return;
        }
        let placeholders: Vec<String> = self
            .conversations
            .iter()
            .filter(|c| c.messages.is_empty() && c.title == "New chat")
            .map(|c| c.id.clone())
            .collect();
        for id in placeholders {
            self.conversations.retain(|c| c.id != id);
            let _ = self
                .db
                .delete_conversation(&self.user, &self.workspace, &id);
        }
        for c in conversations {
            if self.conversations.iter().any(|have| have.id == c.id) {
                continue;
            }
            self.persist(&c);
            self.conversations.push(c);
        }
        self.conversations.sort_by(|a, b| {
            a.created_ms
                .cmp(&b.created_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        let current = if self.conversations.iter().any(|c| c.id == current) {
            current
        } else {
            self.conversations
                .last()
                .map(|c| c.id.clone())
                .unwrap_or_default()
        };
        self.set_current(current);
    }

    /// Newest first.
    pub fn summaries(&self) -> Vec<proto::ConversationSummary> {
        let mut list: Vec<proto::ConversationSummary> = self
            .conversations
            .iter()
            .map(|c| proto::ConversationSummary {
                id: c.id.clone(),
                title: c.title.clone(),
                created_ms: c.created_ms,
                updated_ms: c.updated_ms,
                messages: c
                    .messages
                    .iter()
                    .filter(|m| m.role != proto::Role::Tool)
                    .count(),
            })
            .collect();
        list.sort_by(|a, b| {
            b.updated_ms
                .cmp(&a.updated_ms)
                .then(b.created_ms.cmp(&a.created_ms))
        });
        list
    }
}

/// The first line of the first message, cut to a title's length.
pub fn title_from(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    let mut title: String = line.chars().take(60).collect();
    if line.chars().count() > 60 {
        title.push('…');
    }
    if title.is_empty() {
        "New chat".into()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(text: &str) -> proto::ChatMessage {
        proto::ChatMessage {
            role: proto::Role::User,
            content: text.into(),
            tool_calls: Vec::new(),
        }
    }

    fn db() -> store::Store {
        store::Store::in_memory().unwrap()
    }

    #[test]
    fn a_fresh_store_has_one_current_conversation() {
        let s = Store::load(&db(), "u", "ws_u", 1000);
        assert_eq!(s.conversations.len(), 1);
        assert_eq!(
            s.current().map(|c| c.id.clone()),
            s.conversations.first().map(|c| c.id.clone())
        );
        assert_eq!(s.current().unwrap().title, "New chat");
    }

    #[test]
    fn recording_titles_the_conversation_from_its_first_message_and_persists() {
        let db = db();
        let mut s = Store::load(&db, "u", "ws_u", 1000);
        s.record(
            &[user("Put three risks on the board as red stickies")],
            2000,
        );
        let again = Store::load(&db, "u", "ws_u", 3000);
        let c = again.current().unwrap();
        assert_eq!(c.title, "Put three risks on the board as red stickies");
        assert_eq!(c.messages.len(), 1);
        assert_eq!(c.updated_ms, 2000);
        assert_eq!(again.conversations.len(), 1, "nothing was started on top");
    }

    #[test]
    fn users_and_workspaces_keep_their_own_conversations() {
        let db = db();
        let mut a = Store::load(&db, "anna", "ws_anna", 1000);
        a.record(&[user("Anna's question")], 1500);
        let mut shared = Store::load(&db, "anna", "ws_team", 1000);
        shared.record(&[user("In the team workspace")], 1600);
        let b = Store::load(&db, "ben", "ws_ben", 2000);
        assert_eq!(
            b.current().unwrap().messages.len(),
            0,
            "Ben sees nothing of Anna's"
        );
        let anna_again = Store::load(&db, "anna", "ws_anna", 3000);
        assert_eq!(
            anna_again.current().unwrap().messages[0].content,
            "Anna's question"
        );
        let team_again = Store::load(&db, "anna", "ws_team", 3000);
        assert_eq!(
            team_again.current().unwrap().messages[0].content,
            "In the team workspace"
        );
    }

    #[test]
    fn new_select_and_delete_keep_a_current_conversation() {
        let db = db();
        let mut s = Store::load(&db, "u", "ws_u", 1000);
        let first = s.current.clone();
        s.record(&[user("first")], 1500);
        let second = s.start(2000).id.clone();
        assert_eq!(s.current, second);
        assert!(s.select(&first).is_some());
        assert_eq!(s.current().unwrap().messages.len(), 1);
        assert!(s.select("nowhere").is_none());

        assert!(s.delete(&first, 3000));
        assert_eq!(s.current, second, "the other one became current");
        assert!(s.delete(&second, 4000));
        assert_eq!(
            s.conversations.len(),
            1,
            "deleting the last one starts a fresh one"
        );
        assert_ne!(s.current, second);

        let list = s.summaries();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].messages, 0);

        // Every step was written through.
        let again = Store::load(&db, "u", "ws_u", 5000);
        assert_eq!(again.conversations.len(), 1);
        assert_eq!(again.current, s.current);
    }

    #[test]
    fn the_legacy_file_imports_once_and_replaces_the_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LEGACY_FILE);
        std::fs::write(
            &path,
            serde_json::json!({
                "version": 1,
                "current": "c_2",
                "conversations": [
                    {"id": "c_1", "title": "Older", "created_ms": 1, "updated_ms": 2, "messages": [user("one")]},
                    {"id": "c_2", "title": "Newer", "created_ms": 3, "updated_ms": 4, "messages": [user("two")]}
                ]
            })
            .to_string(),
        )
        .unwrap();
        let (list, current) = read_legacy_file(&path).unwrap();
        let db = db();
        let mut s = Store::load(&db, "u", "ws_u", 1000);
        s.import(list, current);
        assert_eq!(s.conversations.len(), 2, "the empty placeholder gave way");
        assert_eq!(s.current, "c_2");
        assert_eq!(s.current().unwrap().messages[0].content, "two");
        assert!(read_legacy_file(&dir.path().join("nothing.json")).is_none());
    }

    #[test]
    fn titles_are_one_line_and_bounded() {
        assert_eq!(title_from("hello\nworld"), "hello");
        assert_eq!(title_from("   "), "New chat");
        let long = "x".repeat(100);
        assert_eq!(title_from(&long).chars().count(), 61);
    }
}
