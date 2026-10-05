//! Clearance tab: what you need to request a clearance, the numbers you were given, and
//! notes. Auto-filled from clearances (PDC/CPDLC), SimBrief, the VATSIM feed and METARs —
//! always editable. Sidetone never changes cockpit settings for you here.

use super::{Model, section, tune_buttons};
use crate::app::Action;
use sidetone_core::radio::{format_com_khz, format_squawk, is_valid_squawk, phonetic};
use sidetone_ui::imgui::{InputTextFlags, Ui};
use sidetone_ui::theme;
use sidetone_vatsim::naming::Facility;

pub fn build(ui: &Ui, m: &mut Model) {
    request(ui, m);
    clearance(ui, m);
    notes(ui, m);
}

/// Everything needed for the first call: who to call, where you're going, and the words.
fn request(ui: &Ui, m: &mut Model) {
    section(ui, "REQUEST CLEARANCE");
    let dep = m.state.route.departure.clone().unwrap_or_default();
    let arr = m.state.route.arrival.clone().unwrap_or_default();
    if dep.is_empty() || arr.is_empty() {
        ui.text_disabled("Import your SimBrief plan or set departure and arrival in the ATC tab to prepare your request.");
        return;
    }
    let vatspy = m.state.network.vatspy.clone();
    let plan = m.simbrief.clone();
    // Prefer SimBrief's English names ("Copenhagen Kastrup"), else VATSpy's.
    let name_of = |icao: &str, simbrief: Option<&str>| {
        simbrief.filter(|n| !n.is_empty()).map(String::from).or_else(|| vatspy.as_ref().and_then(|v| v.airport_name(icao))).unwrap_or_default()
    };
    let dep_name = name_of(&dep, plan.as_ref().filter(|p| p.origin == dep).map(|p| p.origin_name.as_str()));
    let arr_name = name_of(&arr, plan.as_ref().filter(|p| p.destination == arr).map(|p| p.destination_name.as_str()));
    let country = vatspy.as_ref().and_then(|v| v.country(&arr).map(String::from));
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let col = 105.0 * unit;

    let row = |label: &str| {
        ui.text_disabled(label);
        ui.same_line_with_pos(col);
    };
    row("To");
    ui.text_colored(theme::ACCENT, &arr);
    ui.same_line();
    ui.text(match &country {
        Some(c) if !arr_name.is_empty() => format!("{arr_name}, {c}"),
        _ => arr_name.clone(),
    });

    let stand = m.settings.flight.stand.trim().to_string();
    row("From");
    ui.text_colored(theme::ACCENT, &dep);
    ui.same_line();
    ui.text(if stand.is_empty() { dep_name.clone() } else { format!("{dep_name} · Stand {stand}") });

    // Who to call: Delivery if online, else the first ground/tower unit in contact order.
    let order = [Facility::Delivery, Facility::Ground, Facility::Apron, Facility::Tower];
    let station = m.state.network.snapshot.as_ref().and_then(|snap| {
        order.iter().find_map(|f| snap.stations.iter().find(|s| s.facility == *f && s.callsign.split('_').next() == Some(dep.as_str())).cloned())
    });
    row("Call");
    match &station {
        Some(s) => {
            ui.text(format!("{} {}", s.name, format_com_khz(s.frequency_khz)));
            ui.same_line();
            tune_buttons(ui, m, s.frequency_khz);
        }
        None => ui.text_disabled("No Delivery, Ground or Tower online at your departure. Check UNICOM 122.800 or use PDC in the CPDLC tab."),
    }

    // The request, ready to say.
    let callsign = m.datalink_callsign();
    let atis = m.settings.flight.atis.trim().to_string();
    let aircraft = plan.as_ref().map(|p| p.aircraft_icao.clone()).unwrap_or_default();
    let mut words: Vec<String> = Vec::new();
    words.push(station.as_ref().map(|s| s.name.clone()).unwrap_or_else(|| format!("{dep_name} Delivery")));
    if !callsign.is_empty() {
        words.push(callsign.clone());
    }
    if !aircraft.is_empty() {
        words.push(aircraft);
    }
    if !stand.is_empty() {
        words.push(format!("stand {stand}"));
    }
    if let Some(word) = phonetic(&atis) {
        words.push(format!("information {word}"));
    }
    let destination = if arr_name.is_empty() { arr.clone() } else { arr_name.clone() };
    let phrase = format!("{}, request clearance to {destination}.", words.join(", "));
    ui.spacing();
    ui.text_colored(theme::ACCENT, "Say");
    ui.same_line_with_pos(col);
    let wrap = ui.push_text_wrap_pos(0.0); // 0 = wrap at the window edge
    ui.text(&phrase);
    drop(wrap);
    if ui.small_button("Copy") {
        sidetone_ui::copy_to_clipboard(&phrase);
        m.system_message("Clearance request copied", sidetone_xplm::elapsed_time());
    }
    if stand.is_empty() || atis.is_empty() {
        ui.same_line();
        ui.text_disabled("Add your stand and ATIS letter below to complete it.");
    }
}

fn clearance(ui: &Ui, m: &mut Model) {
    section(ui, "YOUR CLEARANCE");
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let field_w = 110.0 * unit;
    let mut changed = false;

    let mut field = |ui: &Ui, label: &str, hint: &str, value: &mut String, flags: InputTextFlags| {
        ui.align_text_to_frame_padding();
        ui.text_disabled(label);
        ui.same_line_with_pos(105.0 * unit);
        ui.set_next_item_width(field_w);
        ui.input_text(format!("##{label}"), value).hint(hint).flags(flags).build();
        changed |= ui.is_item_deactivated_after_edit();
    };

    let upper = InputTextFlags::CHARS_UPPERCASE;
    let f = &mut m.settings.flight;
    if let Some(_t) = ui.begin_table("clearance", 2) {
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "Squawk", "e.g. 4721", &mut f.squawk, InputTextFlags::CHARS_DECIMAL);
        ui.table_next_column();
        field(ui, "Initial alt", "e.g. FL060", &mut f.initial_altitude, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "SID", "e.g. CPT3J", &mut f.sid, upper);
        ui.table_next_column();
        field(ui, "Runway", "e.g. 27R", &mut f.runway, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "Dep freq", "e.g. 120.525", &mut f.departure_freq, InputTextFlags::NONE);
        ui.table_next_column();
        field(ui, "QNH", "e.g. Q1022", &mut f.qnh, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "ATIS", "e.g. C", &mut f.atis, upper);
        ui.table_next_column();
        field(ui, "Stand", "e.g. 512", &mut f.stand, upper);
    }

    // A reminder only: setting the transponder stays a cockpit task for the pilot.
    let squawk = m.settings.flight.squawk.trim().to_string();
    if is_valid_squawk(&squawk) {
        let current = format_squawk(m.state.transponder.code);
        if current == squawk {
            ui.text_colored(theme::OK, format!("Transponder set to {squawk}"));
        } else {
            ui.text_colored(theme::WARN, format!("Transponder is {current}, assigned {squawk}"));
        }
    } else if !squawk.is_empty() {
        ui.text_colored(theme::DANGER, "Squawk must be four digits 0–7");
    }

    if let Some(khz) = sidetone_core::radio::parse_com_khz(m.settings.flight.departure_freq.trim()) {
        ui.same_line();
        ui.text_disabled("· Departure");
        ui.same_line();
        tune_buttons(ui, m, khz);
    }
    if changed {
        m.actions.push(Action::SaveSettings);
    }
}

fn notes(ui: &Ui, m: &mut Model) {
    section(ui, "NOTES");
    let avail = ui.content_region_avail();
    let footer = ui.frame_height_with_spacing() * 2.0;
    ui.input_text_multiline("##notes", &mut m.settings.flight.notes, [avail[0], (avail[1] - footer).max(60.0)]).build();
    let mut changed = ui.is_item_deactivated_after_edit();

    if ui.button("New flight") {
        m.settings.flight = sidetone_core::clearance::FlightNotes::default();
        changed = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Clear the card for your next flight");
    }
    ui.same_line();
    ui.text_wrapped("Filled in from your clearance, SimBrief, VATSIM and METARs. Anything you type wins.");
    if changed {
        m.actions.push(Action::SaveSettings);
    }
}
