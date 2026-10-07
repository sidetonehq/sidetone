//! One background thread for SimBrief network calls.

use crate::simbrief;
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Update {
    SimBrief(Result<Box<simbrief::Plan>, String>),
}

#[derive(Debug)]
pub enum Request {
    FetchSimBrief { username: String },
}

enum Message {
    Request(Request),
    Stop,
}

pub struct Worker {
    tx: Sender<Message>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// `on_update` runs on the worker thread and must not touch X-Plane.
    pub fn spawn(on_update: impl Fn(Update) + Send + 'static) -> Worker {
        let (tx, rx) = unbounded();
        let handle = std::thread::Builder::new().name("sidetone-services".into()).spawn(move || run(rx, on_update)).expect("spawn services worker");
        Worker { tx, handle: Some(handle) }
    }

    pub fn request(&self, request: Request) {
        let _ = self.tx.send(Message::Request(request));
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(rx: Receiver<Message>, on_update: impl Fn(Update)) {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).user_agent(concat!("Sidetone/", env!("CARGO_PKG_VERSION"))).build().into();
    while let Ok(Message::Request(request)) = rx.recv() {
        match request {
            Request::FetchSimBrief { username } => {
                // SimBrief reports problems as HTTP 400 with a JSON message; read it instead
                // of failing on the status. A numeric entry is a Pilot ID, not a username.
                let (param, value) = simbrief::lookup_param(&username);
                let result = agent
                    .get(simbrief::FETCH_URL)
                    .query(param, value)
                    .query("json", "1")
                    .config()
                    .http_status_as_error(false)
                    .build()
                    .call()
                    .map_err(|e| format!("SimBrief: {e}"))
                    .and_then(|mut r| r.body_mut().with_config().limit(32 * 1024 * 1024).read_to_string().map_err(|e| format!("SimBrief: {e}")))
                    .and_then(|body| simbrief::parse(&body).map(Box::new));
                on_update(Update::SimBrief(result));
            }
        }
    }
}
