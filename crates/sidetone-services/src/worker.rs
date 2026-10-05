//! One background thread for Hoppie and SimBrief network calls.
//!
//! Hoppie etiquette (from the Hoppie technical docs): don't poll until the aircraft has sent a
//! message; then poll at a random 45–75 s interval, expedited to 20 s for a short while after
//! sending. Requests go as form POSTs so the logon code never appears in a URL.

use crate::hoppie::CONNECT_URL;
use crate::hoppie::protocol::{Incoming, Outgoing, parse_response};
use crate::simbrief;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

const EXPEDITED: Duration = Duration::from_secs(20);
const EXPEDITED_POLLS: u32 = 4;

#[derive(Clone, Debug)]
pub enum Update {
    /// Messages received by a poll (may be empty).
    Received(Vec<Incoming>),
    /// Hoppie rejected or failed a request; `Ok` when it recovers.
    HoppieStatus(Result<(), String>),
    SimBrief(Result<Box<simbrief::Plan>, String>),
}

#[derive(Debug)]
pub enum Request {
    /// Sets (or clears) the Hoppie identity. Changing callsign stops polling until the next send.
    ConfigureHoppie {
        logon: Option<String>,
        callsign: String,
    },
    Send(Outgoing),
    FetchSimBrief {
        username: String,
    },
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

/// Random-enough 45–75 s without a dependency: the clock's sub-second noise.
fn normal_interval() -> Duration {
    let nanos = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    Duration::from_secs(45 + (nanos % 31) as u64)
}

struct Hoppie {
    agent: ureq::Agent,
    logon: Option<String>,
    callsign: String,
    /// Polling starts after the first successful send.
    polling: bool,
    next_poll: Option<Instant>,
    expedited_left: u32,
    failing: bool,
}

impl Hoppie {
    fn call(&self, to: &str, kind: &str, packet: &str) -> Result<Vec<Incoming>, String> {
        let logon = self.logon.as_deref().ok_or("No Hoppie logon code set")?;
        if self.callsign.is_empty() {
            return Err("No callsign set".into());
        }
        let form = [("logon", logon), ("from", self.callsign.as_str()), ("to", to), ("type", kind), ("packet", packet)];
        let mut response = self.agent.post(CONNECT_URL).send_form(form).map_err(|e| format!("Hoppie: {e}"))?;
        let body = response.body_mut().read_to_string().map_err(|e| format!("Hoppie: {e}"))?;
        parse_response(&body)
    }

    fn schedule_after_send(&mut self) {
        self.polling = true;
        self.expedited_left = EXPEDITED_POLLS;
        self.next_poll = Some(Instant::now() + EXPEDITED);
    }

    fn schedule_next(&mut self) {
        let wait = if self.expedited_left > 0 {
            self.expedited_left -= 1;
            EXPEDITED
        } else {
            normal_interval()
        };
        self.next_poll = Some(Instant::now() + wait);
    }

    fn report(&mut self, result: &Result<Vec<Incoming>, String>, on_update: &impl Fn(Update)) {
        match result {
            Ok(_) if self.failing => {
                self.failing = false;
                on_update(Update::HoppieStatus(Ok(())));
            }
            Err(e) => {
                if !self.failing {
                    log::warn!("{e}");
                }
                self.failing = true;
                on_update(Update::HoppieStatus(Err(e.clone())));
            }
            _ => {}
        }
    }
}

fn run(rx: Receiver<Message>, on_update: impl Fn(Update)) {
    let agent: ureq::Agent =
        ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).user_agent(concat!("Sidetone/", env!("CARGO_PKG_VERSION"))).build().into();
    let mut hoppie = Hoppie { agent: agent.clone(), logon: None, callsign: String::new(), polling: false, next_poll: None, expedited_left: 0, failing: false };

    loop {
        if hoppie.polling && hoppie.next_poll.is_some_and(|t| Instant::now() >= t) {
            let result = hoppie.call("SERVER", "poll", "");
            hoppie.report(&result, &on_update);
            if let Ok(messages) = result
                && !messages.is_empty()
            {
                on_update(Update::Received(messages));
            }
            hoppie.schedule_next();
        }

        let timeout = hoppie.next_poll.filter(|_| hoppie.polling).map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_secs(3600));
        match rx.recv_timeout(timeout) {
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
            Ok(Message::Request(Request::ConfigureHoppie { logon, callsign })) => {
                let callsign = callsign.trim().to_ascii_uppercase();
                if callsign != hoppie.callsign || logon != hoppie.logon {
                    hoppie.polling = false;
                    hoppie.next_poll = None;
                }
                hoppie.logon = logon;
                hoppie.callsign = callsign;
            }
            Ok(Message::Request(Request::Send(out))) => {
                let result = hoppie.call(&out.to, out.kind.wire(), &out.packet);
                hoppie.report(&result, &on_update);
                if result.is_ok() {
                    hoppie.schedule_after_send();
                }
            }
            Ok(Message::Request(Request::FetchSimBrief { username })) => {
                let result = agent
                    .get(simbrief::FETCH_URL)
                    .query("username", &username)
                    .query("json", "1")
                    .call()
                    .map_err(|e| format!("SimBrief: {e}"))
                    .and_then(|mut r| r.body_mut().with_config().limit(32 * 1024 * 1024).read_to_string().map_err(|e| format!("SimBrief: {e}")))
                    .and_then(|body| simbrief::parse(&body).map(Box::new));
                on_update(Update::SimBrief(result));
            }
        }
    }
}
