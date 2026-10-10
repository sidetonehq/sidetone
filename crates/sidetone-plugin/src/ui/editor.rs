//! The "Enter clearance" and "Arrival" popups: your route for reference, then the fields.
//! Opened from the Flight tab via `m.ui.open_editor`.

use super::Model;
use crate::app::Action;
use sidetone_ui::imgui::{InputTextFlags, TableColumnFlags, TableFlags, Ui, WindowFlags};
use sidetone_ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Clearance,
    Arrival,
}

impl Form {
    fn title(self) -> &'static str {
        match self {
            Form::Clearance => "Enter clearance",
            Form::Arrival => "Arrival",
        }
    }
}

/// Call once per frame from the main window, outside any tab.
pub fn build(ui: &Ui, m: &mut Model) {
    if let Some(form) = m.ui.open_editor.take() {
        ui.open_popup(form.title());
    }
    for form in [Form::Clearance, Form::Arrival] {
        popup(ui, m, form);
    }
}

fn popup(ui: &Ui, m: &mut Model, form: Form) {
    let flags = WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS;
    let Some(_popup) = ui.begin_modal_popup_config(form.title()).flags(flags).begin() else { return };

    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let grid = Grid { label_w: 95.0 * unit, field_w: 110.0 * unit, column_w: 229.0 * unit, columns: if super::narrow(ui) { 1 } else { 2 } };
    let width = grid.column_w * grid.columns as f32;

    route(ui, m, width);
    let edited = match form {
        Form::Clearance => clearance_fields(ui, m, &grid),
        Form::Arrival => arrival_fields(ui, m, &grid),
    };
    if edited {
        m.actions.push(Action::SaveSettings);
    }

    ui.spacing();
    ui.separator();
    ui.spacing();
    let style = ui.clone_style();
    let done_w = ui.calc_text_size("Done")[0] + style.frame_padding()[0] * 2.0;
    ui.set_cursor_pos([width - done_w + style.window_padding()[0], ui.cursor_pos()[1]]);
    if ui.button("Done") {
        m.actions.push(Action::SaveSettings);
        ui.close_current_popup();
    }
}

/// The route you'll fly, so the SID (or STAR) and nearby fixes are in view while you note it.
fn route(ui: &Ui, m: &Model, width: f32) {
    let Some((text, source)) = m.route_text() else { return };
    ui.text_disabled(format!("Your route · {source}"));
    let _wrap = ui.push_text_wrap_pos(ui.cursor_pos()[0] + width);
    ui.text(&text);
    ui.spacing();
    ui.separator();
    ui.spacing();
}

/// Label + field cells in one or two fixed-width columns (one in a narrow window).
struct Grid {
    label_w: f32,
    field_w: f32,
    column_w: f32,
    columns: usize,
}

type Row<'a> = (&'a str, &'a str, &'a mut String, InputTextFlags);

impl Grid {
    /// Draws the rows; returns which ones were just edited.
    fn rows(&self, ui: &Ui, id: &str, rows: Vec<Row>) -> Vec<bool> {
        let mut edited = vec![false; rows.len()];
        let Some(_t) = ui.begin_table_with_flags(format!("{id}{}", self.columns), self.columns, TableFlags::NONE) else { return edited };
        for c in 0..self.columns {
            ui.table_setup_column_fixed_width(format!("##c{c}"), TableColumnFlags::NONE, self.column_w);
        }
        for (i, (label, hint, value, flags)) in rows.into_iter().enumerate() {
            ui.table_next_column();
            ui.align_text_to_frame_padding();
            ui.text_disabled(label);
            ui.same_line_with_pos(self.label_w); // measured from the column start
            ui.set_next_item_width(self.field_w);
            ui.input_text(format!("##{label}"), value).hint(hint).flags(flags).build();
            edited[i] = ui.is_item_deactivated_after_edit();
        }
        edited
    }
}

fn clearance_fields(ui: &Ui, m: &mut Model, grid: &Grid) -> bool {
    let upper = InputTextFlags::CHARS_UPPERCASE;
    let f = &mut m.settings.flight;
    let edited = grid.rows(
        ui,
        "clearance",
        vec![
            // The Clearance card's order.
            ("SID", "e.g. CPT3J", &mut f.sid, upper),
            ("Initial alt", "e.g. FL060", &mut f.initial_altitude, upper),
            ("Squawk", "e.g. 4721", &mut f.squawk, InputTextFlags::CHARS_DECIMAL),
            ("Runway", "e.g. 27R", &mut f.runway, upper),
            ("QNH", "e.g. Q1022", &mut f.qnh, upper),
            ("ATIS", "e.g. C", &mut f.atis, upper),
            ("Trans level", "e.g. FL070", &mut f.transition_level, upper),
            ("Stand", "e.g. 512", &mut f.stand, upper),
        ],
    );
    // A typed value stops auto-tracking; clearing the field turns it back on.
    if edited[4] {
        f.qnh_auto = f.qnh.trim().is_empty();
    }
    if edited[5] {
        f.atis_auto = f.atis.trim().is_empty();
    }
    if edited[6] {
        f.transition_level_auto = f.transition_level.trim().is_empty();
    }
    edited.iter().any(|e| *e)
}

fn arrival_fields(ui: &Ui, m: &mut Model, grid: &Grid) -> bool {
    let upper = InputTextFlags::CHARS_UPPERCASE;
    let a = &mut m.settings.flight.arrival;
    let edited = grid.rows(
        ui,
        "arrival",
        vec![
            // The Arrival card's order.
            ("STAR", "e.g. ELKA3A", &mut a.star, upper),
            ("Approach", "e.g. ILS 22", &mut a.approach, upper),
            ("Frequency", "e.g. 120.130", &mut a.frequency, InputTextFlags::NONE),
            ("Runway", "e.g. 22", &mut a.runway, upper),
            ("QNH", "e.g. Q1018", &mut a.qnh, upper),
            ("ATIS", "e.g. D", &mut a.atis, upper),
            ("Trans level", "e.g. FL070", &mut a.transition_level, upper),
            ("Stand", "e.g. 12", &mut a.stand, upper),
        ],
    );
    if edited[4] {
        a.qnh_auto = a.qnh.trim().is_empty();
    }
    if edited[5] {
        a.atis_auto = a.atis.trim().is_empty();
    }
    if edited[6] {
        a.transition_level_auto = a.transition_level.trim().is_empty();
    }
    edited.iter().any(|e| *e)
}
