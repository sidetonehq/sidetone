//! CPDLC tab: log on to an ATC unit via Hoppie, answer uplinks, send requests and PDCs.

use super::Model;
use crate::app::{Action, HoppieAction};
use sidetone_services::hoppie::{Direction, LogonState};
use sidetone_ui::imgui::{InputTextFlags, Ui};
use sidetone_ui::theme;

pub fn build(ui: &Ui, m: &mut Model) {
    ui.spacing();
    if !m.hoppie_ready {
        ui.text_disabled("Add your Hoppie logon code in Settings to use CPDLC and PDC.");
        return;
    }
    let unit = ui.current_font_size() / theme::FONT_SIZE;

    // Identity + connection.
    let logged_on = m.hoppie.station().is_some();
    ui.align_text_to_frame_padding();
    ui.text_disabled("Callsign");
    ui.same_line();
    ui.set_next_item_width(90.0 * unit);
    let auto = m.datalink_callsign();
    {
        let _disabled = ui.begin_disabled_with_cond(logged_on);
        ui.input_text("##cs", &mut m.ui.cpdlc_callsign)
            .hint(if auto.is_empty() { "e.g. BAW123".to_string() } else { auto })
            .flags(InputTextFlags::CHARS_UPPERCASE)
            .build();
    }
    ui.same_line();
    match m.hoppie.state.clone() {
        LogonState::Off => {
            ui.set_next_item_width(70.0 * unit);
            ui.input_text("##station", &mut m.ui.cpdlc_station).hint("EGTT").flags(InputTextFlags::CHARS_UPPERCASE).build();
            ui.same_line();
            if ui.button("Log on") {
                m.actions.push(Action::Hoppie(HoppieAction::Logon(m.ui.cpdlc_station.clone())));
            }
            ui.same_line();
            ui.text_disabled("Not logged on");
        }
        LogonState::Pending { station } => {
            ui.text_colored(theme::WARN, format!("Logon to {station} pending…"));
            ui.same_line();
            if ui.small_button("Cancel") {
                m.actions.push(Action::Hoppie(HoppieAction::Logoff));
            }
        }
        LogonState::Connected { station } => {
            ui.text_colored(theme::OK, format!("Connected to {station}"));
            ui.same_line();
            if ui.small_button("Log off") {
                m.actions.push(Action::Hoppie(HoppieAction::Logoff));
            }
        }
    }
    if let Some(e) = &m.hoppie_error {
        ui.text_colored(theme::DANGER, e);
    }
    ui.separator();

    // Message log.
    let compose_height = ui.frame_height_with_spacing() * 3.0 + 8.0 * unit;
    let avail = ui.content_region_avail();
    let mut replies: Vec<(u64, String)> = Vec::new();
    ui.child_window("datalink").size([avail[0], (avail[1] - compose_height).max(60.0)]).build(ui, || {
        if m.hoppie.messages.is_empty() {
            ui.text_disabled("No datalink messages yet.");
        }
        for msg in &m.hoppie.messages {
            let _id = ui.push_id(msg.id as usize);
            let (arrow, color) = match msg.direction {
                Direction::Up => ("▼", theme::ACCENT),
                Direction::Down => ("▲", theme::TEXT_DIM),
            };
            ui.text_colored(color, format!("{arrow} {}", msg.station));
            ui.same_line();
            ui.text_disabled(clock(msg.time));
            if msg.direction == Direction::Down && msg.answered {
                ui.same_line();
                ui.text_disabled("· answered");
            }
            ui.text_wrapped(&msg.text);
            if msg.needs_reply() {
                for reply in msg.ra.replies() {
                    if ui.small_button(reply) {
                        replies.push((msg.id, reply.to_string()));
                    }
                    ui.same_line();
                }
                ui.new_line();
            } else if let Some(reply) = &msg.reply {
                ui.text_colored(theme::OK, format!("Replied {reply}"));
            }
            ui.separator();
        }
        if ui.scroll_y() >= ui.scroll_max_y() - 1.0 {
            ui.set_scroll_here_y(1.0);
        }
    });
    for (id, reply) in replies {
        m.actions.push(Action::Hoppie(HoppieAction::Reply { id, reply }));
    }

    // Compose.
    {
        let _disabled = ui.begin_disabled_with_cond(!matches!(m.hoppie.state, LogonState::Connected { .. }));
        ui.set_next_item_width(70.0 * unit);
        ui.input_text("##lvl", &mut m.ui.cpdlc_level).hint("FL350").flags(InputTextFlags::CHARS_UPPERCASE).build();
        ui.same_line();
        if ui.button("Request climb/descent") && !m.ui.cpdlc_level.trim().is_empty() {
            let level = m.ui.cpdlc_level.trim().to_string();
            m.actions.push(Action::Hoppie(HoppieAction::Request(format!("REQUEST {level}"))));
            m.ui.cpdlc_level.clear();
        }
        ui.same_line();
        ui.set_next_item_width(80.0 * unit);
        ui.input_text("##dct", &mut m.ui.cpdlc_direct).hint("WAYPOINT").flags(InputTextFlags::CHARS_UPPERCASE).build();
        ui.same_line();
        if ui.button("Request direct") && !m.ui.cpdlc_direct.trim().is_empty() {
            let wpt = m.ui.cpdlc_direct.trim().to_string();
            m.actions.push(Action::Hoppie(HoppieAction::Request(format!("REQUEST DIRECT TO {wpt}"))));
            m.ui.cpdlc_direct.clear();
        }
        ui.set_next_item_width(-1.0);
        let sent = ui
            .input_text("##free", &mut m.ui.cpdlc_text)
            .hint("Free text request to ATC")
            .enter_returns_true(true)
            .flags(InputTextFlags::CHARS_UPPERCASE)
            .build();
        if sent && !m.ui.cpdlc_text.trim().is_empty() {
            let text = std::mem::take(&mut m.ui.cpdlc_text);
            m.actions.push(Action::Hoppie(HoppieAction::Request(text)));
        }
    }

    // PDC works without a CPDLC logon (it's a telex to the departure airport).
    let dep = m.state.route.departure.clone().unwrap_or_default();
    ui.align_text_to_frame_padding();
    ui.text_disabled(if dep.is_empty() { "PDC: set a departure first".to_string() } else { format!("PDC from {dep}") });
    ui.same_line();
    ui.set_next_item_width(60.0 * unit);
    ui.input_text("##stand", &mut m.ui.pdc_stand).hint("Stand").flags(InputTextFlags::CHARS_UPPERCASE).build();
    ui.same_line();
    let _disabled = ui.begin_disabled_with_cond(dep.is_empty());
    if ui.button("Request PDC") {
        m.actions.push(Action::Hoppie(HoppieAction::Pdc { station: dep, stand: m.ui.pdc_stand.trim().to_string() }));
    }
}

/// "14:05z" from unix seconds.
fn clock(secs: u64) -> String {
    let day = secs % 86_400;
    format!("{:02}:{:02}z", day / 3600, (day % 3600) / 60)
}
