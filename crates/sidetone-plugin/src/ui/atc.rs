//! ATC tab: every online station, grouped by type and nearest first, with search and one-click tuning.

use super::panel::elide;
use super::{Model, section, tune_buttons};
use sidetone_core::radio::format_com_khz;
use sidetone_ui::imgui::{ListClipper, TableColumnFlags, TableFlags, Ui};
use sidetone_ui::theme;
use sidetone_vatsim::naming::Facility;
use std::sync::Arc;

/// Stations further than this are hidden unless "Show all" is ticked or you search.
const NEARBY_NM: f64 = 300.0;

pub fn build(ui: &Ui, m: &mut Model) {
    stations(ui, m);
}

pub fn facility_color(f: Facility) -> [f32; 4] {
    use theme::facility::*;
    match f {
        Facility::Center | Facility::Fss => CENTER,
        Facility::Approach | Facility::Departure => APPROACH,
        Facility::Tower => TOWER,
        Facility::Ground | Facility::Apron | Facility::Delivery => GROUND,
        Facility::Atis => ATIS,
        Facility::Other => theme::TEXT_DIM,
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
                    || s.display_name().to_ascii_lowercase().contains(&query)
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

    // Rows show their COM buttons only under the mouse; elsewhere a tag marks what you're tuned to.
    let (table_left, table_right) = (ui.cursor_screen_pos()[0], ui.cursor_screen_pos()[0] + ui.content_region_avail()[0]);
    let flags = TableFlags::ROW_BG | TableFlags::BORDERS_INNER_H | TableFlags::SCROLL_Y;
    // Narrow windows drop Callsign (it's in each row's hover details) but keep Dist.
    let narrow = super::narrow(ui);
    let (id, columns) = if narrow { ("stations_narrow", 4) } else { ("stations", 5) };
    let Some(_table) = ui.begin_table_with_flags(id, columns, flags) else { return };
    ui.table_setup_column_stretch_weight("Station", TableColumnFlags::NONE, 1.0);
    if !narrow {
        ui.table_setup_column_fixed_width("Callsign", TableColumnFlags::NONE, 110.0 * unit);
    }
    ui.table_setup_column_fixed_width("Freq", TableColumnFlags::NONE, 62.0 * unit);
    ui.table_setup_column_fixed_width("Dist", TableColumnFlags::NONE, 54.0 * unit);
    // Sized from the real button widths so both COM buttons always fit at any UI scale.
    let style = ui.clone_style();
    let button = |label: &str| super::pill_width(ui, label);
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
        // The whole row is the hover target: it highlights, shows the station's details and
        // reveals the COM buttons. (Small buttons are one text line tall, so every row is too.)
        let row_top = ui.cursor_screen_pos()[1] - style.cell_padding()[1];
        let row_height = ui.text_line_height() + style.cell_padding()[1] * 2.0;
        // Compared directly: ImGui's hover test clips to the current cell, which misses the row.
        let [mx, my] = ui.io().mouse_pos();
        // (Still hovered while a COM button is held down: it clicks on release, so it must still be there.)
        let window_hovered = ui.is_window_hovered_with_flags(sidetone_ui::imgui::WindowHoveredFlags::ALLOW_WHEN_BLOCKED_BY_ACTIVE_ITEM);
        let row_hovered = window_hovered && (table_left..table_right).contains(&mx) && (row_top..row_top + row_height).contains(&my);
        if row_hovered {
            ui.table_set_row_bg1_color([0.20, 0.78, 0.72, 0.10]);
        }
        ui.text_colored(facility_color(station.facility), station.facility.short());
        ui.same_line();
        // Long names end in "…".
        let room = ui.content_region_avail()[0];
        let place_width = station.qualifier.as_deref().map(|q| ui.calc_text_size(q)[0] + style.item_spacing()[0]).unwrap_or(0.0);
        if ui.calc_text_size(&station.name)[0] + place_width <= room {
            ui.text(&station.name);
            if let Some(place) = &station.qualifier {
                ui.same_line();
                ui.text_disabled(place);
            }
        } else {
            ui.text(elide(ui, &station.display_name(), room));
        }
        if !narrow {
            ui.table_next_column();
            ui.text_disabled(&station.callsign);
        }
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
        let khz = station.frequency_khz;
        if row_hovered {
            tune_buttons(ui, m, khz);
            // Details for the row, unless the pointer is on a COM button (that has its own job).
            if !ui.is_any_item_hovered() {
                let mut tip = format!("{} · {}\n{}", station.controller, station.callsign, station.name_source.describe());
                if !station.text.is_empty() {
                    tip.push_str("\n\n");
                    tip.push_str(&station.text.join("\n"));
                }
                super::wrapped_tooltip(ui, &tip);
            }
        } else {
            let tuned: Vec<&str> =
                [("COM1", m.state.com1.active_khz), ("COM2", m.state.com2.active_khz)].into_iter().filter(|(_, k)| *k == khz).map(|(l, _)| l).collect();
            if !tuned.is_empty() {
                ui.text_colored(theme::ACCENT, tuned.join(" · "));
            }
        }
    }
    m.atc_rows = rows;
}
