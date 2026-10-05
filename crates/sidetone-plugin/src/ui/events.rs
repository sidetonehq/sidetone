//! Events tab: upcoming VATSIM events, yours highlighted.

use super::{Model, section};
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;
use sidetone_vatsim::time::{format_utc, now_unix};

/// Show events starting within this many hours.
const HORIZON_HOURS: i64 = 48;

pub fn build(ui: &Ui, m: &mut Model) {
    let now = now_unix();
    let airports = m.state.watched_airports();
    let events = m.state.network.events.clone();
    let upcoming: Vec<_> = events.iter().filter(|e| e.start < now + HORIZON_HOURS * 3600).collect();
    let (mine, others): (Vec<_>, Vec<_>) = upcoming.into_iter().partition(|e| airports.iter().any(|a| e.involves(a)));

    if events.is_empty() {
        ui.spacing();
        ui.text_disabled("Loading events…");
        return;
    }
    if !mine.is_empty() {
        section(ui, "AT YOUR AIRPORTS");
        for e in &mine {
            row(ui, e, now, true);
        }
    }
    section(ui, &format!("NEXT {HORIZON_HOURS} HOURS"));
    ui.child_window("events").build(ui, || {
        if others.is_empty() {
            ui.text_disabled("Nothing else scheduled.");
        }
        for e in &others {
            row(ui, e, now, false);
        }
    });
}

fn row(ui: &Ui, e: &sidetone_vatsim::events::Event, now: i64, mine: bool) {
    let _id = ui.push_id(e.id as usize);
    if e.is_live(now) {
        ui.text_colored(theme::OK, "LIVE");
    } else {
        ui.text_disabled(format_utc(e.start, now));
    }
    ui.same_line_with_pos(90.0 * ui.current_font_size() / theme::FONT_SIZE);
    if mine {
        ui.text_colored(theme::ACCENT, &e.name);
    } else {
        ui.text(&e.name);
    }
    if ui.is_item_hovered() && !e.summary.is_empty() {
        ui.tooltip_text(&e.summary);
    }
    ui.same_line();
    ui.text_disabled(e.airports.join(" "));
    if !e.link.is_empty() {
        ui.same_line();
        if ui.small_button("Details") {
            // Opens in the default browser; spawn returns immediately.
            if let Err(err) = std::process::Command::new("open").arg(&e.link).spawn() {
                log::warn!("Could not open {}: {err}", e.link);
            }
        }
    }
}
