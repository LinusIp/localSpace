//! Who has which board open (Pilot 1; the app screens' board shows the
//! people on it). A window of the shell announces the board it shows, again
//! every so often, and nothing when it leaves; a window that stops
//! announcing is forgotten after the time-to-live, so a laptop closed on a
//! board does not haunt it. Kept in memory: presence is not state worth a
//! database, and a restart forgets it rightly.

use std::collections::{BTreeSet, HashMap};
use std::time::{Duration, Instant};

/// A board in a workspace: the unit people are present at.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Place {
    pub workspace: String,
    pub board: String,
}

#[derive(Debug, Clone)]
struct Entry {
    user: String,
    place: Place,
    /// When this window first announced this board: the order of arrival.
    since: Instant,
    /// When it last announced: the time-to-live counts from here.
    at: Instant,
}

/// Every window on a board, by the window's own name.
#[derive(Debug, Default)]
pub struct Table {
    entries: HashMap<String, Entry>,
}

impl Table {
    /// Record what `peer`, one window of `user`, shows now: a board in the
    /// user's workspace, or nothing when it left. Windows that fell silent
    /// are forgotten first. Returns every place whose company changed.
    pub fn announce(
        &mut self,
        peer: &str,
        user: &str,
        workspace: &str,
        board: Option<&str>,
        now: Instant,
        ttl: Duration,
    ) -> BTreeSet<Place> {
        let mut changed = self.sweep(now, ttl);
        let previous = self.entries.remove(peer);
        let Some(board) = board else {
            if let Some(old) = previous {
                changed.insert(old.place);
            }
            return changed;
        };
        let place = Place {
            workspace: workspace.to_string(),
            board: board.to_string(),
        };
        let since = match &previous {
            Some(old) if old.place == place && old.user == user => old.since,
            Some(old) => {
                changed.insert(old.place.clone());
                now
            }
            None => now,
        };
        // A window announcing the same board again is a heartbeat: nothing
        // changed for the others unless it had been swept meanwhile.
        let same = matches!(&previous, Some(old) if old.place == place && old.user == user);
        self.entries.insert(
            peer.to_string(),
            Entry {
                user: user.to_string(),
                place: place.clone(),
                since,
                at: now,
            },
        );
        if !same {
            changed.insert(place);
        }
        changed
    }

    /// Forget every window that has not announced within `ttl`; the places
    /// they were at are returned.
    pub fn sweep(&mut self, now: Instant, ttl: Duration) -> BTreeSet<Place> {
        let mut changed = BTreeSet::new();
        self.entries.retain(|_, e| {
            let alive = now.saturating_duration_since(e.at) <= ttl;
            if !alive {
                changed.insert(e.place.clone());
            }
            alive
        });
        changed
    }

    /// The users at a place, each once, in the order they arrived.
    pub fn users_at(&self, place: &Place) -> Vec<String> {
        let mut here: Vec<&Entry> = self
            .entries
            .values()
            .filter(|e| &e.place == place)
            .collect();
        here.sort_by_key(|e| e.since);
        let mut out: Vec<String> = Vec::new();
        for e in here {
            if !out.contains(&e.user) {
                out.push(e.user.clone());
            }
        }
        out
    }

    /// How many windows are known, for the tests.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(45);

    fn place(board: &str) -> Place {
        Place {
            workspace: "ws".into(),
            board: board.into(),
        }
    }

    #[test]
    fn arrivals_are_in_order_and_a_person_counts_once() {
        let mut t = Table::default();
        let t0 = Instant::now();
        let changed = t.announce("w1", "anna", "ws", Some("board"), t0, TTL);
        assert_eq!(
            changed.into_iter().collect::<Vec<_>>(),
            vec![place("board")]
        );
        t.announce(
            "w2",
            "bek",
            "ws",
            Some("board"),
            t0 + Duration::from_secs(1),
            TTL,
        );
        // Anna's second window changes nothing for the others.
        let changed = t.announce(
            "w3",
            "anna",
            "ws",
            Some("board"),
            t0 + Duration::from_secs(2),
            TTL,
        );
        assert_eq!(
            changed.len(),
            1,
            "the place is reported, so the list can be sent again"
        );
        assert_eq!(t.users_at(&place("board")), vec!["anna", "bek"]);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn a_heartbeat_changes_nothing_and_leaving_does() {
        let mut t = Table::default();
        let t0 = Instant::now();
        t.announce("w1", "anna", "ws", Some("board"), t0, TTL);
        let again = t.announce(
            "w1",
            "anna",
            "ws",
            Some("board"),
            t0 + Duration::from_secs(20),
            TTL,
        );
        assert!(
            again.is_empty(),
            "the same window on the same board: {again:?}"
        );
        let moved = t.announce(
            "w1",
            "anna",
            "ws",
            Some("other"),
            t0 + Duration::from_secs(21),
            TTL,
        );
        assert_eq!(
            moved.into_iter().collect::<Vec<_>>(),
            vec![place("board"), place("other")]
        );
        let left = t.announce("w1", "anna", "ws", None, t0 + Duration::from_secs(22), TTL);
        assert_eq!(left.into_iter().collect::<Vec<_>>(), vec![place("other")]);
        assert!(t.is_empty());
    }

    #[test]
    fn a_silent_window_is_forgotten_after_the_time_to_live() {
        let mut t = Table::default();
        let t0 = Instant::now();
        t.announce("w1", "anna", "ws", Some("board"), t0, TTL);
        t.announce("w2", "bek", "ws", Some("board"), t0, TTL);
        // Anna keeps announcing; Bek's window went quiet.
        let changed = t.announce(
            "w1",
            "anna",
            "ws",
            Some("board"),
            t0 + TTL + Duration::from_secs(1),
            TTL,
        );
        assert_eq!(
            changed.into_iter().collect::<Vec<_>>(),
            vec![place("board")]
        );
        assert_eq!(t.users_at(&place("board")), vec!["anna"]);
    }

    #[test]
    fn workspaces_keep_their_boards_apart() {
        let mut t = Table::default();
        let t0 = Instant::now();
        t.announce("w1", "anna", "finance", Some("board"), t0, TTL);
        t.announce("w2", "bek", "legal", Some("board"), t0, TTL);
        assert_eq!(t.users_at(&place("board")), Vec::<String>::new());
        assert_eq!(
            t.users_at(&Place {
                workspace: "finance".into(),
                board: "board".into()
            }),
            vec!["anna"]
        );
    }
}
