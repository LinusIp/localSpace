//! Conversations (architecture v2 §8): the chat harness keeps several, and
//! the transcript Core works on is the current one. They are kept as one
//! JSON file under the data directory — small, human-readable, and rewritten
//! whole after every turn, which is cheap at the sizes a conversation has.

use anyhow::{Context, Result};
use localspace_proto as proto;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "conversations.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub updated_ms: u64,
    pub messages: Vec<proto::ChatMessage>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "one")]
    pub version: u32,
    pub current: String,
    pub conversations: Vec<Conversation>,
    #[serde(skip)]
    path: Option<PathBuf>,
}

fn one() -> u32 {
    1
}

impl Store {
    /// The store under `data_dir`, or an in-memory one; either way with at
    /// least one conversation, so there is always a current transcript.
    pub fn load(data_dir: Option<&Path>, now_ms: u64) -> Store {
        let path = data_dir.map(|d| d.join(FILE));
        let mut store = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<Store>(&text).ok())
            .unwrap_or_default();
        store.path = path;
        if store.version == 0 {
            store.version = 1;
        }
        if store.conversations.is_empty() {
            store.start(now_ms);
        }
        if store.current_index().is_none() {
            store.current = store.conversations.last().map(|c| c.id.clone()).unwrap_or_default();
        }
        store
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
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
        self.conversations.push(Conversation {
            id: id.clone(),
            title: "New chat".into(),
            created_ms: now_ms,
            updated_ms: now_ms,
            messages: Vec::new(),
        });
        self.current = id;
        self.conversations.last().unwrap()
    }

    /// Write the transcript back into the current conversation.
    pub fn record(&mut self, messages: &[proto::ChatMessage], now_ms: u64) {
        let Some(i) = self.current_index() else { return };
        let c = &mut self.conversations[i];
        if c.messages.len() != messages.len() || c.messages.is_empty() && !messages.is_empty() {
            c.updated_ms = now_ms;
        }
        c.messages = messages.to_vec();
        if c.title == "New chat"
            && let Some(first) = c.messages.iter().find(|m| m.role == proto::Role::User) {
                c.title = title_from(&first.content);
            }
    }

    pub fn select(&mut self, id: &str) -> Option<&Conversation> {
        let i = self.conversations.iter().position(|c| c.id == id)?;
        self.current = id.to_string();
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
        if self.current == id {
            match self.conversations.iter().max_by_key(|c| c.updated_ms) {
                Some(c) => self.current = c.id.clone(),
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
                true
            }
            None => false,
        }
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
                messages: c.messages.iter().filter(|m| m.role != proto::Role::Tool).count(),
            })
            .collect();
        list.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms).then(b.created_ms.cmp(&a.created_ms)));
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

    #[test]
    fn a_fresh_store_has_one_current_conversation() {
        let s = Store::load(None, 1000);
        assert_eq!(s.conversations.len(), 1);
        assert_eq!(s.current().map(|c| c.id.clone()), s.conversations.first().map(|c| c.id.clone()));
        assert_eq!(s.current().unwrap().title, "New chat");
    }

    #[test]
    fn recording_titles_the_conversation_from_its_first_message_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::load(Some(dir.path()), 1000);
        s.record(&[user("Put three risks on the board as red stickies")], 2000);
        s.save().unwrap();
        let again = Store::load(Some(dir.path()), 3000);
        let c = again.current().unwrap();
        assert_eq!(c.title, "Put three risks on the board as red stickies");
        assert_eq!(c.messages.len(), 1);
        assert_eq!(c.updated_ms, 2000);
    }

    #[test]
    fn new_select_and_delete_keep_a_current_conversation() {
        let mut s = Store::load(None, 1000);
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
        assert_eq!(s.conversations.len(), 1, "deleting the last one starts a fresh one");
        assert_ne!(s.current, second);

        let list = s.summaries();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].messages, 0);
    }

    #[test]
    fn titles_are_one_line_and_bounded() {
        assert_eq!(title_from("hello\nworld"), "hello");
        assert_eq!(title_from("   "), "New chat");
        let long = "x".repeat(100);
        assert_eq!(title_from(&long).chars().count(), 61);
    }
}
