//! Flight tab: the setup checklist until it's hidden, who to call now, your airports (ATIS/METAR/events) and ATC along your route.

use super::{Model, section, tune_buttons};
use crate::app::Action;
use sidetone_core::dot_command::Com;
use sidetone_core::radio::format_com_khz;
use sidetone_core::state::CoverageHint;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;
use sidetone_vatsim::naming::Facility;
use sidetone_vatsim::time::{format_utc, now_unix};
use std::sync::Arc;

pub fn build(ui: &Ui, m: &mut Model) {
    super::setup::build(ui, m);
    hint(ui, m);
    your_airports(ui, m);
    along_route(ui, m);
    all_atc_link(ui, m);
}

/// "London Control 129.425 covers you" with a one-click tune.
fn hint(ui: &Ui, m: &mut Model) {
    let Some(hint) = m.state.coverage_hint.clone() else { return };
    ui.spacing();
    match hint {
        CoverageHint::Tune { name, khz } => {
            ui.text_colored(theme::ACCENT, "●");
            ui.same_line();
            if m.state.on_ground {
                ui.text(format!("Your first call: {name} {}", format_com_khz(khz)));
            } else {
                ui.text(format!("{name} {} covers your position", format_com_khz(khz)));
            }
            ui.same_line();
            tune_buttons(ui, m, khz);
        }
        CoverageHint::Unicom => {
            ui.text_colored(theme::WARN, "●");
            ui.same_line();
            ui.text("No ATC overhead. Monitor UNICOM 122.800");
            ui.same_line();
            tune_buttons(ui, m, sidetone_vatsim::freq::UNICOM_KHZ);
        }
    }
}

fn your_airports(ui: &Ui, m: &mut Model) {
    let airports = m.state.watched_airports();
    let source = match (m.state.route.source, &m.state.nearest_airport) {
        ("", Some(_)) => "nearest airport",
        ("", None) => "set below, import SimBrief, or add your CID",
        (s, _) => s,
    };
    section(ui, &format!("YOUR AIRPORTS · {source}"));
    let stations = m.state.network.snapshot.as_ref().map(|s| s.stations.clone()).unwrap_or_default();
    let now = now_unix();
    for icao in &airports {
        let _id = ui.push_id(icao.as_str());
        ui.text_colored(theme::ACCENT, icao);
        if let Some(name) = m.state.network.vatspy.as_ref().and_then(|v| v.airport_name(icao)) {
            ui.same_line();
            ui.text(name);
        }
        ui.same_line();
        ui.text_disabled("·");
        ui.same_line();
        let atis: Vec<_> = stations.iter().filter(|s| s.facility == Facility::Atis && s.callsign.split('_').next() == Some(icao.as_str())).collect();
        if atis.is_empty() {
            ui.text_disabled("No ATIS");
        } else {
            // Split ATIS (EHAM_A_ATIS / EHAM_D_ATIS) reads "Arr E · Dep R".
            let letters: Vec<String> = atis
                .iter()
                .map(|s| {
                    let code = s.atis_code.clone().unwrap_or_else(|| "?".into());
                    match s.callsign.split('_').collect::<Vec<_>>().as_slice() {
                        [_, "A", _] => format!("Arr {code}"),
                        [_, "D", _] => format!("Dep {code}"),
                        _ => code,
                    }
                })
                .collect();
            ui.text(format!("ATIS {}", letters.join(" · ")));
            if ui.is_item_hovered() {
                ui.tooltip_text(atis.iter().map(|s| s.text.join(" ")).collect::<Vec<_>>().join("\n\n"));
            }
        }
        for event in m.state.network.events.iter().filter(|e| e.involves(icao) && e.start < now + 24 * 3600) {
            ui.same_line();
            let (label, color) =
                if event.is_live(now) { ("EVENT LIVE".to_string(), theme::OK) } else { (format!("EVENT {}", format_utc(event.start, now)), theme::WARN) };
            ui.text_colored(color, label);
            if ui.is_item_hovered() {
                ui.tooltip_text(format!("{}\n{}", event.name, event.summary));
            }
        }
        match m.state.network.metars.get(icao) {
            Some(metar) => ui.text_wrapped(metar),
            None => ui.text_disabled("METAR loading…"),
        }
    }

    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let mut changed = false;
    ui.set_next_item_width(70.0 * unit);
    ui.input_text("##dep", &mut m.settings.vatsim.departure).hint("DEP").flags(sidetone_ui::imgui::InputTextFlags::CHARS_UPPERCASE).build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.same_line();
    ui.set_next_item_width(70.0 * unit);
    ui.input_text("##arr", &mut m.settings.vatsim.arrival).hint("ARR").flags(sidetone_ui::imgui::InputTextFlags::CHARS_UPPERCASE).build();
    changed |= ui.is_item_deactivated_after_edit();
    ui.same_line();
    ui.text_disabled("Optional override");
    if changed {
        m.settings.vatsim.departure = m.settings.vatsim.departure.trim().to_ascii_uppercase();
        m.settings.vatsim.arrival = m.settings.vatsim.arrival.trim().to_ascii_uppercase();
        m.recompute_route();
        m.actions.push(Action::SaveSettings);
    }
}

fn along_route(ui: &Ui, m: &mut Model) {
    let legs = Arc::clone(&m.state.network.route_atc);
    if legs.is_empty() {
        return;
    }
    let covered = legs.iter().filter(|l| !l.online.is_empty()).count();
    section(ui, &format!("ATC ALONG YOUR ROUTE · {covered} of {} areas staffed", legs.len()));
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let label_width = 150.0 * unit;
    let style = ui.clone_style();
    for (i, leg) in legs.iter().enumerate() {
        let _id = ui.push_id(i);
        if leg.online.is_empty() {
            ui.text_disabled(format!("○  {}", leg.label));
            ui.same_line_with_pos(label_width);
            ui.text_disabled("unstaffed");
            ui.same_line();
            let unicom = sidetone_vatsim::freq::UNICOM_KHZ;
            if m.state.com1.active_khz == unicom || m.state.com2.active_khz == unicom {
                ui.text_colored(theme::OK, "· on UNICOM ✓");
            } else {
                if ui.small_button("Switch to UNICOM") {
                    m.actions.push(Action::Tune { com: Com::One, khz: unicom });
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Tune COM1 to 122.800 and announce your intentions in text or voice");
                }
            }
            continue;
        }
        ui.text_colored(theme::OK, "●");
        ui.same_line();
        ui.text(&leg.label);
        ui.same_line_with_pos(label_width);

        // Chips flow onto further lines (aligned with the first) instead of running off the edge.
        let row_start = ui.cursor_screen_pos()[0];
        let right_edge = row_start + ui.content_region_avail()[0];
        let shown = leg.online.len().min(MAX_CHIPS);
        let mut first = true;
        for (callsign, name, khz) in leg.online.iter().take(shown) {
            let _id = ui.push_id(callsign.as_str());
            let label = format!("{name} {}", format_com_khz(*khz));
            chip_place(ui, &mut first, row_start, right_edge, ui.calc_text_size(&label)[0] + style.frame_padding()[0] * 2.0);
            if ui.small_button(&label) {
                m.actions.push(Action::Tune { com: Com::One, khz: *khz });
            }
            if ui.is_item_hovered() {
                let source = m
                    .state
                    .network
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.stations.iter().find(|s| s.callsign == *callsign))
                    .map(|s| s.name_source.describe())
                    .unwrap_or_default();
                ui.tooltip_text(format!("{callsign}\n{source}\nClick to tune COM1"));
            }
        }
        if leg.online.len() > shown {
            let more = format!("+{} more", leg.online.len() - shown);
            chip_place(ui, &mut first, row_start, right_edge, ui.calc_text_size(&more)[0]);
            ui.text_disabled(&more);
            if ui.is_item_hovered() {
                let rest: Vec<String> = leg.online.iter().skip(shown).map(|(cs, name, khz)| format!("{name} {} · {cs}", format_com_khz(*khz))).collect();
                ui.tooltip_text(format!("{}\n\nAll are in the ATC tab.", rest.join("\n")));
            }
        }
    }
}

/// Stations shown per route area before collapsing into "+N more".
const MAX_CHIPS: usize = 3;

/// Places the next chip on the current line if it fits, otherwise on a new line indented to `row_start`.
fn chip_place(ui: &Ui, first: &mut bool, row_start: f32, right_edge: f32, width: f32) {
    if *first {
        *first = false;
        return;
    }
    ui.same_line();
    if ui.cursor_screen_pos()[0] + width > right_edge {
        ui.new_line();
        let y = ui.cursor_screen_pos()[1];
        ui.set_cursor_screen_pos([row_start, y]);
    }
}

/// A way into the full list, which now lives on its own tab.
fn all_atc_link(ui: &Ui, m: &mut Model) {
    let Some(snapshot) = &m.state.network.snapshot else { return };
    ui.spacing();
    ui.spacing();
    ui.text_disabled(format!("{} ATC stations online", snapshot.stations.len()));
    ui.same_line();
    if ui.small_button("See all") {
        m.ui.select_tab = Some(super::Tab::Atc);
    }
}
