//! Tenancy and access control (deployment spec §§5–6).
//!
//! A workspace is the unit of access control, storage quota, retrieval scope and
//! audit scope. Every document carries an ACL; Core checks it on every read, every
//! tool call, every retrieval hit and every sync message. There is no path that
//! bypasses it — admin break-glass is a normal access with a reason attached.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Principal {
    User(String),
    Group(String),
    /// Everyone in the workspace.
    Workspace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    View = 0,
    Comment = 1,
    Edit = 2,
    Owner = 3,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::View => "view",
            Level::Comment => "comment",
            Level::Edit => "edit",
            Level::Owner => "owner",
        }
    }
}

/// Who a request is being made as.
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub user: String,
    pub groups: Vec<String>,
    /// Org admin using break-glass. Still an ordinary access, but audited with a reason.
    pub break_glass: Option<String>,
}

impl Identity {
    pub fn user(id: &str) -> Identity {
        Identity {
            user: id.to_string(),
            ..Default::default()
        }
    }

    pub fn in_group(mut self, group: &str) -> Identity {
        self.groups.push(group.to_string());
        self
    }

    fn matches(&self, p: &Principal) -> bool {
        match p {
            Principal::User(u) => *u == self.user,
            Principal::Group(g) => self.groups.iter().any(|x| x == g),
            Principal::Workspace => true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Acl {
    pub entries: Vec<(Principal, Level)>,
}

impl Acl {
    pub fn owned_by(user: &str) -> Acl {
        Acl {
            entries: vec![(Principal::User(user.into()), Level::Owner)],
        }
    }

    pub fn grant(mut self, principal: Principal, level: Level) -> Acl {
        self.entries.push((principal, level));
        self
    }

    /// Highest level this identity holds, if any.
    pub fn level_for(&self, id: &Identity) -> Option<Level> {
        self.entries
            .iter()
            .filter(|(p, _)| id.matches(p))
            .map(|(_, l)| *l)
            .max()
    }

    pub fn allows(&self, id: &Identity, need: Level) -> bool {
        self.level_for(id).map(|l| l >= need).unwrap_or(false)
    }
}

/// How agent writes reach a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentWrites {
    /// An agent commit is like any member commit, undoable the same way.
    Direct,
    /// The run writes to a proposal branch; someone applies or discards it.
    Proposal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub personal_to: Option<String>,
    pub default_acl: Acl,
    pub agent_writes: AgentWrites,
}

impl Workspace {
    pub fn personal(user: &str) -> Workspace {
        Workspace {
            id: format!("ws_{user}"),
            name: format!("{user}'s workspace"),
            personal_to: Some(user.to_string()),
            default_acl: Acl::owned_by(user),
            // Personal-workspace documents default to direct.
            agent_writes: AgentWrites::Direct,
        }
    }

    pub fn shared(id: &str, name: &str, owner_group: &str) -> Workspace {
        Workspace {
            id: id.into(),
            name: name.into(),
            personal_to: None,
            default_acl: Acl::default().grant(Principal::Group(owner_group.into()), Level::Edit),
            // Shared documents get proposals, so an agent never writes to the head.
            agent_writes: AgentWrites::Proposal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub id: String,
    pub workspace: String,
    /// A document can be tightened, never loosened beyond its workspace.
    pub acl: Acl,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Denied {
    NoSuchDocument(String),
    NoSuchWorkspace(String),
    Insufficient { need: Level, held: Option<Level> },
}

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Denied::NoSuchDocument(d) => write!(f, "no document `{d}`"),
            Denied::NoSuchWorkspace(w) => write!(f, "no workspace `{w}`"),
            Denied::Insufficient { need, held } => match held {
                Some(l) => write!(
                    f,
                    "needs `{}` on this document; you hold `{}`",
                    need.label(),
                    l.label()
                ),
                None => write!(f, "needs `{}` on this document", need.label()),
            },
        }
    }
}

#[derive(Default)]
pub struct AccessControl {
    workspaces: HashMap<String, Workspace>,
    documents: HashMap<String, DocumentMeta>,
}

impl AccessControl {
    pub fn new() -> AccessControl {
        AccessControl::default()
    }

    pub fn add_workspace(&mut self, ws: Workspace) {
        self.workspaces.insert(ws.id.clone(), ws);
    }

    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.get(id)
    }

    /// Register a document, inheriting the workspace ACL when none is given.
    pub fn add_document(&mut self, id: &str, workspace: &str, title: &str, acl: Option<Acl>) {
        let inherited = self
            .workspaces
            .get(workspace)
            .map(|w| w.default_acl.clone())
            .unwrap_or_default();
        self.documents.insert(
            id.to_string(),
            DocumentMeta {
                id: id.to_string(),
                workspace: workspace.to_string(),
                acl: acl.unwrap_or(inherited),
                title: title.to_string(),
            },
        );
    }

    pub fn document(&self, id: &str) -> Option<&DocumentMeta> {
        self.documents.get(id)
    }

    /// The one check. Every read, tool call, retrieval hit and sync message goes
    /// through here.
    pub fn check(&self, id: &Identity, doc: &str, need: Level) -> Result<Level, Denied> {
        let meta = self
            .documents
            .get(doc)
            .ok_or_else(|| Denied::NoSuchDocument(doc.to_string()))?;
        if !self.workspaces.contains_key(&meta.workspace) {
            return Err(Denied::NoSuchWorkspace(meta.workspace.clone()));
        }
        let held = meta.acl.level_for(id);
        match held {
            Some(l) if l >= need => Ok(l),
            other => Err(Denied::Insufficient { need, held: other }),
        }
    }

    /// Filter retrieval hits *before* ranking, so top-k is never diluted by hits
    /// the user cannot see.
    pub fn visible_documents(&self, id: &Identity) -> Vec<&DocumentMeta> {
        self.documents
            .values()
            .filter(|d| d.acl.allows(id, Level::View))
            .collect()
    }

    /// How an agent's writes reach this document.
    pub fn agent_writes(&self, doc: &str) -> AgentWrites {
        self.documents
            .get(doc)
            .and_then(|d| self.workspaces.get(&d.workspace))
            .map(|w| w.agent_writes)
            .unwrap_or(AgentWrites::Direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> AccessControl {
        let mut ac = AccessControl::new();
        ac.add_workspace(Workspace::personal("anna"));
        ac.add_workspace(Workspace::shared("ws_finance", "Finance", "finance-team"));
        ac.add_document("d_personal", "ws_anna", "Anna's board", None);
        ac.add_document("d_shared", "ws_finance", "Q4 board", None);
        ac.add_document(
            "d_tight",
            "ws_finance",
            "Comp review",
            Some(Acl::owned_by("cfo")),
        );
        ac
    }

    #[test]
    fn a_member_of_the_owning_group_can_edit_a_shared_document() {
        let ac = setup();
        let bob = Identity::user("bob").in_group("finance-team");
        assert_eq!(ac.check(&bob, "d_shared", Level::Edit), Ok(Level::Edit));
    }

    #[test]
    fn a_user_outside_the_group_is_refused_with_a_useful_message() {
        let ac = setup();
        let carl = Identity::user("carl");
        let err = ac.check(&carl, "d_shared", Level::View).unwrap_err();
        assert_eq!(
            err,
            Denied::Insufficient {
                need: Level::View,
                held: None
            }
        );
        assert!(err.to_string().contains("needs `view`"));
    }

    #[test]
    fn a_tightened_document_is_not_loosened_by_its_workspace() {
        let ac = setup();
        let bob = Identity::user("bob").in_group("finance-team");
        // Bob can edit the workspace's normal board but not the tightened one.
        assert!(ac.check(&bob, "d_shared", Level::Edit).is_ok());
        assert!(ac.check(&bob, "d_tight", Level::View).is_err());
        let cfo = Identity::user("cfo");
        assert_eq!(ac.check(&cfo, "d_tight", Level::Owner), Ok(Level::Owner));
    }

    #[test]
    fn a_view_member_can_read_but_not_write() {
        let mut ac = setup();
        ac.add_document(
            "d_ro",
            "ws_finance",
            "Read only",
            Some(Acl::default().grant(Principal::User("dana".into()), Level::View)),
        );
        let dana = Identity::user("dana");
        assert!(ac.check(&dana, "d_ro", Level::View).is_ok());
        assert!(ac.check(&dana, "d_ro", Level::Edit).is_err());
    }

    #[test]
    fn retrieval_sees_only_documents_the_user_holds() {
        let ac = setup();
        let bob = Identity::user("bob").in_group("finance-team");
        let visible: Vec<&str> = ac
            .visible_documents(&bob)
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert!(visible.contains(&"d_shared"));
        assert!(!visible.contains(&"d_tight"), "tightened doc must not be visible");
        assert!(!visible.contains(&"d_personal"), "another user's personal doc");
    }

    #[test]
    fn agents_propose_in_shared_workspaces_and_write_directly_in_personal_ones() {
        let ac = setup();
        assert_eq!(ac.agent_writes("d_shared"), AgentWrites::Proposal);
        assert_eq!(ac.agent_writes("d_personal"), AgentWrites::Direct);
    }

    #[test]
    fn a_missing_document_is_refused_not_defaulted_open() {
        let ac = setup();
        let anyone = Identity::user("anyone");
        assert_eq!(
            ac.check(&anyone, "d_nope", Level::View),
            Err(Denied::NoSuchDocument("d_nope".into()))
        );
    }

    #[test]
    fn levels_are_ordered() {
        assert!(Level::Owner > Level::Edit);
        assert!(Level::Edit > Level::Comment);
        assert!(Level::Comment > Level::View);
    }
}
