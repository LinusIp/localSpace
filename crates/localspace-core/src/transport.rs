//! The `Backend` trait and its two implementations.
//!
//! The Client talks to Core through this and nothing else. `InProcess` moves typed
//! values over channels with no serialisation at all (§16.3); the WebSocket
//! implementation lives in `localspace-server` and the browser Client and speaks
//! exactly the same `proto` types.
//!
//! The shape is poll-based rather than the `async fn call` the spec sketches,
//! because the Client is immediate-mode and also compiles to wasm, where blocking
//! on a future inside a frame is not possible. Responses and events arrive on one
//! ordered channel, which is what keeps a repaint cheap: drain, apply, draw.

use localspace_proto as proto;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum Incoming {
    Response { id: u64, response: proto::Response },
    Event(proto::Event),
}

/// Called from the Core side whenever something is waiting in `poll`.
pub type Wake = Box<dyn Fn() + Send + Sync>;

pub trait Backend: Send + Sync {
    /// Queue a request. The reply arrives from `poll` carrying this id.
    fn request(&self, req: proto::Request) -> u64;
    /// Non-blocking drain of everything that has arrived since the last call.
    fn poll(&self) -> Vec<Incoming>;
    /// True while the connection is usable.
    fn connected(&self) -> bool {
        true
    }
    /// Install a callback that fires when a response or event arrives, so an
    /// idle Client can sleep until there is something to draw rather than poll.
    /// A transport that cannot wake anyone keeps the default and the Client
    /// falls back to polling.
    fn set_wake(&self, _wake: Wake) {}
}

/// Desktop transport: Core on its own thread, typed values over channels.
pub struct InProcess {
    to_core: Sender<(u64, proto::Request)>,
    from_core: Mutex<Receiver<Incoming>>,
    next_id: AtomicU64,
    wake: Arc<Mutex<Option<Wake>>>,
}

fn notify(wake: &Mutex<Option<Wake>>) {
    if let Some(w) = wake.lock().unwrap().as_ref() {
        w();
    }
}

impl InProcess {
    /// Take ownership of a Core and run it on its own thread.
    ///
    /// One thread per environment: tool routing, permission checks and DAG commits
    /// are synchronous in-memory operations there, and the only I/O on the hot path
    /// is the append to the commit log.
    pub fn spawn(mut core: crate::Core) -> InProcess {
        let (to_core, core_rx) = std::sync::mpsc::channel::<(u64, proto::Request)>();
        let (core_tx, from_core) = std::sync::mpsc::channel::<Incoming>();
        let wake: Arc<Mutex<Option<Wake>>> = Arc::new(Mutex::new(None));

        let event_tx = core_tx.clone();
        let event_wake = wake.clone();
        core.set_event_sink(Box::new(move |ev| {
            let _ = event_tx.send(Incoming::Event(ev));
            notify(&event_wake);
        }));

        let thread_wake = wake.clone();
        std::thread::Builder::new()
            .name("localspace-core".into())
            .spawn(move || {
                while let Ok((id, req)) = core_rx.recv() {
                    let response = core.handle(req);
                    if core_tx.send(Incoming::Response { id, response }).is_err() {
                        break;
                    }
                    notify(&thread_wake);
                }
            })
            .expect("spawning the Core thread");

        InProcess {
            to_core,
            from_core: Mutex::new(from_core),
            next_id: AtomicU64::new(1),
            wake,
        }
    }
}

impl Backend for InProcess {
    fn request(&self, req: proto::Request) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let _ = self.to_core.send((id, req));
        id
    }

    fn poll(&self) -> Vec<Incoming> {
        let rx = self.from_core.lock().unwrap();
        rx.try_iter().collect()
    }

    fn set_wake(&self, wake: Wake) {
        *self.wake.lock().unwrap() = Some(wake);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_hook_fires_when_something_arrives() {
        // What lets an idle Client sleep: it does not poll on a timer, Core
        // taps it on the shoulder.
        use std::sync::atomic::AtomicUsize;
        let core = crate::Core::ephemeral("tester").unwrap();
        let backend = InProcess::spawn(core);

        let woken = Arc::new(AtomicUsize::new(0));
        let counter = woken.clone();
        backend.set_wake(Box::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));

        assert_eq!(woken.load(Ordering::SeqCst), 0, "nothing has arrived yet");
        backend.request(proto::Request::GetEnvironment);
        for _ in 0..200 {
            if woken.load(Ordering::SeqCst) > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(woken.load(Ordering::SeqCst) >= 1, "the reply arrived without a wake");
        assert!(!backend.poll().is_empty(), "and it is waiting in poll");
    }

    #[test]
    fn a_request_gets_its_own_response_back() {
        let core = crate::Core::ephemeral("tester").unwrap();
        let backend = InProcess::spawn(core);

        let id = backend.request(proto::Request::GetEnvironment);
        // The Core thread is asynchronous; drain until the reply lands.
        let mut env = None;
        for _ in 0..200 {
            for msg in backend.poll() {
                if let Incoming::Response { id: got, response } = msg {
                    assert_eq!(got, id);
                    env = Some(response);
                }
            }
            if env.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        match env.expect("no response from Core") {
            proto::Response::Environment(e) => assert_eq!(e.user, "tester"),
            other => panic!("wrong response: {other:?}"),
        }
    }
}
