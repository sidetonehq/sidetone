//! Flight tab: the whole flight on one page, in collapsible sections. Setup checklist (until
//! hidden), who to call now, the flight (departure, arrival with ATIS/METAR on demand, route),
//! the clearance (collapses itself at takeoff), ATC along the route, and notes.

use super::panel::elide;
use super::{Fold, Model, Tab, card, flow, fold, strong, toggle_button, tune_buttons, tune_buttons_width, wrapped_tooltip};
use crate::app::Action;
use sidetone_core::clearance::qnh_from_metar;
use sidetone_core::dot_command::Com;
use sidetone_core::radio::format_com_khz;
use sidetone_core::state::CoverageHint;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;
use sidetone_ui::theme::icon;
use sidetone_vatsim::naming::Facility;
use sidetone_vatsim::time::{format_utc, now_unix};
use std::sync::Arc;

/// Each section is a card of its own; notes take whatever height is left.
pub fn build(ui: &Ui, m: &mut Model) {
    if !m.settings.setup.dismissed {
        card(ui, "setup", None, || super::setup::build(ui, m));
    }
    hint(ui, m);
    card(ui, "flight", None, || flight(ui, m));
    card(ui, "clearance", None, || super::clearance::build(ui, m));
    card(ui, "route_atc", None, || along_route(ui, m));
    card(ui, "arrival", None, || super::arrival::build(ui, m));
    let height = (!m.ui.collapsed.contains(&Fold::Notes)).then(|| {
        let unit = ui.current_font_size() / theme::FONT_SIZE;
        let padding = ui.clone_style().window_padding()[1];
        (ui.content_region_avail()[1] - padding - 6.0 * unit).max(130.0 * unit)
    });
    card(ui, "notes", height, || notes(ui, m));
}

/// "London Control 129.425 covers you" with a one-click tune.
fn hint(ui: &Ui, m: &mut Model) {
    let Some(hint) = m.state.coverage_hint.clone() else { return };
    ui.spacing();
    ui.align_text_to_frame_padding();
    let left = ui.cursor_screen_pos()[0];
    let right = left + ui.content_region_avail()[0];
    let text_width = |t: &str| ui.calc_text_size(t)[0];
    match hint {
        CoverageHint::Tune { name, khz } => {
            ui.text_colored(theme::ACCENT, "●");
            let station = format!("{name} {}", format_com_khz(khz));
            if m.state.on_ground {
                flow(ui, left, right, text_width("Your first call:"));
                ui.text("Your first call:");
                flow(ui, left, right, text_width(&station));
                strong(ui, m, theme::TEXT, &station);
            } else {
                flow(ui, left, right, text_width(&station));
                strong(ui, m, theme::TEXT, &station);
                flow(ui, left, right, text_width("covers your position"));
                ui.text("covers your position");
            }
            flow(ui, left, right, tune_buttons_width(ui));
            tune_buttons(ui, m, khz);
        }
        CoverageHint::Unicom => {
            ui.text_colored(theme::WARN, "●");
            let text = "No ATC overhead. Monitor UNICOM 122.800";
            flow(ui, left, right, text_width(text));
            ui.text(text);
            flow(ui, left, right, tune_buttons_width(ui));
            tune_buttons(ui, m, sidetone_vatsim::freq::UNICOM_KHZ);
        }
    }
}

/// Departure and arrival, one line each: airport, ATIS and METAR buttons (hover to read,
/// click to keep open) and events (QNH too for the nearest airport). Then the route.
fn flight(ui: &Ui, m: &mut Model) {
    let title = match &m.simbrief {
        Some(p) if !p.callsign.is_empty() => format!("FLIGHT · {} · {} · {} · {}", p.callsign, p.aircraft_icao, p.cruise_label(), p.ete_label()),
        _ => match m.state.route.source {
            "" => "FLIGHT".to_string(),
            source => format!("FLIGHT · from {source}"),
        },
    };
    let (open, _) = fold(ui, m, Fold::Flight, &title, &[]);
    if !open {
        return;
    }

    let mut rows: Vec<(&str, String)> = Vec::new();
    if let Some(dep) = &m.state.route.departure {
        rows.push(("Departure", dep.clone()));
    }
    if let Some(arr) = &m.state.route.arrival {
        rows.push(("Arrival", arr.clone()));
    }
    if rows.is_empty()
        && let Some((icao, _)) = &m.state.nearest_airport
    {
        rows.push(("Nearest", icao.clone()));
    }

    let unit = ui.current_font_size() / theme::FONT_SIZE;
    // Narrow windows stack each label above its values; wide ones line them up in columns.
    let narrow = super::narrow(ui);
    let base = ui.cursor_pos()[0];
    let icao_x = if narrow { base } else { 90.0 * unit };
    let name_x = icao_x + 48.0 * unit;
    let names: Vec<String> = rows.iter().map(|(_, icao)| m.state.network.vatspy.as_ref().and_then(|v| v.airport_name(icao)).unwrap_or_default()).collect();
    let widest = names.iter().map(|n| ui.calc_text_size(n)[0]).fold(0.0, f32::max);
    // Buttons line up after the longest name, but never get pushed off a narrow window.
    let right = ui.cursor_pos()[0] + ui.content_region_avail()[0];
    let buttons_x = (name_x + widest + 16.0 * unit).min(name_x + (right - name_x) * 0.45).max(name_x + 60.0 * unit);
    let name_room = buttons_x - name_x - 8.0 * unit;
    let stations = m.state.network.snapshot.as_ref().map(|s| s.stations.clone()).unwrap_or_default();
    let now = now_unix();

    // Narrow windows put departure and arrival side by side, each stacked in its own column;
    // texts clicked open then go full width under both.
    let columns = narrow && rows.len() == 2;
    let gap = 16.0 * unit;
    let column_w = (right - base - gap) / 2.0;
    let top = ui.cursor_pos()[1];
    let mut bottom = top;
    let mut opened: Vec<(String, String)> = Vec::new();

    for (i, ((label, icao), name)) in rows.iter().zip(&names).enumerate() {
        let _id = ui.push_id(icao.as_str());
        let (column_left, right) = if columns {
            let left = base + i as f32 * (column_w + gap);
            (left, left + column_w)
        } else {
            (base, right)
        };
        let group = columns.then(|| {
            ui.set_cursor_pos([column_left, top]);
            ui.begin_group()
        });
        if narrow {
            // Label, then airport, then its buttons: each on its own line.
            ui.spacing();
            super::caption(ui, m, label);
            strong(ui, m, theme::ACCENT, icao);
            ui.same_line();
            ui.text(elide(ui, name, right - ui.cursor_pos()[0]));
        } else {
            ui.align_text_to_frame_padding();
            super::caption(ui, m, label);
            ui.same_line_with_pos(icao_x);
            strong(ui, m, theme::ACCENT, icao);
            ui.same_line_with_pos(name_x);
            ui.text(elide(ui, name, name_room));
            ui.same_line_with_pos(buttons_x);
        }
        // The buttons flow onto a further line (aligned with the first) if the window is narrow.
        let line_left = ui.cursor_screen_pos()[0];
        let line_right = ui.window_pos()[0] + right;

        // ATIS: one button per station, so split ATIS reads "Arr B" "Dep O".
        let atis: Vec<_> = stations.iter().filter(|s| s.facility == Facility::Atis && s.callsign.split('_').next() == Some(icao.as_str())).collect();
        let mut open: Vec<(String, String)> = Vec::new();
        if atis.is_empty() {
            ui.text_disabled("No ATIS");
        }
        for (i, s) in atis.iter().enumerate() {
            let code = s.atis_code.clone().unwrap_or_else(|| "?".into());
            let button = match s.callsign.split('_').collect::<Vec<_>>().as_slice() {
                [_, "A", _] => format!("Arr {code}"),
                [_, "D", _] => format!("Dep {code}"),
                _ => format!("ATIS {code}"),
            };
            if i > 0 {
                flow(ui, line_left, line_right, super::pill_width(ui, &button));
            }
            let text = s.text.join(" ");
            let key = format!("atis:{}", s.callsign);
            let status = (*label == "Departure").then(|| atis_status(m, &s.callsign)).flatten();
            if expandable(ui, m, &button, &key, &text, status.as_ref().map(|(_, note)| note.as_str())) {
                open.push((s.callsign.clone(), text));
            }
            // The letter check, as a tick or warning right after the button.
            if let Some(status) = &status {
                ui.same_line();
                super::status_icon(ui, status);
            }
        }
        if let Some(metar) = m.state.network.metars.get(icao).cloned() {
            flow(ui, line_left, line_right, super::pill_width(ui, "METAR"));
            if expandable(ui, m, "METAR", &format!("metar:{icao}"), &metar, None) {
                open.push((format!("{icao} METAR"), metar.clone()));
            }
            // Departure and arrival QNH live on the Clearance and Arrival cards; only the
            // nearest airport (no flight loaded) shows it here.
            if *label == "Nearest"
                && let Some(qnh) = qnh_from_metar(&metar)
            {
                flow(ui, line_left, line_right, ui.calc_text_size(&qnh)[0]);
                ui.text_disabled(qnh);
            }
        }
        for event in m.state.network.events.iter().filter(|e| e.involves(icao) && e.start < now + 24 * 3600) {
            let (text, color) =
                if event.is_live(now) { ("EVENT LIVE".to_string(), theme::OK) } else { (format!("EVENT {}", format_utc(event.start, now)), theme::WARN) };
            flow(ui, line_left, line_right, ui.calc_text_size(&text)[0]);
            ui.text_colored(color, text);
            if ui.is_item_hovered() {
                wrapped_tooltip(ui, &format!("{}\n{}", event.name, event.summary));
            }
        }
        if let Some(group) = group {
            group.end();
            bottom = bottom.max(ui.cursor_pos()[1]);
            opened.extend(open);
            continue;
        }
        // Texts the pilot clicked open, under the row and aligned with the airport.
        opened_texts(ui, icao_x, open);
    }
    if columns {
        super::below(ui, base, bottom - ui.clone_style().item_spacing()[1], 0.0);
        opened_texts(ui, base, opened);
    }

    route(ui, m, icao_x, narrow);
    if rows.is_empty() {
        ui.text_disabled("Use Import flight (top right) to load your plan.");
    }
}

/// ATIS and METAR texts the pilot clicked open, each under its title, starting at `x`.
fn opened_texts(ui: &Ui, x: f32, texts: Vec<(String, String)>) {
    for (title, text) in texts {
        ui.set_cursor_pos([x, ui.cursor_pos()[1]]);
        ui.group(|| {
            let _wrap = ui.push_text_wrap_pos(0.0);
            ui.text_disabled(&title);
            ui.text(&text);
        });
        ui.spacing();
    }
}

/// The full route: as filed on VATSIM (what ATC sees) when there is one, with the SID and STAR
/// from SimBrief when the filing leaves them out, else SimBrief's. Its source and the alternate
/// are behind an info icon. If the two differ, a warning and the SimBrief route on demand.
fn route(ui: &Ui, m: &mut Model, text_x: f32, narrow: bool) {
    let filed = m.filed_plan();
    let simbrief = m.simbrief.clone().filter(|p| !p.route.trim().is_empty());
    let Some((text, source)) = m.route_text() else { return };
    let alternate = filed.as_ref().map(|fp| fp.alternate.clone()).or_else(|| simbrief.as_ref().map(|p| p.alternate.clone())).unwrap_or_default();
    ui.spacing();
    super::caption(ui, m, "Route");
    if !narrow {
        ui.same_line_with_pos(text_x);
    }
    // Where the route came from, and the alternate, behind an info icon before it.
    let alternate = alternate.trim();
    let details = if alternate.is_empty() { source.to_string() } else { format!("{source}\nAlternate {alternate}") };
    ui.group(|| {
        super::info_icon(ui, &details);
        ui.same_line();
        ui.group(|| {
            {
                let _wrap = ui.push_text_wrap_pos(0.0);
                ui.text(&text);
            }
            // A warning only when the filed route isn't the SimBrief one; it flows onto further
            // lines rather than squeezing.
            if let (Some(fp), Some(plan)) = (&filed, &simbrief)
                && !sidetone_core::clearance::routes_match(&fp.route, &plan.route)
            {
                let left = ui.cursor_screen_pos()[0];
                let right = left + ui.content_region_avail()[0];
                let warning = format!("{} Differs from SimBrief", icon::ALERT);
                ui.text_colored(theme::WARN, &warning);
                flow(ui, left, right, super::pill_width(ui, "SimBrief route"));
                if expandable(ui, m, "SimBrief route", "route:simbrief", plan.route.trim(), None) {
                    let _wrap = ui.push_text_wrap_pos(0.0);
                    ui.text(plan.route.trim());
                }
            }
        });
    });
}

/// A button that shows `text` on hover and keeps it open on click. Returns whether it's open.
/// `note` leads the tooltip (the ATIS letter check).
fn expandable(ui: &Ui, m: &mut Model, label: &str, key: &str, text: &str, note: Option<&str>) -> bool {
    let open = m.ui.expanded.contains(key);
    if toggle_button(ui, &format!("{label}##{key}"), open) && !m.ui.expanded.remove(key) {
        m.ui.expanded.insert(key.to_string());
    }
    if ui.is_item_hovered() {
        let note = note.map(|note| format!("{note}\n\n")).unwrap_or_default();
        let body = if open || text.is_empty() { String::new() } else { format!("{text}\n\nClick to keep it open") };
        let tip = format!("{note}{body}");
        if !tip.trim().is_empty() {
            wrapped_tooltip(ui, tip.trim_end());
        }
    }
    m.ui.expanded.contains(key)
}

/// The ATIS letter check, for the departure ATIS station only.
fn atis_status(m: &Model, callsign: &str) -> Option<super::Check> {
    let (_, station) = m.departure_atis()?;
    if station == callsign { m.atis_check() } else { None }
}

fn along_route(ui: &Ui, m: &mut Model) {
    let legs = Arc::clone(&m.state.network.route_atc);
    let online = m.state.network.snapshot.as_ref().map(|s| s.stations.len()).unwrap_or(0);
    let see_all = format!("{} All ({online})", icon::LIST);
    let title = if legs.is_empty() {
        "ATC ALONG ROUTE".to_string()
    } else {
        // Staffed areas out of all areas on the route.
        let covered = legs.iter().filter(|l| !l.online.is_empty()).count();
        format!("ATC ALONG ROUTE · {covered}/{}", legs.len())
    };
    let (open, clicked) = fold(ui, m, Fold::RouteAtc, &title, &[(&see_all, "Every station online, with search")]);
    if clicked.is_some() {
        m.ui.select_tab = Some(Tab::Atc);
    }
    if !open {
        return;
    }
    if legs.is_empty() {
        ui.text_disabled("Shows who's staffed along the way once you have a departure and arrival.");
        return;
    }
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let narrow = super::narrow(ui);
    // Only staffed areas are listed (the count says how many aren't). Nobody at all: one line
    // with UNICOM. Nobody where you are now, but someone later: UNICOM first, then them.
    let here = legs.get(current_leg(m, &legs)).filter(|l| l.online.is_empty());
    if legs.iter().all(|l| l.online.is_empty()) {
        unicom_line(ui, m, "No ATC along your route", here.map(|l| l.label.as_str()));
        return;
    }
    if let Some(leg) = here {
        unicom_line(ui, m, &format!("No ATC in {}", leg.label), Some(&leg.label));
    }
    // Areas you've passed fold away by themselves (unless you've opened or folded one); the one
    // you're in is marked. Without a position nothing counts as passed.
    let current = m.state.position.map(|_| current_leg(m, &legs));
    for (i, leg) in legs.iter().enumerate().filter(|(_, l)| !l.online.is_empty()) {
        let _id = ui.push_id(i);
        let passed = current.is_some_and(|c| i < c);
        let open = m.ui.area_folds.get(&leg.id).copied().unwrap_or(!passed);
        if staffed_area(ui, m, leg, narrow, unit, open, current == Some(i)) {
            m.ui.area_folds.insert(leg.id.clone(), !open);
        }
    }
}

/// No ATC where you are: say so (`text`), and offer UNICOM. Hover explains any similarly named
/// units in `area` that don't count (approach only).
fn unicom_line(ui: &Ui, m: &mut Model, text: &str, area: Option<&str>) {
    ui.align_text_to_frame_padding();
    let left = ui.cursor_screen_pos()[0];
    let right = left + ui.content_region_avail()[0];
    ui.text_colored(theme::WARN, "●");
    flow(ui, left, right, ui.calc_text_size(text)[0]);
    ui.text_disabled(text);
    if let Some(area) = area
        && ui.is_item_hovered()
    {
        let stations = m.state.network.snapshot.as_ref().map(|s| s.stations.as_slice()).unwrap_or_default();
        wrapped_tooltip(ui, &unstaffed_note(area, stations));
    }
    let unicom = sidetone_vatsim::freq::UNICOM_KHZ;
    if m.state.com1.active_khz == unicom || m.state.com2.active_khz == unicom {
        let text = format!("{} On UNICOM", icon::CHECK);
        flow(ui, left, right, ui.calc_text_size(&text)[0]);
        ui.text_colored(theme::OK, text);
    } else {
        flow(ui, left, right, super::pill_width(ui, "Switch to UNICOM"));
        if super::pill(ui, "Switch to UNICOM") {
            m.actions.push(Action::Tune { com: Com::One, khz: unicom });
        }
        if ui.is_item_hovered() {
            wrapped_tooltip(ui, "Tune COM1 to 122.800 and announce your intentions in text or voice");
        }
    }
}

/// A staffed area and every station covering it as tune-on-click chips. Its header folds it
/// (click the arrow or name); folded it's one line with how many stations. Wide windows put
/// the chips in a column on the area's line; narrow ones always start them on the line below
/// (indented). Either way, further chips wrap under the first. Returns true when the pilot
/// clicked the header.
fn staffed_area(ui: &Ui, m: &mut Model, leg: &sidetone_vatsim::coverage::RouteLeg, narrow: bool, unit: f32, open: bool, now: bool) -> bool {
    let left = ui.cursor_screen_pos()[0];
    let top = ui.cursor_screen_pos()[1];
    let right_edge = left + ui.content_region_avail()[0];
    // The header: arrow, dot, name, "now", and the count when folded. One click target.
    let header_right = if narrow || !open { right_edge } else { ui.window_pos()[0] + 150.0 * unit - 8.0 * unit };
    let clicked = ui.invisible_button("##area", [(header_right - left).max(40.0 * unit), ui.frame_height()]);
    let hovered = ui.is_item_hovered();
    let arrow = [left + 4.0 * unit, top + ui.frame_height() * 0.5];
    let color = if hovered { theme::TEXT } else { theme::TEXT_DIM };
    let s = 3.5 * unit;
    let points = if open {
        [[arrow[0] - s, arrow[1] - s * 0.6], [arrow[0] + s, arrow[1] - s * 0.6], [arrow[0], arrow[1] + s * 0.8]]
    } else {
        [[arrow[0] - s * 0.6, arrow[1] - s], [arrow[0] - s * 0.6, arrow[1] + s], [arrow[0] + s * 0.8, arrow[1]]]
    };
    ui.get_window_draw_list().add_triangle(points[0], points[1], points[2], color).filled(true).build();
    ui.set_cursor_screen_pos([left + 14.0 * unit, top]);
    ui.align_text_to_frame_padding();
    ui.text_colored(theme::OK, "●");
    ui.same_line();
    ui.text(&leg.label);
    if now {
        ui.same_line();
        ui.text_colored(theme::ACCENT, "· now");
    }
    if !open {
        let n = leg.online.len();
        ui.same_line();
        ui.text_disabled(format!("· {n} station{}", if n == 1 { "" } else { "s" }));
        return clicked;
    }
    let row_start = if narrow {
        let row_start = left + 22.0 * unit;
        let y = ui.cursor_screen_pos()[1];
        ui.set_cursor_screen_pos([row_start, y]);
        row_start
    } else {
        ui.same_line_with_pos(150.0 * unit);
        ui.cursor_screen_pos()[0]
    };
    let mut first = true;
    for (callsign, name, khz) in leg.online.iter() {
        let _id = ui.push_id(callsign.as_str());
        let label = format!("{name} {}", format_com_khz(*khz));
        chip_place(ui, &mut first, row_start, right_edge, super::pill_width(ui, &label));
        if super::pill(ui, &label) {
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
            wrapped_tooltip(ui, &format!("{callsign}\n{source}\nClick to tune COM1"));
        }
        // Other frequencies the controller also transmits on: a sector they're covering
        // ("contact Muenchen Radar 129.525" from EDMM_BBG on 133.615, seen live).
        let extra: Vec<i32> = m
            .state
            .network
            .snapshot
            .as_ref()
            .and_then(|snap| snap.stations.iter().find(|s| s.callsign == *callsign))
            .map(|s| {
                let main = sidetone_vatsim::freq::channel_to_hz(*khz);
                let mut extra: Vec<i32> = s
                    .frequencies_hz
                    .iter()
                    .filter(|hz| !sidetone_vatsim::freq::same_frequency(**hz, main))
                    .map(|hz| sidetone_vatsim::freq::hz_to_channel(*hz))
                    .collect();
                extra.sort_unstable();
                extra.dedup();
                extra
            })
            .unwrap_or_default();
        for other in extra {
            let label = format_com_khz(other);
            chip_place(ui, &mut first, row_start, right_edge, super::pill_width(ui, &label));
            if super::pill(ui, &label) {
                m.actions.push(Action::Tune { com: Com::One, khz: other });
            }
            if ui.is_item_hovered() {
                wrapped_tooltip(ui, &format!("{callsign} also transmits on {label}\n(another sector they're covering)\nClick to tune COM1"));
            }
        }
    }
    clicked
}

/// Which route area you're in now: the departure while on the ground, else the first area
/// whose airspace contains you (0 when unknown).
fn current_leg(m: &Model, legs: &[sidetone_vatsim::coverage::RouteLeg]) -> usize {
    if m.state.on_ground {
        return 0;
    }
    let (Some(pos), Some(boundaries)) = (m.state.position, &m.state.network.boundaries) else { return 0 };
    let firs: Vec<&str> = boundaries.containing(pos).map(|b| b.id.split('-').next().unwrap_or(&b.id)).collect();
    legs.iter().position(|l| firs.contains(&l.id.as_str())).unwrap_or(0)
}

/// Why an area reads "unstaffed" when a station with its name is online: in Germany every DFS
/// unit is "Langen Radar", but an approach unit only works its own terminal area. Two lines at most.
fn unstaffed_note(label: &str, stations: &[sidetone_vatsim::stations::Station]) -> String {
    let area = label.split_whitespace().next().unwrap_or(label).to_ascii_lowercase();
    let approach: Vec<&sidetone_vatsim::stations::Station> =
        stations.iter().filter(|s| matches!(s.facility, Facility::Approach | Facility::Departure) && s.name.to_ascii_lowercase().starts_with(&area)).collect();
    let mut note = format!("No {label} en-route controller online.");
    if let Some(first) = approach.first() {
        let mut places: Vec<String> = approach
            .iter()
            .map(|s| s.qualifier.as_deref().map(|q| q.trim_end_matches(" APP").trim_end_matches(" DEP").to_string()).unwrap_or_else(|| s.callsign.clone()))
            .collect();
        places.dedup();
        let places = match places.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => places.join(""),
        };
        note.push_str(&format!("\n{} at {places} is approach only.", first.name));
    }
    note
}

/// Places the next chip on the current line if it fits, otherwise on a new line indented to `row_start`.
fn chip_place(ui: &Ui, first: &mut bool, row_start: f32, right_edge: f32, width: f32) {
    if !std::mem::replace(first, false) {
        flow(ui, row_start, right_edge, width);
    }
}

/// Free notes for this flight (taxi route, reminders); cleared by the next import.
fn notes(ui: &Ui, m: &mut Model) {
    if !fold(ui, m, Fold::Notes, "NOTES", &[]).0 {
        return;
    }
    // The card is sized to the rest of the tab; the notes fill it.
    let avail = ui.content_region_avail();
    ui.input_text_multiline("##notes", &mut m.settings.flight.notes, avail).build();
    if ui.is_item_deactivated_after_edit() {
        m.actions.push(Action::SaveSettings);
    }
}
