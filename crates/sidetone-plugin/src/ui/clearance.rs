//! Clearance & notes card: the numbers you were given, plus free-form notes. Auto-filled from
//! clearances (PDC/CPDLC), SimBrief, the VATSIM feed and METARs — always editable.

use super::{Model, section};
use crate::app::Action;
use sidetone_core::radio::{format_squawk, is_valid_squawk};
use sidetone_ui::imgui::{InputTextFlags, Ui};
use sidetone_ui::theme;

pub fn build(ui: &Ui, m: &mut Model) {
    section(ui, "CLEARANCE");
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
    // Two columns: left = departure clearance, right = what you'll need next.
    if let Some(_t) = ui.begin_table("clearance", 2) {
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "Squawk", "e.g. 4721", &mut f.squawk, InputTextFlags::CHARS_DECIMAL);
        ui.table_next_column();
        field(ui, "Initial alt", "FL060 / 5000 ft", &mut f.initial_altitude, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "SID", "CPT3J", &mut f.sid, upper);
        ui.table_next_column();
        field(ui, "Runway", "27R", &mut f.runway, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "Dep freq", "120.525", &mut f.departure_freq, InputTextFlags::NONE);
        ui.table_next_column();
        field(ui, "QNH", "Q1022", &mut f.qnh, upper);
        ui.table_next_row();
        ui.table_next_column();
        field(ui, "ATIS", "C", &mut f.atis, upper);
        ui.table_next_column();
        field(ui, "Stand", "512", &mut f.stand, upper);
    }

    // Transponder check + one-click set.
    let squawk = m.settings.flight.squawk.trim().to_string();
    if is_valid_squawk(&squawk) {
        let current = format_squawk(m.state.transponder.code);
        if current == squawk {
            ui.text_colored(theme::OK, format!("Transponder set to {squawk}"));
        } else {
            ui.text_colored(theme::WARN, format!("Transponder is {current}, assigned {squawk}"));
            ui.same_line();
            if ui.small_button("Set transponder") {
                m.actions.push(Action::SubmitChat(format!(".x {squawk}")));
            }
        }
    } else if !squawk.is_empty() {
        ui.text_colored(theme::DANGER, "Squawk must be four digits 0–7");
    }

    let dep_freq = m.settings.flight.departure_freq.trim().to_string();
    if let Some(khz) = sidetone_core::radio::parse_com_khz(&dep_freq) {
        ui.same_line();
        if ui.small_button("Departure freq on COM2") {
            m.actions.push(Action::Tune { com: sidetone_core::dot_command::Com::Two, khz });
        }
    }

    section(ui, "NOTES");
    let avail = ui.content_region_avail();
    ui.input_text_multiline("##notes", &mut m.settings.flight.notes, [avail[0], (avail[1] - ui.frame_height_with_spacing() - 4.0).max(60.0)]).build();
    changed |= ui.is_item_deactivated_after_edit();

    if ui.button("New flight") {
        m.settings.flight = sidetone_core::clearance::FlightNotes::default();
        changed = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Clear the card for your next flight");
    }
    ui.same_line();
    ui.text_disabled("Auto-filled from your clearance, SimBrief, VATSIM and METARs. Your typing always wins.");
    if changed {
        m.actions.push(Action::SaveSettings);
    }
}
