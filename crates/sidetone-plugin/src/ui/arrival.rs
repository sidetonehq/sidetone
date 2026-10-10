//! The Flight tab's Arrival section: runway and STAR from SimBrief, ATIS letter, QNH and
//! transition level from the arrival ATIS and METAR, and what you note as you're told (approach,
//! stand, the frequency to call, with one-click tuning). Opens itself at takeoff.

use super::editor::Form;
use super::{Fold, Model, flow, fold, tune_buttons, tune_buttons_width, value_tiles};
use sidetone_core::clearance::{display_level, display_qnh};
use sidetone_core::radio::{format_com_khz, parse_com_khz};
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme::icon;

pub fn build(ui: &Ui, m: &mut Model) {
    let arrival = m.state.route.arrival.clone();
    let title = arrival.as_ref().map_or("ARRIVAL".to_string(), |icao| format!("ARRIVAL · {icao}"));
    let edit = format!("{} Edit", icon::EDIT);
    let (open, clicked) = fold(ui, m, Fold::Arrival, &title, &[(&edit, "Note your runway, approach, stand and frequency")]);
    if clicked.is_some() {
        m.ui.open_editor = Some(Form::Arrival);
    }
    if !open {
        return;
    }
    if arrival.is_none() {
        ui.text_disabled("Import your flight (top right) to plan the arrival.");
        return;
    }

    let a = &m.settings.flight.arrival;
    let frequency = a.frequency.trim().to_string();
    let khz = parse_com_khz(&frequency);
    // What you'll fly, then the same places as the Clearance card for Runway, QNH, ATIS, TL and Stand.
    let values: Vec<super::Value> = vec![
        ("STAR", a.star.trim().to_string(), None),
        ("Approach", a.approach.trim().to_string(), None),
        ("Frequency", khz.map(format_com_khz).unwrap_or(frequency), None),
        ("Runway", a.runway.trim().to_string(), None),
        ("QNH", display_qnh(&a.qnh), None),
        ("ATIS", a.atis.trim().to_string(), m.arrival_atis_check()),
        ("TL", display_level(&a.transition_level), None),
        ("Stand", a.stand.trim().to_string(), None),
    ];
    value_tiles(ui, m, &values);

    // The frequency you were told to call, ready to tune.
    if let Some(khz) = khz {
        ui.align_text_to_frame_padding();
        let left = ui.cursor_screen_pos()[0];
        let right = left + ui.content_region_avail()[0];
        ui.text_disabled(format!("Tune {}", format_com_khz(khz)));
        flow(ui, left, right, tune_buttons_width(ui));
        tune_buttons(ui, m, khz);
    }
}
