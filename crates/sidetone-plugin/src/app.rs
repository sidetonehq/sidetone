//! Wires state, settings, windows, commands and the flight loop together.

use crate::sim::Sim;
use crate::ui::{self, Model};
use sidetone_core::bus::BusSender;
use sidetone_core::bus::Event;
use sidetone_core::clearance::{self, FlightNotes};
use sidetone_core::dot_command::{self, Com, DotCommand};
use sidetone_core::layout::{self, Bounds};
use sidetone_core::settings::Settings;
use sidetone_core::state::CoverageHint;
use sidetone_core::state::TunedStation;
use sidetone_services::hoppie::session::Notice;
use sidetone_services::keychain::{self, Secret};
use sidetone_services::worker as services;
use sidetone_ui::ImguiWindow;
use sidetone_ui::WindowKind;
use sidetone_vatsim::coverage;
use sidetone_vatsim::freq::UNICOM_KHZ;
use sidetone_vatsim::geo::LatLon;
use sidetone_vatsim::stations::{self, radio_horizon_nm};
use sidetone_vatsim::worker::{Request, RouteQuery, Update, Worker};
use sidetone_xplm::command::{Command, CommandHandler, Phase};
use sidetone_xplm::flight_loop::FlightLoop;
use sidetone_xplm::graphics;
use sidetone_xplm::menu::PluginMenu;
use sidetone_xplm::window::{Positioning, Rect};
use sidetone_xplm::{elapsed_time, paths};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Things the UI asks the app to do; applied in the flight loop, outside any draw callback.
#[derive(Debug)]
pub enum Action {
    TogglePanel,
    ToggleMainWindow,
    SubmitChat(String),
    Tune { com: Com, khz: i32 },
    SaveSecret(Secret, Redacted),
    DeleteSecret(Secret),
    ImportSimBrief,
    Hoppie(HoppieAction),
    ResetPanelPosition,
    SaveSettings,
}

/// A secret in transit from the UI to the Keychain. Its Debug output never shows the value,
/// so action logging can't leak it.
pub struct Redacted(pub String);

impl std::fmt::Debug for Redacted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// CPDLC/ACARS operations requested by the UI.
#[derive(Debug)]
pub enum HoppieAction {
    Logon(String),
    Logoff,
    Reply { id: u64, reply: String },
    Request(String),
    Pdc { station: String, stand: String },
}

pub const PANEL_SIZE: (i32, i32) = (440, 84);

/// The flight loop runs at 20 Hz, not every frame: nothing here needs per-frame precision
/// (drawing happens in the window callbacks; PTT is handled directly by the command).
const LOOP_INTERVAL: f32 = 0.05;
const PANEL_MARGIN: i32 = 12;

const MENU_PANEL: usize = 0;
const MENU_WINDOW: usize = 1;

pub struct App {
    model: Rc<RefCell<Model>>,
    // Dropped in declaration order: callbacks first, then windows, then the worker thread.
    _flight_loop: FlightLoop,
    _handlers: Vec<CommandHandler>,
    _menu: Rc<RefCell<Option<PluginMenu>>>,
    panel: Rc<ImguiWindow>,
    main_window: Rc<ImguiWindow>,
    _worker: Rc<Worker>,
    _services: Rc<services::Worker>,
    settings_path: PathBuf,
}

fn screen() -> Bounds {
    let (left, top, right, bottom) = graphics::screen_bounds();
    Bounds { left, top, right, bottom }
}

fn to_rect(b: Bounds) -> Rect {
    Rect { left: b.left, top: b.top, right: b.right, bottom: b.bottom }
}

fn panel_rect(settings: &Settings) -> Rect {
    let scale = settings.ui.font_scale;
    let (w, h) = ((PANEL_SIZE.0 as f32 * scale) as i32, (PANEL_SIZE.1 as f32 * scale) as i32);
    let bounds = match settings.panel.position {
        Some((left, top)) => layout::place_clamped(screen(), left, top, w, h),
        None => layout::panel_default(screen(), w, h, PANEL_MARGIN),
    };
    to_rect(bounds)
}

fn main_window_rect(settings: &Settings) -> Rect {
    let (w, h) = settings.main_window.size;
    let bounds = match settings.main_window.position {
        Some((left, top)) => layout::place_clamped(screen(), left, top, w, h),
        None => layout::centered(screen(), w, h),
    };
    to_rect(bounds)
}

impl App {
    pub fn new(bus: BusSender) -> App {
        let settings_path = paths::preferences_dir().join("Sidetone.toml");
        let settings = Settings::load(&settings_path);
        log::info!("Settings: {}", settings_path.display());

        let panel_visible = settings.panel.visible;
        let panel_at = panel_rect(&settings);
        let window_at = main_window_rect(&settings);
        let model = Rc::new(RefCell::new(Model::new(settings)));

        let panel = {
            let model = model.clone();
            Rc::new(ImguiWindow::new(WindowKind::Panel, panel_at, panel_visible, move |frame| ui::panel::build(frame, &mut model.borrow_mut())))
        };
        let main_window = {
            let model = model.clone();
            Rc::new(ImguiWindow::new(WindowKind::Floating { title: "Sidetone".into() }, window_at, false, move |frame| {
                ui::main_window::build(frame, &mut model.borrow_mut())
            }))
        };

        let menu: Rc<RefCell<Option<PluginMenu>>> = Rc::new(RefCell::new(None));
        {
            let model = model.clone();
            let mut m = PluginMenu::new("Sidetone", move |item| {
                let action = match item {
                    MENU_PANEL => Action::TogglePanel,
                    MENU_WINDOW => Action::ToggleMainWindow,
                    _ => return,
                };
                model.borrow_mut().actions.push(action);
            });
            if let Some(m) = m.as_mut() {
                m.add_item("Show Panel");
                m.add_item("Open Sidetone…");
                m.set_checked(MENU_PANEL, panel_visible);
            }
            *menu.borrow_mut() = m;
        }

        let mut handlers = Vec::new();
        let mut on_command = |name: &str, description: &str, action: fn() -> Action| {
            if let Some(cmd) = Command::create(name, description) {
                let model = model.clone();
                handlers.push(CommandHandler::register(cmd, true, move |phase| {
                    if phase == Phase::Begin {
                        model.borrow_mut().actions.push(action());
                    }
                    false
                }));
            }
        };
        on_command("sidetone/toggle_panel", "Sidetone: show/hide the minimal panel", || Action::TogglePanel);
        on_command("sidetone/toggle_window", "Sidetone: open/close the main window", || Action::ToggleMainWindow);

        if let Some(ptt) = Command::create("sidetone/ptt", "Sidetone: push-to-talk (hold)") {
            let model = model.clone();
            handlers.push(CommandHandler::register(ptt, true, move |phase| {
                model.borrow_mut().state.ptt_pressed = phase != Phase::End;
                false
            }));
        }
        // X-Plane's own ATC push-to-talk works too (configurable). We let X-Plane keep handling it.
        if let Some(atc) = Command::find("sim/operation/contact_atc") {
            let model = model.clone();
            handlers.push(CommandHandler::register(atc, true, move |phase| {
                let mut m = model.borrow_mut();
                if m.settings.audio.ptt_uses_xplane_atc_command {
                    m.state.ptt_pressed = phase != Phase::End;
                }
                true
            }));
        } else {
            log::warn!("sim/operation/contact_atc not found; only sidetone/ptt will key the mic");
        }

        let cache_dir = paths::system_path().join("Output").join("Sidetone").join("cache");
        let worker = {
            let bus = bus.clone();
            Rc::new(Worker::spawn(cache_dir, move |update| bus.send(Event::Vatsim(update))))
        };

        let services = {
            let bus = bus.clone();
            Rc::new(services::Worker::spawn(move |update| bus.send(Event::Services(update))))
        };
        {
            let mut m = model.borrow_mut();
            m.hoppie_ready = keychain::get(Secret::HoppieLogon).is_some();
            m.simbrief_user_saved = keychain::get(Secret::SimbriefUsername).is_some();
        }

        let sim = Sim::new();
        let flight_loop = {
            let worker = worker.clone();
            let services = services.clone();
            let mut next_network_tick = 0.0f32;
            let model = model.clone();
            let panel = panel.clone();
            let main_window = main_window.clone();
            let menu = menu.clone();
            let settings_path = settings_path.clone();
            FlightLoop::new(move |_dt| {
                let loop_started = std::time::Instant::now();
                let mut m = model.borrow_mut();
                let now = elapsed_time();
                crate::drain_bus(|event| match event {
                    Event::Vatsim(update) => on_vatsim(&mut m, update, now),
                    Event::Services(update) => on_services(&mut m, update, &services, now),
                    Event::SimLog(_) => {}
                });
                sim.read(&mut m.state);
                bridge_tick(&mut m, now);
                if now >= next_network_tick {
                    next_network_tick = now + 1.0;
                    network_tick(&mut m, &worker);
                    datalink_tick(&mut m, &services);
                    window_tick(&mut m, &sim, &panel, &main_window);
                }
                let actions = std::mem::take(&mut m.actions);
                let mut save = false;
                for action in actions {
                    log::debug!("Action: {action:?}");
                    save |= apply(action, &mut m, &sim, &services, &panel, &main_window, &menu);
                }
                keep_on_screen(&panel, &m.settings);
                if panel.take_moved() {
                    let g = panel.handle().geometry();
                    m.settings.panel.position = Some((g.left, g.top));
                    save = true;
                }
                if main_window.take_moved() && !main_window.handle().is_popped_out() {
                    let g = main_window.handle().geometry();
                    m.settings.main_window.position = Some((g.left, g.top));
                    m.settings.main_window.size = (g.width(), g.height());
                    save = true;
                }
                save |= std::mem::take(&mut m.settings_dirty);
                if save && let Err(e) = m.settings.save(&settings_path) {
                    log::error!("Could not save settings: {e}");
                }
                let ms = loop_started.elapsed().as_secs_f32() * 1000.0;
                m.state.perf.loop_ms = m.state.perf.loop_ms * 0.9 + ms * 0.1;
                LOOP_INTERVAL
            })
        };
        flight_loop.schedule(LOOP_INTERVAL);

        App { model, _flight_loop: flight_loop, _handlers: handlers, _menu: menu, panel, main_window, _worker: worker, _services: services, settings_path }
    }
}

/// xPilot companion mode: mirror xPilot's connection while the mode is on.
fn bridge_tick(m: &mut Model, now: f32) {
    use crate::bridge::xpilot::Notice;
    let Model { xpilot, state, settings, .. } = m;
    state.transmitting = state.ptt_pressed;
    if !settings.integrations.xpilot_companion {
        xpilot.release(state);
        return;
    }
    for notice in xpilot.tick(state, now) {
        let text = match notice {
            Notice::Connected(callsign) => format!("Connected as {callsign} (via xPilot)"),
            Notice::Disconnected => "Disconnected from VATSIM (xPilot)".to_string(),
            Notice::Selcal => "SELCAL received".to_string(),
        };
        state.push_message("Sidetone", text, now);
    }
}

/// Once a second: UI scale, VR placement, keyboard indicator and performance figures.
fn window_tick(m: &mut Model, sim: &Sim, panel: &ImguiWindow, main_window: &ImguiWindow) {
    let scale = m.settings.ui.font_scale;
    panel.set_user_scale(scale);
    main_window.set_user_scale(scale);
    let g = panel.handle().geometry();
    let want = panel_rect(&m.settings);
    if !panel.handle().is_popped_out() && (g.width() != want.width() || g.height() != want.height()) {
        panel.handle().set_geometry(Rect { left: g.left, top: g.top, right: g.left + want.width(), bottom: g.top - want.height() });
    }

    // Follow the headset: move visible windows into VR when it turns on, back when it turns off.
    let vr = sim.vr_enabled();
    if vr != m.was_vr {
        let mode = if vr { Positioning::Vr } else { Positioning::Free };
        for w in [panel, main_window] {
            if w.is_visible() {
                w.handle().set_positioning(mode);
            }
        }
        if !vr {
            panel.handle().set_geometry(panel_rect(&m.settings));
        }
        log::info!("VR {}", if vr { "on: windows moved into the headset" } else { "off" });
        m.was_vr = vr;
    }

    m.state.keyboard_captured = panel.handle().has_keyboard_focus() || main_window.handle().has_keyboard_focus();
    m.state.perf.panel_ms = panel.cpu_ms();
    m.state.perf.window_ms = if main_window.is_visible() { main_window.cpu_ms() } else { 0.0 };
}

/// X-Plane may report a placeholder screen size while starting up, and the user can resize
/// the sim window later. Re-place the panel whenever it ends up (partly) off screen.
fn keep_on_screen(panel: &ImguiWindow, settings: &Settings) {
    if panel.handle().is_popped_out() {
        return;
    }
    let s = screen();
    if s.width() <= 0 || s.height() <= 0 {
        return;
    }
    let g = panel.handle().geometry();
    if g.left < s.left || g.right > s.right || g.top > s.top || g.bottom < s.bottom {
        let fixed = panel_rect(settings);
        log::info!("Panel off screen at {g:?}; moving to {fixed:?} (screen {s:?})");
        panel.handle().set_geometry(fixed);
    }
}

/// Applies one UI action. Returns true if settings changed and should be saved.
fn apply(
    action: Action,
    m: &mut Model,
    sim: &Sim,
    services: &services::Worker,
    panel: &ImguiWindow,
    main_window: &ImguiWindow,
    menu: &RefCell<Option<PluginMenu>>,
) -> bool {
    match action {
        Action::TogglePanel => {
            let visible = !panel.is_visible();
            panel.set_visible(visible);
            m.settings.panel.visible = visible;
            if let Some(menu) = menu.borrow().as_ref() {
                menu.set_checked(MENU_PANEL, visible);
            }
            true
        }
        Action::ToggleMainWindow => {
            main_window.set_visible(!main_window.is_visible());
            if main_window.is_visible() {
                m.state.unread = 0;
            }
            false
        }
        Action::ResetPanelPosition => {
            m.settings.panel.position = None;
            panel.handle().set_geometry(panel_rect(&m.settings));
            true
        }
        Action::SaveSettings => true,
        Action::Tune { com, khz } => {
            sim.tune(com, khz);
            false
        }
        Action::SaveSecret(secret, Redacted(value)) => {
            match keychain::set(secret, value.trim()) {
                Ok(()) => {
                    match secret {
                        Secret::HoppieLogon => {
                            m.hoppie_ready = true;
                            m.hoppie_configured_callsign.clear(); // re-send identity with the new code
                        }
                        Secret::SimbriefUsername => m.simbrief_user_saved = true,
                    }
                    m.system_message("Saved to your macOS Keychain", elapsed_time());
                }
                Err(e) => m.system_message(format!("Keychain error: {e}"), elapsed_time()),
            }
            false
        }
        Action::DeleteSecret(secret) => {
            keychain::delete(secret);
            match secret {
                Secret::HoppieLogon => {
                    m.hoppie_ready = false;
                    services.request(services::Request::ConfigureHoppie { logon: None, callsign: String::new() });
                    m.hoppie_configured_callsign.clear();
                }
                Secret::SimbriefUsername => m.simbrief_user_saved = false,
            }
            false
        }
        Action::ImportSimBrief => {
            match keychain::get(Secret::SimbriefUsername) {
                Some(username) => {
                    m.simbrief_status = Some("Importing…".into());
                    services.request(services::Request::FetchSimBrief { username });
                }
                None => m.simbrief_status = Some("Add your SimBrief Pilot ID in Settings first.".into()),
            }
            false
        }
        Action::Hoppie(op) => {
            hoppie_action(op, m, services);
            false
        }
        Action::SubmitChat(text) => {
            submit_chat(&text, m, sim);
            false
        }
    }
}

/// Keeps the services worker's Hoppie identity in step with the datalink callsign. The
/// callsign is frozen while logged on to a station.
fn datalink_tick(m: &mut Model, services: &services::Worker) {
    if !m.hoppie_ready || m.hoppie.station().is_some() {
        return;
    }
    let callsign = m.datalink_callsign();
    if callsign != m.hoppie_configured_callsign {
        m.hoppie.callsign = callsign.clone();
        services.request(services::Request::ConfigureHoppie { logon: keychain::get(Secret::HoppieLogon), callsign: callsign.clone() });
        m.hoppie_configured_callsign = callsign;
    }
}

fn hoppie_action(op: HoppieAction, m: &mut Model, services: &services::Worker) {
    let now = elapsed_time();
    if !m.hoppie_ready {
        m.system_message("Add your Hoppie logon code in Settings first.", now);
        return;
    }
    if m.hoppie.callsign.is_empty() {
        m.system_message("Set a callsign for datalink first (CPDLC tab).", now);
        return;
    }
    let out = match op {
        HoppieAction::Logon(station) if !station.trim().is_empty() => Some(m.hoppie.logon(&station)),
        HoppieAction::Logon(_) => None,
        HoppieAction::Logoff => m.hoppie.logoff(),
        HoppieAction::Reply { id, reply } => m.hoppie.reply(id, &reply),
        HoppieAction::Request(text) => {
            let out = m.hoppie.request(&text);
            if out.is_none() {
                m.system_message("Log on to an ATC station before sending CPDLC requests.", now);
            }
            out
        }
        HoppieAction::Pdc { station, stand } => {
            let (dep, arr) = (m.state.route.departure.clone().unwrap_or_default(), m.state.route.arrival.clone().unwrap_or_default());
            if dep.is_empty() || arr.is_empty() {
                m.system_message("A PDC needs your departure and arrival (import SimBrief or set them in the ATC tab).", now);
                None
            } else {
                let aircraft = m.simbrief.as_ref().map(|p| p.aircraft_icao.clone()).unwrap_or_default();
                let atis = m
                    .state
                    .network
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.stations.iter().find(|st| st.callsign.starts_with(dep.as_str()) && st.atis_code.is_some()))
                    .and_then(|st| st.atis_code.clone())
                    .unwrap_or_default();
                let station = if station.trim().is_empty() { dep.clone() } else { station };
                Some(m.hoppie.request_pdc(&station, &aircraft, &dep, &arr, &stand, &atis))
            }
        }
    };
    if let Some(out) = out {
        services.request(services::Request::Send(out));
    }
}

/// Applies an update from the services worker (Hoppie, SimBrief).
fn on_services(m: &mut Model, update: services::Update, worker: &services::Worker, now: f32) {
    match update {
        services::Update::Received(messages) => {
            let (notices, auto) = m.hoppie.on_incoming(messages);
            for out in auto {
                worker.request(services::Request::Send(out));
            }
            for notice in notices {
                let text = match notice {
                    Notice::Uplink { station, text } => {
                        let parsed = clearance::parse(&text);
                        if !parsed.is_empty() && m.settings.flight.apply_clearance(&parsed) {
                            m.settings_dirty = true;
                            m.state.push_message("Sidetone", "Clearance noted in your Clearance card", now);
                        }
                        format!("{station}: {}", text.replace('\n', " "))
                    }
                    Notice::LoggedOn(station) => format!("Logged on to {station}"),
                    Notice::LogonRejected(station) => format!("{station} rejected the logon"),
                    Notice::LoggedOff(station) => format!("{station} ended the CPDLC connection"),
                };
                m.state.push_message("CPDLC", text, now);
            }
        }
        services::Update::HoppieStatus(status) => m.hoppie_error = status.err(),
        services::Update::SimBrief(result) => match result {
            Ok(plan) => {
                m.simbrief_status = None;
                m.system_message(format!("Imported SimBrief plan {} {} → {}", plan.callsign, plan.origin, plan.destination), now);
                let f = &mut m.settings.flight;
                let mut changed = FlightNotes::fill(&mut f.runway, &plan.origin_runway);
                if let Some(sid) = clearance::sid_from_route(&plan.route) {
                    changed |= FlightNotes::fill(&mut f.sid, &sid);
                }
                m.settings_dirty |= changed;
                m.simbrief = Some(plan);
                m.recompute_route();
            }
            Err(e) => m.simbrief_status = Some(e),
        },
    }
}

/// Applies an update from the VATSIM worker.
fn on_vatsim(m: &mut Model, update: Update, now: f32) {
    match update {
        Update::Snapshot(snapshot) => {
            m.state.network.snapshot = Some(snapshot.clone());
            m.recompute_route();
            let airports = m.state.watched_airports();
            let mut notices = Vec::new();
            if m.settings.vatsim.weather_alerts {
                notices.extend(m.watcher.atis(&snapshot.stations, &airports));
            }
            let (statuses, friend_notices) = m.watcher.friends(&snapshot.feed, &snapshot.stations, &m.settings.friends);
            m.friend_statuses = statuses;
            if m.settings.vatsim.friend_alerts {
                notices.extend(friend_notices);
            }
            for notice in notices {
                m.state.push_message("VATSIM", notice, now);
            }
            // Find the pilot's CID automatically: from the live callsign (xPilot) or SimBrief.
            if m.settings.vatsim.cid.is_none() {
                let callsign = match &m.state.connection {
                    sidetone_core::state::Connection::Connected { callsign } => callsign.clone(),
                    _ => m.simbrief.as_ref().map(|p| p.callsign.clone()).unwrap_or_default(),
                };
                let (dep, arr) = (m.state.route.departure.clone(), m.state.route.arrival.clone());
                if let Some(cid) = sidetone_core::watch::detect_cid(&snapshot.feed, &callsign, dep.as_deref(), arr.as_deref()) {
                    m.settings.vatsim.cid = Some(cid);
                    m.ui.cid_input = cid.to_string();
                    m.cid_detected = true;
                    m.settings_dirty = true;
                    m.state.push_message("Sidetone", format!("Found your VATSIM CID ({cid}) from your {callsign} flight"), now);
                }
            }
            // Auto-fill the clearance card (only empty fields).
            if let Some(cid) = m.settings.vatsim.cid
                && let Some(fp) = snapshot.feed.flight_plan_for(cid)
                && fp.assigned_transponder != "0000"
            {
                m.settings_dirty |= FlightNotes::fill(&mut m.settings.flight.squawk, &fp.assigned_transponder);
            }
            if let Some(dep) = m.state.route.departure.clone()
                && let Some(code) =
                    snapshot.stations.iter().find(|s| s.callsign.starts_with(dep.as_str()) && s.atis_code.is_some()).and_then(|s| s.atis_code.clone())
            {
                m.settings_dirty |= FlightNotes::fill(&mut m.settings.flight.atis, &code);
            }
        }
        Update::Boundaries(b) => {
            log::info!("FIR boundaries loaded: {}", b.items.len());
            m.state.network.boundaries = Some(b);
        }
        Update::Events(events) => {
            let airports = m.state.watched_airports();
            for notice in m.watcher.events(&events, &airports, sidetone_vatsim::time::now_unix()) {
                m.state.push_message("VATSIM", notice, now);
            }
            m.state.network.events = events;
        }
        Update::RouteAtc(legs) => m.state.network.route_atc = legs,
        Update::VatSpy(vatspy) => {
            log::info!("VATSpy data loaded: {} airports, {} FIRs", vatspy.airports.len(), vatspy.firs.len());
            m.state.network.vatspy = Some(vatspy);
        }
        Update::Metars(metars) => {
            let notices = m.watcher.metars(&metars);
            if m.settings.vatsim.weather_alerts {
                for notice in notices {
                    m.state.push_message("VATSIM", notice, now);
                }
            }
            if let Some(dep) = m.state.route.departure.clone()
                && let Some(qnh) = metars.get(&dep).and_then(|t| clearance::qnh_from_metar(t))
            {
                m.settings_dirty |= FlightNotes::fill(&mut m.settings.flight.qnh, &qnh);
            }
            m.state.network.metars.extend(metars);
        }
        Update::Stats(cid, stats) => m.state.network.stats = Some((cid, stats)),
        Update::FeedError(error) => m.state.network.feed_error = error,
    }
}

/// Once a second: match COM frequencies to stations, track the nearest airport, and keep the
/// worker watching the right airports and member.
fn network_tick(m: &mut Model, worker: &Worker) {
    let position = m.state.position;
    let horizon = radio_horizon_nm(m.state.altitude_ft);
    if let Some(snapshot) = m.state.network.snapshot.clone() {
        for radio in [&mut m.state.com1, &mut m.state.com2] {
            radio.station = match stations::on_frequency(&snapshot.stations, radio.active_khz, position) {
                Some(s) => {
                    let distance_nm = position.and_then(|p| s.distance_nm(p));
                    Some(TunedStation {
                        callsign: s.callsign.clone(),
                        name: s.name.clone(),
                        distance_nm,
                        out_of_range: distance_nm.is_some_and(|d| d > horizon),
                    })
                }
                None if radio.active_khz == UNICOM_KHZ => {
                    Some(TunedStation { callsign: "UNICOM".into(), name: "UNICOM".into(), distance_nm: None, out_of_range: false })
                }
                None => None,
            };
        }
    }

    if let (Some(vatspy), Some(pos)) = (&m.state.network.vatspy, position)
        && m.nearest_checked_at.is_none_or(|at| sidetone_vatsim::geo::distance_nm(at, pos) > 2.0)
    {
        m.state.nearest_airport = vatspy.nearest_airport(pos).map(|(a, d)| (a.icao.clone(), d));
        m.nearest_checked_at = Some(pos);
    }

    // Which frequency should you be on?
    m.state.coverage_hint = None;
    if let (Some(snapshot), Some(vatspy), Some(boundaries), Some(pos)) =
        (&m.state.network.snapshot, &m.state.network.vatspy, &m.state.network.boundaries, position)
    {
        let covering = coverage::covering(pos, m.state.on_ground, &snapshot.stations, vatspy, boundaries);
        let tuned = |khz: i32| m.state.com1.active_khz == khz || m.state.com2.active_khz == khz;
        m.state.coverage_hint = match covering.first() {
            Some(s) if !covering.iter().any(|c| tuned(c.frequency_khz)) => Some(CoverageHint::Tune { name: s.name.clone(), khz: s.frequency_khz }),
            None if !m.state.on_ground && !tuned(UNICOM_KHZ) => Some(CoverageHint::Unicom),
            _ => None,
        };
    }

    // Route for "ATC along my route": SimBrief waypoints when we have them.
    let points: Vec<LatLon> = m.simbrief.as_ref().map(|p| p.fixes.iter().map(|(_, lat, lon)| LatLon { lat: *lat, lon: *lon }).collect()).unwrap_or_default();
    let query = RouteQuery { departure: m.state.route.departure.clone(), arrival: m.state.route.arrival.clone(), points };
    if query != m.requested_route {
        worker.request(Request::SetRoute(query.clone()));
        m.requested_route = query;
    }

    let airports = m.state.watched_airports();
    if airports != m.requested_airports {
        worker.request(Request::WatchAirports(airports.clone()));
        m.requested_airports = airports;
    }
    if m.settings.vatsim.cid != m.requested_cid {
        worker.request(Request::SetCid(m.settings.vatsim.cid));
        m.requested_cid = m.settings.vatsim.cid;
    }
}

fn submit_chat(text: &str, m: &mut Model, sim: &Sim) {
    let now = elapsed_time();
    match dot_command::parse(text) {
        Ok(DotCommand::Tune { com, khz }) => {
            sim.tune(com, khz);
            m.system_message(
                format!("Tuned {} to {}", if com == dot_command::Com::One { "COM1" } else { "COM2" }, sidetone_core::radio::format_com_khz(khz)),
                now,
            );
        }
        Ok(DotCommand::Squawk(code)) => {
            sim.squawk(code);
            m.system_message(format!("Squawking {}", sidetone_core::radio::format_squawk(code)), now);
        }
        Ok(DotCommand::Clear) => m.state.messages.clear(),
        Ok(DotCommand::Metar(icao)) => match m.state.network.metars.get(&icao).cloned() {
            Some(metar) => m.system_message(metar, now),
            None => m.system_message(format!("No METAR loaded for {icao}. Add it as your departure or arrival in the ATC tab."), now),
        },
        Ok(DotCommand::PrivateMessage { .. } | DotCommand::Atis(_)) => {
            m.system_message("Private messages and ATIS requests need a VATSIM connection. Use your pilot client for these until Sidetone is approved.", now)
        }
        Err(dot_command::ParseError::NotACommand) => {
            m.state.messages.push(sidetone_core::state::Message { from: "You".into(), text: text.into(), received_at: now });
        }
        Err(e) => m.system_message(e.to_string(), now),
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let mut m = self.model.borrow_mut();
        let Model { xpilot, state, .. } = &mut *m;
        xpilot.release(state);
        m.settings.panel.visible = self.panel.is_visible();
        if !self.main_window.handle().is_popped_out() {
            let g = self.main_window.handle().geometry();
            m.settings.main_window.position = Some((g.left, g.top));
            m.settings.main_window.size = (g.width(), g.height());
        }
        if let Err(e) = m.settings.save(&self.settings_path) {
            log::error!("Could not save settings on shutdown: {e}");
        }
    }
}
