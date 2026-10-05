//! xPilot companion mode.
//!
//! Reads the datarefs the xPilot plugin publishes (connection, callsign, stations, receive
//! activity, SELCAL, traffic count) and forwards Sidetone's push-to-talk to `xpilot/ptt`.
//! It is strictly read-only towards the network: Sidetone never sends traffic through xPilot,
//! which would make it an unapproved client by proxy.

use sidetone_core::state::{AppState, Connection};
use sidetone_xplm::command::Command;
use sidetone_xplm::dataref::DataRef;

/// How often to look for the xPilot plugin when it isn't loaded (it may load after us).
const DISCOVERY_INTERVAL: f32 = 5.0;

struct Refs {
    status: DataRef,
    callsign: DataRef,
    com1_rx: Option<DataRef>,
    com2_rx: Option<DataRef>,
    selcal: Option<DataRef>,
    aircraft: Option<DataRef>,
    ptt: Option<Command>,
    /// Set by xPilot while its push-to-talk is held through X-Plane.
    transmitting: Option<DataRef>,
}

impl Refs {
    fn find() -> Option<Refs> {
        Some(Refs {
            status: DataRef::find("xpilot/login/status")?,
            callsign: DataRef::find("xpilot/login/callsign")?,
            com1_rx: DataRef::find("xpilot/audio/com1_rx"),
            com2_rx: DataRef::find("xpilot/audio/com2_rx"),
            selcal: DataRef::find("xpilot/selcal_received"),
            aircraft: DataRef::find("xpilot/num_aircraft"),
            ptt: Command::find("xpilot/ptt"),
            transmitting: DataRef::find("xpilot/ptt"),
        })
    }
}

#[derive(Default)]
pub struct XpilotBridge {
    refs: Option<Refs>,
    next_discovery: f32,
    ptt_held: bool,
    selcal_was: bool,
    was_connected: bool,
}

/// What changed this tick that the pilot should hear about.
pub enum Notice {
    Connected(String),
    Disconnected,
    Selcal,
}

impl XpilotBridge {
    pub fn detected(&self) -> bool {
        self.refs.is_some()
    }

    /// Mirrors xPilot's state into `state` and forwards PTT. Call from the flight loop while the
    /// mode is on. Returns notices for the message ticker.
    pub fn tick(&mut self, state: &mut AppState, now: f32) -> Vec<Notice> {
        if self.refs.is_none() && now >= self.next_discovery {
            self.next_discovery = now + DISCOVERY_INTERVAL;
            self.refs = Refs::find();
            if self.refs.is_some() {
                log::info!("xPilot plugin found; companion mode active");
            }
        }
        let Some(refs) = &self.refs else {
            state.connection_via = None;
            return Vec::new();
        };
        let mut notices = Vec::new();

        let status = refs.status.get_i32();
        let callsign = refs.callsign.get_string();
        let connected = status != 0 && !callsign.is_empty();
        state.connection = if connected { Connection::Connected { callsign: callsign.clone() } } else { Connection::Disconnected };
        state.connection_via = Some(if status == 2 { "xPilot observer" } else { "xPilot" });
        if connected != self.was_connected {
            notices.push(if connected { Notice::Connected(callsign) } else { Notice::Disconnected });
            self.was_connected = connected;
        }

        let flag = |r: &Option<DataRef>| r.is_some_and(|r| r.get_i32() != 0);
        state.com1.receiving = flag(&refs.com1_rx);
        state.com2.receiving = flag(&refs.com2_rx);
        state.nearby_aircraft = refs.aircraft.map(|r| r.get_i32().max(0) as u32);
        state.transmitting = state.ptt_pressed || (connected && flag(&refs.transmitting));

        let selcal = flag(&refs.selcal);
        if selcal && !self.selcal_was {
            notices.push(Notice::Selcal);
        }
        self.selcal_was = selcal;

        // One push-to-talk for both: hold xPilot's PTT while Sidetone's is held.
        if let Some(ptt) = refs.ptt
            && state.ptt_pressed != self.ptt_held
        {
            if state.ptt_pressed {
                ptt.begin()
            } else {
                ptt.end()
            }
            self.ptt_held = state.ptt_pressed;
        }
        notices
    }

    /// Releases anything we hold and clears mirrored state (mode switched off or plugin stopping).
    pub fn release(&mut self, state: &mut AppState) {
        if self.ptt_held
            && let Some(ptt) = self.refs.as_ref().and_then(|r| r.ptt)
        {
            ptt.end();
        }
        self.ptt_held = false;
        if state.connection_via.is_some() {
            state.connection = Connection::Disconnected;
            state.connection_via = None;
            state.nearby_aircraft = None;
            state.com1.receiving = false;
            state.com2.receiving = false;
        }
        self.was_connected = false;
    }
}
