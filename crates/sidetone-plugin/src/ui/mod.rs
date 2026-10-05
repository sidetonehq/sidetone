//! Sidetone's screens. Each `build` function draws one window from the shared [`Model`].

pub mod atc;
pub mod clearance;
pub mod cpdlc;
pub mod events;
pub mod flight_plan;
pub mod main_window;
pub mod panel;

use crate::app::Action;
use sidetone_core::settings::Settings;
use sidetone_core::state::{AppState, Message};
use sidetone_core::watch::{FriendStatus, Watcher};
use sidetone_services::hoppie::Session;
use sidetone_services::simbrief::Plan;
use sidetone_vatsim::geo::LatLon;

pub struct Model {
    pub state: AppState,
    pub settings: Settings,
    pub actions: Vec<Action>,
    pub chat_input: String,
    pub watcher: Watcher,
    pub friend_statuses: Vec<FriendStatus>,
    pub nearest_checked_at: Option<LatLon>,
    pub requested_airports: Vec<String>,
    pub requested_cid: Option<u32>,
    pub ui: UiState,
    pub simbrief: Option<Box<Plan>>,
    /// "Importing…" or the last error.
    pub simbrief_status: Option<String>,
    pub hoppie: Session,
    /// Whether a logon code is in the Keychain (the code itself never sits in the model).
    pub hoppie_ready: bool,
    pub hoppie_error: Option<String>,
    pub simbrief_user_saved: bool,
    /// Callsign the services worker was last configured with.
    pub hoppie_configured_callsign: String,
    pub requested_route: sidetone_vatsim::worker::RouteQuery,
    /// Settings changed outside an explicit action; saved on the next loop.
    pub settings_dirty: bool,
    /// Cached, filtered and sorted ATC list (rebuilt at most once a second).
    pub atc_rows: Vec<(usize, Option<f64>)>,
    pub atc_rows_key: (usize, String, bool, i64),
    pub was_vr: bool,
}

/// Transient widget state (search boxes, input fields).
#[derive(Default)]
pub struct UiState {
    pub atc_search: String,
    pub atc_show_all: bool,
    pub cid_input: String,
    pub friend_input: String,
    pub friend_label: String,
    pub hoppie_code_input: String,
    pub simbrief_user_input: String,
    pub cpdlc_callsign: String,
    pub cpdlc_station: String,
    pub cpdlc_text: String,
    pub cpdlc_level: String,
    pub cpdlc_direct: String,
    pub pdc_stand: String,
}

impl Model {
    pub fn new(settings: Settings) -> Model {
        let cid_input = settings.vatsim.cid.map(|c| c.to_string()).unwrap_or_default();
        Model {
            state: AppState::default(),
            settings,
            actions: Vec::new(),
            chat_input: String::new(),
            watcher: Watcher::default(),
            friend_statuses: Vec::new(),
            nearest_checked_at: None,
            requested_airports: Vec::new(),
            requested_cid: None,
            ui: UiState { cid_input, ..Default::default() },
            simbrief: None,
            simbrief_status: None,
            hoppie: Session::default(),
            hoppie_ready: false,
            hoppie_error: None,
            simbrief_user_saved: false,
            hoppie_configured_callsign: String::new(),
            requested_route: Default::default(),
            settings_dirty: false,
            atc_rows: Vec::new(),
            atc_rows_key: (0, String::new(), false, 0),
            was_vr: false,
        }
    }

    /// Adds a local notice to the chat (not counted as unread).
    pub fn system_message(&mut self, text: impl Into<String>, now: f32) {
        self.state.messages.push(Message { from: "Sidetone".into(), text: text.into(), received_at: now });
    }
}

impl Model {
    /// Callsign for datalink: typed override, else SimBrief, else your live VATSIM connection.
    pub fn datalink_callsign(&self) -> String {
        let typed = self.ui.cpdlc_callsign.trim().to_ascii_uppercase();
        if !typed.is_empty() {
            return typed;
        }
        if let Some(plan) = &self.simbrief
            && !plan.callsign.is_empty()
        {
            return plan.callsign.to_ascii_uppercase();
        }
        if let (Some(cid), Some(snapshot)) = (self.settings.vatsim.cid, &self.state.network.snapshot)
            && let Some(p) = snapshot.feed.pilots.iter().find(|p| p.cid == cid)
        {
            return p.callsign.clone();
        }
        String::new()
    }

    pub fn recompute_route(&mut self) {
        let feed = self.state.network.snapshot.as_ref().map(|s| &s.feed);
        self.state.route = sidetone_core::watch::route(&self.settings, self.simbrief.as_deref(), feed);
    }
}

/// A section heading: small dim caps with breathing room, used on every tab for consistency.
pub fn section(ui: &sidetone_ui::imgui::Ui, title: &str) {
    ui.spacing();
    ui.spacing();
    ui.text_disabled(title);
    ui.separator();
}

/// "COM1" / "COM2" buttons that tune `khz`; the radio already on it is highlighted.
pub fn tune_buttons(ui: &sidetone_ui::imgui::Ui, m: &mut Model, khz: i32) {
    use sidetone_core::dot_command::Com;
    use sidetone_ui::imgui::StyleColor;
    let _id = ui.push_id(khz as usize);
    for (com, label, active) in [(Com::One, "COM1", m.state.com1.active_khz), (Com::Two, "COM2", m.state.com2.active_khz)] {
        let tuned = active == khz;
        let _c = tuned.then(|| ui.push_style_color(StyleColor::Button, [0.20, 0.78, 0.72, 0.45]));
        if ui.small_button(label) && !tuned {
            m.actions.push(Action::Tune { com, khz });
        }
        if com == Com::One {
            ui.same_line();
        }
    }
}
