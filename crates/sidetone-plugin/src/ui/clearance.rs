//! Clearance tab: your SimBrief plan, what you need to request a clearance, the numbers you were given, and
//! notes. Auto-filled from clearances (PDC/CPDLC), SimBrief, the VATSIM feed and METARs —
//! always editable. Sidetone never changes cockpit settings for you here.

use super::{Model, section, tune_buttons};
use crate::app::Action;
use sidetone_core::radio::{format_com_khz, format_squawk, is_valid_squawk, phonetic};
use sidetone_ui::imgui::{InputTextFlags, TreeNodeFlags, Ui};
use sidetone_ui::theme;
use sidetone_vatsim::naming::Facility;

pub fn build(ui: &Ui, m: &mut Model) {
    simbrief(ui, m);
    request(ui, m);
    clearance(ui, m);
    route(ui, m);
    notes(ui, m);
}

/// SimBrief import and a one-line summary of the plan (filing arrives with the network client).
fn simbrief(ui: &Ui, m: &mut Model) {
    section(ui, "FLIGHT PLAN");
    let busy = m.simbrief_status.as_deref() == Some("Importing…");
    {
        let _disabled = ui.begin_disabled_with_cond(busy || !m.simbrief_user_saved);
        if ui.small_button(if m.simbrief.is_some() { "Re-import from SimBrief" } else { "Import from SimBrief" }) {
            m.actions.push(Action::ImportSimBrief);
        }
    }
    ui.same_line();
    match (&m.simbrief_status, &m.simbrief) {
        (Some(status), _) => ui.text_colored(if busy { theme::TEXT_DIM } else { theme::WARN }, status),
        (None, _) if !m.simbrief_user_saved => ui.text_disabled("Add your SimBrief Pilot ID in Settings first."),
        (None, None) => ui.text_disabled("No plan imported."),
        (None, Some(plan)) => {
            ui.text_colored(theme::ACCENT, &plan.callsign);
            ui.same_line();
            let runway = |icao: &str, rwy: &str| if rwy.is_empty() { icao.to_string() } else { format!("{icao} {rwy}") };
            let mut summary = format!(
                "{} · {} → {} · {} · {}",
                plan.aircraft_icao,
                runway(&plan.origin, &plan.origin_runway),
                runway(&plan.destination, &plan.destination_runway),
                plan.cruise_label(),
                plan.ete_label()
            );
            if !plan.cost_index.is_empty() {
                summary.push_str(&format!(" · CI {}", plan.cost_index));
            }
            ui.text_wrapped(summary);
        }
    }
}

/// The departure ATIS letter on the network now, and the station broadcasting it.
fn current_atis(m: &Model) -> Option<(String, String)> {
    let dep = m.state.route.departure.as_deref()?;
    let snapshot = m.state.network.snapshot.as_ref()?;
    let s = sidetone_vatsim::stations::departure_atis(&snapshot.stations, dep)?;
    Some((s.atis_code.clone()?, s.callsign.clone()))
}

/// Shows whether the pilot's ATIS letter is current, with a one-click update if not.
fn atis_check(ui: &Ui, m: &mut Model) {
    let entered = m.settings.flight.atis.trim().to_ascii_uppercase();
    let Some(dep) = m.state.route.departure.clone() else { return };
    if m.state.network.snapshot.is_none() {
        return;
    }
    let spell = |c: &str| phonetic(c).map(String::from).unwrap_or_else(|| c.to_string());
    match current_atis(m) {
        None => ui.text_disabled(format!("No ATIS online at {dep}")),
        Some((code, callsign)) if entered.is_empty() => {
            ui.text_disabled(format!("{callsign} is broadcasting information {}", spell(&code)));
            ui.same_line();
            if ui.small_button(format!("Use {code}")) {
                m.settings.flight.atis = code;
                m.settings.flight.atis_auto = true; // keep it current from now on
                m.actions.push(Action::SaveSettings);
            }
        }
        Some((code, _)) if code.eq_ignore_ascii_case(&entered) => {
            ui.text_colored(theme::OK, format!("✓ ATIS {} is current", spell(&code)));
        }
        Some((code, callsign)) => {
            ui.text_colored(theme::WARN, format!("ATIS is now {} ({callsign}). You have {}.", spell(&code), spell(&entered)));
            ui.same_line();
            if ui.small_button(format!("Use {code}")) {
                m.settings.flight.atis = code;
                m.settings.flight.atis_auto = true; // keep it current from now on
                m.actions.push(Action::SaveSettings);
            }
        }
    }
}

/// Everything needed for the first call: who to call, where you're going, and the words.
fn request(ui: &Ui, m: &mut Model) {
    section(ui, "REQUEST CLEARANCE");
    let dep = m.state.route.departure.clone().unwrap_or_default();
    let arr = m.state.route.arrival.clone().unwrap_or_default();
    if dep.is_empty() || arr.is_empty() {
        ui.text_disabled("Import your SimBrief plan or set departure and arrival on the Flight tab to prepare your request.");
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
    let outdated = current_atis(m).is_some_and(|(code, _)| !atis.is_empty() && !code.eq_ignore_ascii_case(&atis));
    if outdated {
        ui.same_line();
        ui.text_colored(theme::WARN, "Your ATIS letter is outdated. Check below.");
    } else if stand.is_empty() || atis.is_empty() {
        ui.same_line();
        ui.text_disabled("Add your stand and ATIS letter below to complete it.");
    }
}

fn clearance(ui: &Ui, m: &mut Model) {
    section(ui, "YOUR CLEARANCE");
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let field_w = 110.0 * unit;
    let mut changed = false;

    // Returns true when the pilot finished editing this field.
    let mut field = |ui: &Ui, label: &str, hint: &str, value: &mut String, flags: InputTextFlags| {
        ui.align_text_to_frame_padding();
        ui.text_disabled(label);
        ui.same_line_with_pos(105.0 * unit);
        ui.set_next_item_width(field_w);
        ui.input_text(format!("##{label}"), value).hint(hint).flags(flags).build();
        let edited = ui.is_item_deactivated_after_edit();
        changed |= edited;
        edited
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
        if field(ui, "QNH", "e.g. Q1022", &mut f.qnh, upper) {
            f.qnh_auto = f.qnh.trim().is_empty(); // typed by the pilot: stop tracking
        }
        ui.table_next_row();
        ui.table_next_column();
        if field(ui, "ATIS", "e.g. C", &mut f.atis, upper) {
            f.atis_auto = f.atis.trim().is_empty(); // typed by the pilot: stop tracking
        }
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

    atis_check(ui, m);

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

/// Collapsible route: as filed on VATSIM (what ATC sees) when available, else SimBrief.
fn route(ui: &Ui, m: &mut Model) {
    let filed = m.settings.vatsim.cid.and_then(|cid| m.state.network.snapshot.as_ref().and_then(|s| s.feed.flight_plan_for(cid).cloned()));
    let filed = filed.filter(|fp| !fp.route.trim().is_empty());
    let simbrief = m.simbrief.clone().filter(|p| !p.route.trim().is_empty());
    if filed.is_none() && simbrief.is_none() {
        return;
    }
    ui.spacing();
    if !ui.collapsing_header("Flight plan route", TreeNodeFlags::NONE) {
        return;
    }
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let col = 105.0 * unit;
    let mut block = |ui: &Ui, title: &str, from: &str, to: &str, cruise: &str, alternate: &str, route: &str| {
        let _id = ui.push_id(title);
        ui.text_disabled(title);
        ui.same_line_with_pos(col);
        let mut summary = format!("{from} → {to}");
        if !cruise.is_empty() {
            summary.push_str(&format!(" · {cruise}"));
        }
        if !alternate.is_empty() {
            summary.push_str(&format!(" · alternate {alternate}"));
        }
        ui.text(summary);
        ui.same_line();
        if ui.small_button("Copy") {
            sidetone_ui::copy_to_clipboard(route.trim());
            m.system_message("Route copied", sidetone_xplm::elapsed_time());
        }
        let wrap = ui.push_text_wrap_pos(0.0);
        ui.text_colored(theme::TEXT, route.trim());
        drop(wrap);
        ui.spacing();
    };
    // VATSIM levels are filed as "35000" or "FL350"; show them the way pilots say them.
    let level = |raw: &str| match raw.trim().trim_start_matches("FL").parse::<u32>() {
        Ok(ft) if ft >= 1000 => {
            if ft >= 18_000 {
                format!("FL{}", ft / 100)
            } else {
                format!("{ft} ft")
            }
        }
        Ok(fl) => format!("FL{fl:03}"),
        Err(_) => raw.trim().to_string(),
    };
    if let Some(fp) = &filed {
        block(ui, "Filed on VATSIM", &fp.departure, &fp.arrival, &level(&fp.altitude), &fp.alternate, &fp.route);
    }
    if let Some(plan) = &simbrief {
        let differs = filed.as_ref().is_some_and(|fp| normalise(&fp.route) != normalise(&plan.route));
        if filed.is_none() || differs {
            if differs {
                ui.text_colored(theme::WARN, "Your SimBrief route differs from the one filed on VATSIM:");
            }
            block(ui, "SimBrief", &plan.origin, &plan.destination, &plan.cruise_label(), &plan.alternate, &plan.route);
        }
        if !plan.icao_flight_plan.trim().is_empty() {
            ui.text_disabled("ICAO flight plan");
            let wrap = ui.push_text_wrap_pos(0.0);
            ui.text_colored(theme::TEXT, plan.icao_flight_plan.trim());
            drop(wrap);
        }
    }
}

/// Route text without speed/level groups and DCTs, for comparing filed vs planned.
fn normalise(route: &str) -> Vec<String> {
    route.split_whitespace().map(|t| t.split('/').next().unwrap_or(t).to_ascii_uppercase()).filter(|t| t != "DCT" && !t.is_empty()).collect()
}

fn notes(ui: &Ui, m: &mut Model) {
    section(ui, "NOTES");
    let avail = ui.content_region_avail();
    let footer = ui.frame_height_with_spacing() * 2.0;
    ui.input_text_multiline("##notes", &mut m.settings.flight.notes, [avail[0], (avail[1] - footer).max(60.0)]).build();
    let changed = ui.is_item_deactivated_after_edit();

    if ui.button("New flight") {
        m.actions.push(Action::NewFlight);
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Clear this card, your notes and airport overrides for the next flight.\nImporting a SimBrief plan does this automatically.");
    }
    ui.same_line();
    ui.text_wrapped("Filled in from your clearance, SimBrief, VATSIM and METARs. Anything you type wins.");
    if changed {
        m.actions.push(Action::SaveSettings);
    }
}
