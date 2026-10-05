//! ATC tab: frequency hint, your airports (ATIS/METAR/events), ATC along your route, and the
//! online station list with one-click tuning.

use super::{Model, section, tune_buttons};
use crate::app::Action;
use sidetone_core::dot_command::Com;
use sidetone_core::radio::format_com_khz;
use sidetone_core::state::CoverageHint;
use sidetone_ui::imgui::{ListClipper, TableColumnFlags, TableFlags, Ui};
use sidetone_ui::theme;
use sidetone_vatsim::naming::Facility;
use sidetone_vatsim::time::{format_utc, now_unix};
use std::sync::Arc;

/// Stations further than this are hidden unless "Show all" is ticked or you search.
const NEARBY_NM: f64 = 300.0;

pub fn build(ui: &Ui, m: &mut Model) {
    hint(ui, m);
    your_airports(ui, m);
    along_route(ui, m);
    stations(ui, m);
}

pub fn facility_color(f: Facility) -> [f32; 4] {
    match f {
        Facility::Center | Facility::Fss => [0.55, 0.62, 0.98, 1.0],
        Facility::Approach | Facility::Departure => [0.80, 0.55, 0.98, 1.0],
        Facility::Tower => theme::DANGER,
        Facility::Ground | Facility::Apron | Facility::Delivery => theme::WARN,
        Facility::Atis => theme::OK,
        Facility::Other => theme::TEXT_DIM,
    }
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
                ui.tooltip_text(format!("{callsign}\nClick to tune COM1"));
            }
        }
        if leg.online.len() > shown {
            let more = format!("+{} more", leg.online.len() - shown);
            chip_place(ui, &mut first, row_start, right_edge, ui.calc_text_size(&more)[0]);
            ui.text_disabled(&more);
            if ui.is_item_hovered() {
                let rest: Vec<String> = leg.online.iter().skip(shown).map(|(cs, name, khz)| format!("{name} {} · {cs}", format_com_khz(*khz))).collect();
                ui.tooltip_text(format!("{}\n\nAll are in the Online ATC list below.", rest.join("\n")));
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

fn stations(ui: &Ui, m: &mut Model) {
    let Some(snapshot) = m.state.network.snapshot.clone() else {
        section(ui, "ONLINE ATC");
        ui.text_disabled(match &m.state.network.feed_error {
            Some(e) => format!("VATSIM data unavailable: {e}"),
            None => "Loading VATSIM data…".into(),
        });
        return;
    };
    section(ui, &format!("ONLINE ATC · {} stations", snapshot.stations.len()));
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    ui.set_next_item_width(220.0 * unit);
    ui.input_text("##atc_search", &mut m.ui.atc_search).hint("Search callsign, name or frequency").build();
    ui.same_line();
    ui.checkbox("Show all", &mut m.ui.atc_show_all);

    // Rebuild the filtered/sorted list only when its inputs change (≈6 nm position steps).
    let position = m.state.position;
    let pos_key = position.map(|p| ((p.lat * 10.0) as i64) * 10_000 + (p.lon * 10.0) as i64).unwrap_or(i64::MIN);
    let key = (Arc::as_ptr(&snapshot) as usize, m.ui.atc_search.trim().to_ascii_lowercase(), m.ui.atc_show_all, pos_key);
    if key != m.atc_rows_key {
        let query = key.1.clone();
        let mut rows: Vec<(usize, Option<f64>)> = snapshot
            .stations
            .iter()
            .enumerate()
            .map(|(i, s)| (i, position.and_then(|p| s.distance_nm(p))))
            .filter(|(_, d)| m.ui.atc_show_all || !query.is_empty() || d.is_some_and(|d| d <= NEARBY_NM))
            .filter(|(i, _)| {
                let s = &snapshot.stations[*i];
                query.is_empty()
                    || s.callsign.to_ascii_lowercase().contains(&query)
                    || s.name.to_ascii_lowercase().contains(&query)
                    || format_com_khz(s.frequency_khz).contains(&query)
            })
            .collect();
        rows.sort_by(|a, b| {
            let (sa, sb) = (&snapshot.stations[a.0], &snapshot.stations[b.0]);
            sa.facility.cmp(&sb.facility).then(a.1.unwrap_or(f64::MAX).total_cmp(&b.1.unwrap_or(f64::MAX)))
        });
        m.atc_rows = rows;
        m.atc_rows_key = key;
    }

    if m.atc_rows.is_empty() {
        ui.spacing();
        ui.text_disabled(if position.is_none() { "Waiting for aircraft position…" } else { "No ATC within 300 nm. Tick \"Show all\" or search." });
        return;
    }

    let flags = TableFlags::ROW_BG | TableFlags::BORDERS_INNER_H | TableFlags::SCROLL_Y;
    let Some(_table) = ui.begin_table_with_flags("stations", 5, flags) else { return };
    ui.table_setup_column_stretch_weight("Station", TableColumnFlags::NONE, 1.0);
    ui.table_setup_column_fixed_width("Callsign", TableColumnFlags::NONE, 110.0 * unit);
    ui.table_setup_column_fixed_width("Freq", TableColumnFlags::NONE, 62.0 * unit);
    ui.table_setup_column_fixed_width("Dist", TableColumnFlags::NONE, 54.0 * unit);
    // Sized from the real button widths so both COM buttons always fit at any UI scale.
    let style = ui.clone_style();
    let button = |label: &str| ui.calc_text_size(label)[0] + style.frame_padding()[0] * 2.0;
    let tune_width = button("COM1") + button("COM2") + style.item_spacing()[0] + style.cell_padding()[0] * 2.0;
    ui.table_setup_column_fixed_width("Tune", TableColumnFlags::NONE, tune_width);
    ui.table_headers_row();

    let rows = std::mem::take(&mut m.atc_rows);
    // Only rows on screen are laid out — the list stays cheap with hundreds of stations.
    for index in ListClipper::new(rows.len()).begin(ui).iter() {
        let (i, distance) = rows[index];
        let station = &snapshot.stations[i];
        let _id = ui.push_id(station.callsign.as_str());
        ui.table_next_row();
        ui.table_next_column();
        ui.text_colored(facility_color(station.facility), station.facility.short());
        ui.same_line();
        ui.text(&station.name);
        if ui.is_item_hovered() {
            let mut tip = format!("{} · {}", station.controller, station.callsign);
            for line in &station.text {
                tip.push('\n');
                tip.push_str(line);
            }
            ui.tooltip_text(tip);
        }
        ui.table_next_column();
        ui.text_disabled(&station.callsign);
        ui.table_next_column();
        let freq = format_com_khz(station.frequency_khz);
        if m.state.com1.active_khz == station.frequency_khz || m.state.com2.active_khz == station.frequency_khz {
            ui.text_colored(theme::ACCENT, &freq);
        } else {
            ui.text(&freq);
        }
        ui.table_next_column();
        ui.text_disabled(distance.map(|d| format!("{d:.0} nm")).unwrap_or_default());
        ui.table_next_column();
        tune_buttons(ui, m, station.frequency_khz);
    }
    m.atc_rows = rows;
}
