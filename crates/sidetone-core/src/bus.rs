//! Worker threads never touch the sim. They post [`Event`]s here; the main thread drains
//! them once per flight loop and applies them to [`crate::state::AppState`].

use crossbeam_channel::{Receiver, Sender, unbounded};

#[derive(Clone, Debug)]
pub enum Event {
    /// A log line that should also appear in X-Plane's Log.txt.
    SimLog(String),
    /// Fresh public VATSIM data from the worker thread.
    Vatsim(sidetone_vatsim::worker::Update),
    /// Hoppie / SimBrief results from the services worker.
    Services(sidetone_services::worker::Update),
}

#[derive(Clone)]
pub struct BusSender(Sender<Event>);

impl BusSender {
    /// Never blocks. Dropped silently if the plugin is shutting down.
    pub fn send(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

pub struct Bus {
    sender: Sender<Event>,
    receiver: Receiver<Event>,
}

impl Bus {
    pub fn new() -> Bus {
        let (sender, receiver) = unbounded();
        Bus { sender, receiver }
    }

    pub fn sender(&self) -> BusSender {
        BusSender(self.sender.clone())
    }

    /// Returns everything queued so far, without blocking.
    pub fn drain(&self) -> impl Iterator<Item = Event> + '_ {
        self.receiver.try_iter()
    }
}

impl Default for Bus {
    fn default() -> Self {
        Bus::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivers_from_other_threads() {
        let bus = Bus::new();
        let tx = bus.sender();
        std::thread::spawn(move || tx.send(Event::SimLog("hi".into()))).join().unwrap();
        let events: Vec<_> = bus.drain().collect();
        assert!(matches!(&events[..], [Event::SimLog(s)] if s == "hi"));
        assert_eq!(bus.drain().count(), 0);
    }
}
