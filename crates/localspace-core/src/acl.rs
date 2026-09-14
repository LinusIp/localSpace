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
    /// An administrator inside a workspace they are not a member of, with
    /// the reason the audit keeps (deployment §6.1). An ordinary access: it
    /// goes through the same check, and holds `owner` on that one
    /// workspace's documents and nothing anywhere else.
    pub break_glass: Option<BreakGlass>,
}

/// Where an administrator went in with a reason, and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakGlass {
    pub workspace: String,
    pub reason: String,
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

    /// Whether a principal names this identity. "Everyone in the workspace"
    /// is not answered here, where no workspace is in sight: `AccessControl`
    /// resolves it against the workspace's own members.
    fn matches(&self, p: &Principal) -> bool {
        match p {
            Principal::User(u) => *u == self.user,
            Principal::Group(g) => self.groups.iter().any(|x| x == g),
            Principal::Workspace => false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Who holds what here: the workspace's members. A document inherits
    /// this unless it was tightened.
    pub default_acl: Acl,
    pub agent_writes: AgentWrites,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub created_by: String,
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
            created_ms: 0,
            created_by: user.to_string(),
        }
    }

    /// A shared workspace an administrator made, owned by them; members are
    /// added one by one (deployment §5). Agents write proposals here.
    pub fn shared_by(id: &str, name: &str, owner: &str, now_ms: u64) -> Workspace {
        Workspace {
            id: id.into(),
            name: name.into(),
            personal_to: None,
            default_acl: Acl::owned_by(owner),
            agent_writes: AgentWrites::Proposal,
            created_ms: now_ms,
            created_by: owner.to_string(),
        }
    }

    /// The level this workspace itself gives an identity, before any
    /// document tightening.
    pub fn level_of(&self, id: &Identity) -> Option<Level> {
        self.default_acl.level_for(id)
    }

    /// Give a principal a level here, replacing what it had.
    pub fn set_member(&mut self, principal: Principal, level: Level) {
        self.default_acl.entries.retain(|(p, _)| *p != principal);
        self.default_acl.entries.push((principal, level));
    }

    /// Returns whether the principal was a member.
    pub fn remove_member(&mut self, principal: &Principal) -> bool {
        let before = self.default_acl.entries.len();
        self.default_acl.entries.retain(|(p, _)| p != principal);
        self.default_acl.entries.len() != before
    }

    pub fn shared(id: &str, name: &str, owner_group: &str) -> Workspace {
        Workspace {
            id: id.into(),
            name: name.into(),
            personal_to: None,
            default_acl: Acl::default().grant(Principal::Group(owner_group.into()), Level::Edit),
            // Shared documents get proposals, so an agent never writes to the head.
            agent_writes: AgentWrites::Proposal,
            created_ms: 0,
            created_by: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMeta {
    pub id: String,
    pub workspace: String,
    /// Set when the document was tightened; otherwise the workspace's
    /// members hold what they hold in the workspace, as it changes.
    pub acl: Option<Acl>,
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

    /// Register a document. Without an ACL of its own it follows its
    /// workspace's members as they change; with one, it is tightened.
    pub fn add_document(&mut self, id: &str, workspace: &str, title: &str, acl: Option<Acl>) {
        self.documents.insert(
            id.to_string(),
            DocumentMeta {
                id: id.to_string(),
                workspace: workspace.to_string(),
                acl,
                title: title.to_string(),
            },
        );
    }

    pub fn document(&self, id: &str) -> Option<&DocumentMeta> {
        self.documents.get(id)
    }

    /// What an identity holds on a document: its level in the workspace,
    /// capped by the document's own tightening when it has one. A tightening
    /// never gives what the workspace does not, and someone the workspace no
    /// longer holds holds nothing here either, whatever an older tightening
    /// says. "Everyone in the workspace" in a tightening means the members.
    fn held(&self, meta: &DocumentMeta, id: &Identity) -> Option<Level> {
        let ws = self.workspaces.get(&meta.workspace)?;
        let in_workspace = ws.level_of(id)?;
        match &meta.acl {
            None => Some(in_workspace),
            Some(own) => {
                let tightened = own
                    .entries
                    .iter()
                    .filter(|(p, _)| matches!(p, Principal::Workspace) || id.matches(p))
                    .map(|(_, l)| *l)
                    .max()?;
                Some(tightened.min(in_workspace))
            }
        }
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
        let held = match &id.break_glass {
            Some(glass) if glass.workspace == meta.workspace => Some(Level::Owner),
            _ => self.held(meta, id),
        };
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
            .filter(|d| {
                matches!(&id.break_glass, Some(glass) if glass.workspace == d.workspace)
                    || self.held(d, id).is_some()
            })
            .collect()
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &Workspace> {
        self.workspaces.values()
    }

    pub fn workspace_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.get_mut(id)
    }

    /// The level a workspace itself gives an identity: membership.
    pub fn level_in(&self, workspace: &str, id: &Identity) -> Option<Level> {
        self.workspaces.get(workspace).and_then(|w| w.level_of(id))
    }

    /// Tighten a document, or clear the tightening. A level above what the
    /// workspace gives the same principal is refused: a document is never
    /// loosened beyond its workspace (deployment §6.1).
    pub fn set_document_acl(&mut self, doc: &str, acl: Option<Acl>) -> Result<(), String> {
        let workspace = match self.documents.get(doc) {
            Some(meta) => meta.workspace.clone(),
            None => return Err(format!("no document `{doc}`")),
        };
        if let Some(acl) = &acl {
            let Some(ws) = self.workspaces.get(&workspace) else {
                return Err(format!("no workspace `{workspace}`"));
            };
            for (principal, level) in &acl.entries {
                let in_workspace = ws
                    .default_acl
                    .entries
                    .iter()
                    .filter(|(p, _)| p == principal)
                    .map(|(_, l)| *l)
                    .max();
                match in_workspace {
                    Some(held) if held >= *level => {}
                    Some(held) => {
                        return Err(format!(
                            "a document cannot give more than its workspace does: `{}` there, `{}` asked",
                            held.label(),
                            level.label()
                        ));
                    }
                    None => {
                        return Err(
                            "a document cannot give access to someone its workspace does not"
                                .into(),
                        );
                    }
                }
            }
        }
        if let Some(meta) = self.documents.get_mut(doc) {
            meta.acl = acl;
        }
        Ok(())
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
        // The CFO is a member of the workspace: a tightening can only narrow
        // what the workspace gives, never name someone it does not hold.
        ac.workspace_mut("ws_finance")
            .unwrap()
            .set_member(Principal::User("cfo".into()), Level::Owner);
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
    fn break_glass_is_owner_in_that_workspace_and_nothing_elsewhere() {
        let ac = setup();
        let mut boss = Identity::user("boss");
        assert!(ac.check(&boss, "d_shared", Level::View).is_err());
        boss.break_glass = Some(BreakGlass {
            workspace: "ws_finance".into(),
            reason: "incident 42".into(),
        });
        assert_eq!(ac.check(&boss, "d_shared", Level::Owner), Ok(Level::Owner));
        assert_eq!(
            ac.check(&boss, "d_tight", Level::Owner),
            Ok(Level::Owner),
            "a tightened document too"
        );
        assert!(
            ac.check(&boss, "d_personal", Level::View).is_err(),
            "not Anna's workspace"
        );
        let visible: Vec<&str> = ac
            .visible_documents(&boss)
            .iter()
            .map(|d| d.id.as_str())
            .collect();
        assert!(visible.contains(&"d_shared"), "{visible:?}");
        assert!(visible.contains(&"d_tight"), "{visible:?}");
        assert!(!visible.contains(&"d_personal"), "{visible:?}");
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
        // Dana edits in the workspace; this one document is tightened to view.
        ac.workspace_mut("ws_finance")
            .unwrap()
            .set_member(Principal::User("dana".into()), Level::Edit);
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
    fn someone_the_workspace_no_longer_holds_holds_nothing_on_a_tightened_document() {
        let mut ac = setup();
        let cfo = Identity::user("cfo");
        assert_eq!(ac.check(&cfo, "d_tight", Level::Owner), Ok(Level::Owner));
        ac.workspace_mut("ws_finance")
            .unwrap()
            .remove_member(&Principal::User("cfo".into()));
        assert!(
            ac.check(&cfo, "d_tight", Level::View).is_err(),
            "the tightening still names the CFO, and gives nothing without the workspace"
        );
        assert!(!ac.visible_documents(&cfo).iter().any(|d| d.id == "d_tight"));
    }

    #[test]
    fn everyone_in_the_workspace_in_a_tightening_means_its_members() {
        let mut ac = setup();
        ac.add_document(
            "d_team",
            "ws_finance",
            "Team notes",
            Some(Acl::default().grant(Principal::Workspace, Level::View)),
        );
        let bob = Identity::user("bob").in_group("finance-team");
        let carl = Identity::user("carl");
        assert_eq!(ac.check(&bob, "d_team", Level::View), Ok(Level::View));
        assert!(
            ac.check(&bob, "d_team", Level::Edit).is_err(),
            "view is what the tightening gives"
        );
        assert!(
            ac.check(&carl, "d_team", Level::View).is_err(),
            "not a member: not everyone"
        );
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
        assert!(
            !visible.contains(&"d_tight"),
            "tightened doc must not be visible"
        );
        assert!(
            !visible.contains(&"d_personal"),
            "another user's personal doc"
        );
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
