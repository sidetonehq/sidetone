//! The Flight tab's Clearance section. Until you're cleared it shows who to call; once you've
//! entered a clearance it shows it at a glance, with a tick or warning on the squawk (against
//! the transponder) and the ATIS letter. Sidetone never changes cockpit settings for you.

use super::{Fold, Model, flow, fold, strong, tune_buttons, tune_buttons_width};
use sidetone_core::clearance::{display_level, display_qnh};
use sidetone_core::radio::format_com_khz;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;
use sidetone_ui::theme::icon;
use sidetone_vatsim::naming::Facility;

pub fn build(ui: &Ui, m: &mut Model) {
    let f = &m.settings.flight;
    // In the order ATC reads a clearance ("via the SID, climb FL060, squawk 3432"), then what you
    // set up for departure. Runway, QNH, ATIS, TL and Stand sit where they do on the Arrival card.
    let values: Vec<super::Value> = vec![
        ("SID", f.sid.trim().to_string(), None),
        ("Initial", display_level(&f.initial_altitude), None),
        ("Squawk", f.squawk.trim().to_string(), m.squawk_check()),
        ("Runway", f.runway.trim().to_string(), None),
        ("QNH", display_qnh(&f.qnh), None),
        ("ATIS", f.atis.trim().to_string(), m.atis_check()),
        ("TL", display_level(&f.transition_level), None),
        ("Stand", f.stand.trim().to_string(), None),
    ];
    // Squawk is what ATC gives you; until then, help get the clearance.
    let cleared = !m.settings.flight.squawk.trim().is_empty();

    let action = format!("{} {}", icon::EDIT, if cleared { "Edit" } else { "Enter clearance" });
    let (open, clicked) = fold(ui, m, Fold::Clearance, "CLEARANCE", &[(&action, "Note the clearance you were given")]);
    if clicked.is_some() {
        m.ui.open_editor = Some(super::editor::Form::Clearance);
    }
    if !open {
        return;
    }
    if cleared {
        super::value_tiles(ui, m, &values);
    } else {
        request(ui, m);
    }
}

/// Who to call for the clearance at your departure, with one-click tuning.
fn request(ui: &Ui, m: &mut Model) {
    let Some(dep) = m.state.route.departure.clone() else {
        ui.text_disabled("Import your flight (top right) to see who to call.");
        return;
    };
    // Delivery if online, else the first ground/tower unit in contact order.
    let order = [Facility::Delivery, Facility::Ground, Facility::Apron, Facility::Tower];
    let station = m.state.network.snapshot.as_ref().and_then(|snap| {
        order.iter().find_map(|f| snap.stations.iter().find(|s| s.facility == *f && s.callsign.split('_').next() == Some(dep.as_str())).cloned())
    });
    ui.align_text_to_frame_padding();
    let left = ui.cursor_screen_pos()[0];
    let right = left + ui.content_region_avail()[0];
    super::caption(ui, m, "Call");
    // Narrow windows stack the label above its value, like the rest of the Flight tab.
    let narrow = super::narrow(ui);
    match &station {
        Some(s) => {
            let name = format!("{} {}", s.display_name(), format_com_khz(s.frequency_khz));
            if !narrow {
                flow(ui, left, right, ui.calc_text_size(&name)[0]);
            }
            strong(ui, m, theme::TEXT, &name);
            flow(ui, left, right, tune_buttons_width(ui));
            tune_buttons(ui, m, s.frequency_khz);
        }
        None => {
            if !narrow {
                ui.same_line();
            }
            ui.group(|| {
                let _wrap = ui.push_text_wrap_pos(0.0);
                ui.text_disabled("No Delivery, Ground or Tower online. Try UNICOM 122.800, or a PDC from your aircraft.");
            });
        }
    }
}
