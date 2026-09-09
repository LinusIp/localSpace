//! One `Session` per user: a Core on its own thread, a pump that routes its
//! answers, and a broadcast of its events.
//!
//! `InProcess` hands out responses and events from one queue. A REST handler
//! that drained it would swallow the answers to everyone else's requests and
//! every event a socket was waiting for, so nobody polls it but the pump.
//! Requests register a one-shot before they are sent; the pump completes them
//! by id and fans events out to every open socket.

use localspace_core::transport::{Backend, InProcess, Incoming};
use localspace_core::Core;
use localspace_proto as proto;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{broadcast, oneshot, Notify};

/// How long a request may take before the caller is told so. Model turns
/// stream their progress as events and answer at the end, so this is a
/// ceiling on the whole turn, not on a token.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(300);

pub struct Session {
    backend: Arc<InProcess>,
    pending: Mutex<HashMap<u64, oneshot::Sender<proto::Response>>>,
    events: broadcast::Sender<proto::Event>,
    wake: Arc<Notify>,
    pub user: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CallError {
    #[error("Core did not answer within {} s", CALL_TIMEOUT.as_secs())]
    Timeout,
    #[error("the session is closed")]
    Closed,
}

impl Session {
    pub fn spawn(user: &str, core: Core) -> Arc<Session> {
        let backend = Arc::new(InProcess::spawn(core));
        let wake = Arc::new(Notify::new());
        {
            let wake = wake.clone();
            backend.set_wake(Box::new(move || wake.notify_one()));
        }
        let (events, _) = broadcast::channel(1024);
        let session = Arc::new(Session {
            backend,
            pending: Mutex::new(HashMap::new()),
            events,
            wake,
            user: user.to_string(),
        });
        tokio::spawn(pump(session.clone()));
        session
    }

    /// Send a request and wait for its response.
    pub async fn call(&self, req: proto::Request) -> Result<proto::Response, CallError> {
        let (tx, rx) = oneshot::channel();
        let id = {
            // Registered under the same lock the pump takes, so an answer that
            // arrives before this returns still finds its sender.
            let mut pending = self.pending.lock().unwrap();
            let id = self.backend.request(req);
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

    /// Every event Core emits from now on. A slow reader that falls 1024
    /// events behind is told so by the channel and reconnects.
    pub fn subscribe(&self) -> broadcast::Receiver<proto::Event> {
        self.events.subscribe()
    }
}

async fn pump(session: Arc<Session>) {
    loop {
        for msg in session.backend.poll() {
            match msg {
                Incoming::Response { id, response } => {
                    let sender = session.pending.lock().unwrap().remove(&id);
                    if let Some(tx) = sender {
                        let _ = tx.send(response);
                    }
                }
                Incoming::Event(event) => {
                    // No subscriber is not an error: events are advisory.
                    let _ = session.events.send(event);
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
