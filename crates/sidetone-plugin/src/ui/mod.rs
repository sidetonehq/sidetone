//! Sidetone's screens. Each `build` function draws one window from the shared [`Model`].

pub mod atc;
pub mod clearance;
pub mod cpdlc;
pub mod events;
pub mod flight;
pub mod main_window;
pub mod panel;
pub mod setup;

use crate::app::{Action, Redacted};
use sidetone_core::settings::Settings;
use sidetone_core::state::{AppState, Message};
use sidetone_core::watch::Watcher;
use sidetone_services::hoppie::Session;
use sidetone_services::keychain::Secret;
use sidetone_services::simbrief::Plan;
use sidetone_ui::imgui::{InputTextFlags, Ui};
use sidetone_ui::theme;
use sidetone_vatsim::geo::LatLon;

pub struct Model {
    pub state: AppState,
    pub settings: Settings,
    pub actions: Vec<Action>,
    pub chat_input: String,
    pub watcher: Watcher,
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
    pub xpilot: crate::bridge::xpilot::XpilotBridge,
    /// The CID shown in Settings was found automatically.
    pub cid_detected: bool,
}

/// Transient widget state (search boxes, input fields).
#[derive(Default)]
pub struct UiState {
    pub atc_search: String,
    pub atc_show_all: bool,
    pub cid_input: String,
    /// A tab to bring to the front on the next frame (e.g. "See all" on the Flight tab).
    pub select_tab: Option<Tab>,
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
            xpilot: Default::default(),
            cid_detected: false,
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

    /// Push-to-talk was pressed, so it's bound: ticks the setup checklist (saved once).
    pub fn note_ptt_works(&mut self) {
        if self.state.ptt_pressed && !self.settings.setup.ptt_tested {
            self.settings.setup.ptt_tested = true;
            self.settings_dirty = true;
        }
    }

    pub fn recompute_route(&mut self) {
        let feed = self.state.network.snapshot.as_ref().map(|s| &s.feed);
        self.state.route = sidetone_core::watch::route(&self.settings, self.simbrief.as_deref(), feed);
    }
}

/// Main window tabs that other screens can switch to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Flight,
    Atc,
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

/// The VATSIM CID box; saved when you leave the field.
pub fn cid_field(ui: &Ui, m: &mut Model, label: &str) {
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    ui.set_next_item_width(120.0 * unit);
    ui.input_text(label, &mut m.ui.cid_input).hint("e.g. 1234567").build();
    if ui.is_item_deactivated_after_edit() {
        let input = m.ui.cid_input.trim();
        m.settings.vatsim.cid = input.parse().ok();
        if m.settings.vatsim.cid.is_none() && !input.is_empty() {
            m.ui.cid_input.clear();
        }
        m.actions.push(Action::SaveSettings);
    }
}

/// A Keychain-backed field: type and Save, or Remove once saved. The value never stays in the model.
pub fn secret_field(ui: &Ui, m: &mut Model, secret: Secret, label: &str, saved: bool, masked: bool) {
    let _id = ui.push_id(label);
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let buffer = match secret {
        Secret::SimbriefUsername => &mut m.ui.simbrief_user_input,
        Secret::HoppieLogon => &mut m.ui.hoppie_code_input,
    };
    ui.set_next_item_width(180.0 * unit);
    let flags = if masked { InputTextFlags::PASSWORD } else { InputTextFlags::NONE };
    let hint = match (saved, secret) {
        (true, _) => "Saved — type to replace",
        (false, Secret::SimbriefUsername) => "e.g. 123456",
        (false, Secret::HoppieLogon) => "",
    };
    let entered = ui.input_text(label, buffer).hint(hint).flags(flags).enter_returns_true(true).build();
    ui.same_line();
    if (ui.small_button("Save") || entered) && !buffer.trim().is_empty() {
        let value = std::mem::take(buffer);
        m.actions.push(Action::SaveSecret(secret, Redacted(value)));
    }
    if saved {
        ui.same_line();
        if ui.small_button("Remove") {
            m.actions.push(Action::DeleteSecret(secret));
        }
    }
}
