//! The one Core of this server, on its own thread, and the pump that routes
//! its answers to their callers and its events to their users.
//!
//! `Hub` hands out answers and events from one queue. A handler that drained
//! it would swallow the answers to everyone else's requests and every event a
//! socket was waiting for, so nobody polls it but the pump. Requests register
//! a one-shot before they are sent; the pump completes them by id, and sends
//! each event to the user it is for — or to every user, when it is everyone's.

use localspace_core::identity::Directory;
use localspace_core::transport::{Hub, Outgoing};
use localspace_core::{Caller, Core, To};
use localspace_proto as proto;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, broadcast, oneshot};

/// How long a request may take before the caller is told so. Model turns
/// stream their progress as events and answer at the end, so this is a
/// ceiling on the whole turn, not on a token.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// Events a user's sockets may fall behind by before they are told to
/// refresh.
const EVENTS_PER_USER: usize = 1024;

pub struct Session {
    /// Sessions are read here on every request, off Core's thread.
    pub directory: Directory,
    backend: Arc<Hub>,
    pending: Mutex<HashMap<u64, oneshot::Sender<proto::Response>>>,
    /// One channel per user who has asked for events; a shared event goes
    /// to all of them.
    users: Mutex<HashMap<String, broadcast::Sender<proto::Event>>>,
    wake: Arc<Notify>,
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("Core did not answer within {} s", CALL_TIMEOUT.as_secs())]
    Timeout,
    #[error("the session is closed")]
    Closed,
}

impl Session {
    pub fn spawn(core: Core) -> Arc<Session> {
        let directory = core.directory();
        let backend = Arc::new(Hub::spawn(core));
        let wake = Arc::new(Notify::new());
        {
            let wake = wake.clone();
            backend.set_wake(Box::new(move || wake.notify_one()));
        }
        let session = Arc::new(Session {
            directory,
            backend,
            pending: Mutex::new(HashMap::new()),
            users: Mutex::new(HashMap::new()),
            wake,
        });
        tokio::spawn(pump(session.clone()));
        session
    }

    /// Send a request as `caller` and wait for its response.
    pub async fn call_as(
        &self,
        caller: &Caller,
        req: proto::Request,
    ) -> Result<proto::Response, CallError> {
        let (tx, rx) = oneshot::channel();
        let id = {
            // Registered under the same lock the pump takes, so an answer that
            // arrives before this returns still finds its sender.
            let mut pending = self.pending.lock().unwrap();
            let id = self.backend.request_as(caller, req);
            pending.insert(id, tx);
            id
        };
        match tokio::time::timeout(CALL_TIMEOUT, rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(CallError::Closed),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(CallError::Timeout)
            }
        }
    }

    /// Every event for `user` from now on: theirs, and everyone's. A slow
    /// reader that falls `EVENTS_PER_USER` behind is told so by the channel
    /// and reconnects.
    pub fn subscribe(&self, user: &str) -> broadcast::Receiver<proto::Event> {
        let mut users = self.users.lock().unwrap();
        users
            .entry(user.to_string())
            .or_insert_with(|| broadcast::channel(EVENTS_PER_USER).0)
            .subscribe()
    }

    /// Users who have asked for events since the server started.
    pub fn user_count(&self) -> usize {
        self.users.lock().unwrap().len()
    }
}

async fn pump(session: Arc<Session>) {
    loop {
        for msg in session.backend.poll() {
            match msg {
                Outgoing::Response { id, response } => {
                    let sender = session.pending.lock().unwrap().remove(&id);
                    if let Some(tx) = sender {
                        let _ = tx.send(response);
                    }
                }
                Outgoing::Event { to, event } => {
                    // No subscriber is not an error: events are advisory.
                    let users = session.users.lock().unwrap();
                    match to {
                        To::User(user) => {
                            if let Some(tx) = users.get(&user) {
                                let _ = tx.send(event);
                            }
                        }
                        To::All => {
                            for tx in users.values() {
                                let _ = tx.send(event.clone());
                            }
                        }
                    }
                }
            }
        }
        // Core wakes the pump when it has something; the sleep is only a
        // backstop against a wake lost in a race.
        tokio::select! {
            _ = session.wake.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use localspace_core::Config;
    use tokio::sync::broadcast::error::TryRecvError;

    fn member(name: &str) -> Caller {
        Caller {
            user: name.into(),
            session: format!("s_{name}"),
            ip: String::new(),
            roles: vec![proto::UserRole::Member],
            groups: Vec::new(),
        }
    }

    fn drain(rx: &mut broadcast::Receiver<proto::Event>) -> Vec<proto::Event> {
        let mut out = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(ev) => out.push(ev),
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => return out,
                Err(TryRecvError::Lagged(_)) => continue,
            }
        }
    }

    #[tokio::test]
    async fn events_go_to_their_user_and_shared_ones_to_everyone() {
        let core = Core::new(Config::personal("anna")).unwrap();
        let session = Session::spawn(core);
        let anna = member("anna");
        let mut for_anna = session.subscribe("anna");
        let mut for_ben = session.subscribe("ben");

        // Anna's turn: her events, none of them Ben's. The message is answered
        // beside Core's queue, so its end comes after the call returns.
        session
            .call_as(
                &anna,
                proto::Request::SendMessage {
                    text: "hello".into(),
                    conversation: None,
                },
            )
            .await
            .unwrap();
        let mut annas = Vec::new();
        for _ in 0..200 {
            annas.extend(drain(&mut for_anna));
            if annas
                .iter()
                .any(|ev| matches!(ev, proto::Event::AssistantDone { .. }))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            annas
                .iter()
                .any(|ev| matches!(ev, proto::Event::AssistantDone { .. })),
            "{annas:?}"
        );
        assert!(
            drain(&mut for_ben).is_empty(),
            "Ben heard nothing of Anna's turn"
        );

        // A shared change, which is an administrator's to make: everyone is
        // told their view is stale, and only the caller gets the environment.
        let ben_the_admin = Caller {
            roles: vec![proto::UserRole::Admin],
            ..member("ben")
        };
        session
            .call_as(
                &ben_the_admin,
                proto::Request::SetNetworkMode {
                    mode: proto::NetworkMode::Airgapped,
                },
            )
            .await
            .unwrap();
        let annas = drain(&mut for_anna);
        let bens = drain(&mut for_ben);
        assert!(
            annas
                .iter()
                .any(|ev| matches!(ev, proto::Event::EnvironmentOutdated)),
            "{annas:?}"
        );
        assert!(
            !annas
                .iter()
                .any(|ev| matches!(ev, proto::Event::EnvironmentChanged(_))),
            "Anna does not get Ben's view: {annas:?}"
        );
        assert!(
            bens.iter()
                .any(|ev| matches!(ev, proto::Event::EnvironmentChanged(env) if env.user == "ben")),
            "{bens:?}"
        );
        assert_eq!(session.user_count(), 2);
    }
}
