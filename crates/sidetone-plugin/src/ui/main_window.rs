//! The full Sidetone window: ATC, Chat, Flight plan, Friends and Settings tabs.

use super::Model;
use crate::app::{Action, Redacted};
use sidetone_core::radio::{format_com_khz, format_squawk};
use sidetone_core::settings::Friend;
use sidetone_core::watch::friend_key;
use sidetone_services::keychain::Secret;
use sidetone_ui::imgui::{InputTextFlags, Ui};
use sidetone_ui::theme;
use sidetone_ui::{Fonts, HostFrame};

pub fn build(frame: &mut HostFrame, m: &mut Model) {
    let ui = frame.ui;
    header(ui, frame.fonts, m);
    ui.same_line();
    let label = if frame.popped_out { "Dock" } else { "Pop out" };
    let style = ui.clone_style();
    let w = ui.calc_text_size(label)[0] + style.frame_padding()[0] * 2.0;
    ui.set_cursor_pos([ui.window_size()[0] - w - style.window_padding()[0], ui.cursor_pos()[1]]);
    if ui.small_button(label) {
        frame.set_popped_out(!frame.popped_out);
    }
    ui.separator();

    if let Some(_bar) = ui.tab_bar("tabs") {
        if let Some(_t) = ui.tab_item("ATC") {
            super::atc::build(ui, m);
        }
        if let Some(_t) = ui.tab_item("Clearance") {
            super::clearance::build(ui, m);
        }
        let cpdlc_label = match m.hoppie.open_uplinks() {
            0 => "CPDLC###cpdlc".to_string(),
            n => format!("CPDLC ({n})###cpdlc"),
        };
        if let Some(_t) = ui.tab_item(cpdlc_label) {
            super::cpdlc::build(ui, m);
        }
        let chat_label = match m.state.unread {
            0 => "Messages###chat".to_string(),
            n => format!("Messages ({n})###chat"),
        };
        if let Some(_t) = ui.tab_item(chat_label) {
            m.state.unread = 0;
            chat(ui, m);
        }
        if let Some(_t) = ui.tab_item("Flight plan") {
            super::flight_plan::build(ui, m);
        }
        if let Some(_t) = ui.tab_item("Friends") {
            friends(ui, m);
        }
        if let Some(_t) = ui.tab_item("Events") {
            super::events::build(ui, m);
        }
        if let Some(_t) = ui.tab_item("Settings") {
            settings(ui, frame.fonts, m);
        }
    }
}

fn header(ui: &Ui, fonts: Fonts, m: &Model) {
    let pos = ui.cursor_screen_pos();
    let height = ui.current_font_size() * 1.25;
    let width = {
        let draw = ui.get_window_draw_list();
        theme::wordmark(ui, &draw, fonts, pos, height, theme::TEXT)
    };
    ui.dummy([width, height]);
    ui.same_line();
    ui.align_text_to_frame_padding();
    let via = m.state.connection_via.map(|v| format!(" via {v}")).unwrap_or_default();
    let traffic = m.state.nearby_aircraft.map(|n| format!("   {n} aircraft")).unwrap_or_default();
    ui.text_disabled(format!(
        "{}{via}   COM1 {}   COM2 {}   XPDR {} {}{traffic}",
        m.state.connection.label(),
        format_com_khz(m.state.com1.active_khz),
        format_com_khz(m.state.com2.active_khz),
        format_squawk(m.state.transponder.code),
        m.state.transponder.mode.label(),
    ));
}

fn chat(ui: &Ui, m: &mut Model) {
    let input_height = ui.frame_height_with_spacing();
    let avail = ui.content_region_avail();
    ui.child_window("messages").size([avail[0], avail[1] - input_height]).build(ui, || {
        for msg in &m.state.messages {
            let color = match msg.from.as_str() {
                "Sidetone" => theme::TEXT_DIM,
                "VATSIM" => theme::WARN,
                "CPDLC" => theme::OK,
                _ => theme::ACCENT,
            };
            ui.text_colored(color, &msg.from);
            ui.same_line();
            ui.text_wrapped(&msg.text);
        }
        if ui.scroll_y() >= ui.scroll_max_y() - 1.0 {
            ui.set_scroll_here_y(1.0);
        }
    });
    ui.set_next_item_width(-1.0);
    let sent = ui.input_text("##chat", &mut m.chat_input).hint("Message or dot command (.com1 122.8, .x 7000)").enter_returns_true(true).build();
    if sent {
        let text = std::mem::take(&mut m.chat_input);
        if !text.trim().is_empty() {
            m.actions.push(Action::SubmitChat(text));
        }
        // Enter ends editing and hands the keyboard back to X-Plane; click the box to type again.
    }
}

fn friends(ui: &Ui, m: &mut Model) {
    ui.spacing();
    match (m.settings.vatsim.cid, &m.state.network.stats) {
        (Some(cid), Some((stats_cid, stats))) if *stats_cid == cid => {
            ui.text_colored(theme::ACCENT, format!("You · {cid}"));
            ui.same_line();
            ui.text(format!("Pilot {:.0} h   ATC {:.0} h", stats.pilot, stats.atc));
        }
        (Some(cid), _) => ui.text_disabled(format!("You · {cid} · loading stats…")),
        (None, _) => ui.text_disabled("Add your VATSIM CID in Settings to see your hours."),
    }
    ui.separator();

    let unit = ui.current_font_size() / theme::FONT_SIZE;
    ui.set_next_item_width(140.0 * unit);
    let mut add = ui.input_text("##friend", &mut m.ui.friend_input).hint("CID or callsign").enter_returns_true(true).build();
    ui.same_line();
    ui.set_next_item_width(140.0 * unit);
    add |= ui.input_text("##friend_label", &mut m.ui.friend_label).hint("Name (optional)").enter_returns_true(true).build();
    ui.same_line();
    add |= ui.button("Add friend");
    if add {
        let id = m.ui.friend_input.trim().to_ascii_uppercase();
        if !id.is_empty() {
            let friend = match id.parse::<u32>() {
                Ok(cid) => Friend { cid: Some(cid), callsign: None, label: m.ui.friend_label.trim().to_string() },
                Err(_) => Friend { cid: None, callsign: Some(id), label: m.ui.friend_label.trim().to_string() },
            };
            if !m.settings.friends.iter().any(|f| friend_key(f) == friend_key(&friend)) {
                m.settings.friends.push(friend);
                m.actions.push(Action::SaveSettings);
            }
            m.ui.friend_input.clear();
            m.ui.friend_label.clear();
        }
    }
    ui.spacing();

    if m.settings.friends.is_empty() {
        ui.text_disabled("No friends yet. You'll get a notice when they connect.");
        return;
    }
    let mut remove = None;
    for (i, friend) in m.settings.friends.iter().enumerate() {
        let _id = ui.push_id(i);
        let key = friend_key(friend);
        let status = m.friend_statuses.iter().find(|s| s.key == key);
        let label = if friend.label.is_empty() { key.clone() } else { format!("{} ({key})", friend.label) };
        match status.and_then(|s| s.online_as.as_ref().map(|cs| (cs, &s.detail))) {
            Some((callsign, detail)) => {
                ui.text_colored(theme::OK, "●");
                ui.same_line();
                ui.text(&label);
                ui.same_line();
                ui.text_colored(theme::ACCENT, callsign);
                ui.same_line();
                ui.text_disabled(detail);
            }
            None => {
                ui.text_disabled("○");
                ui.same_line();
                ui.text(&label);
                ui.same_line();
                ui.text_disabled("offline");
            }
        }
        ui.same_line();
        if ui.small_button("Remove") {
            remove = Some(i);
        }
    }
    if let Some(i) = remove {
        m.settings.friends.remove(i);
        m.actions.push(Action::SaveSettings);
    }
}

fn secret_row(ui: &Ui, m: &mut Model, secret: Secret, label: &str, saved: bool, masked: bool) {
    let _id = ui.push_id(label);
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let buffer = match secret {
        Secret::SimbriefUsername => &mut m.ui.simbrief_user_input,
        Secret::HoppieLogon => &mut m.ui.hoppie_code_input,
    };
    ui.set_next_item_width(180.0 * unit);
    let flags = if masked { InputTextFlags::PASSWORD } else { InputTextFlags::NONE };
    let hint = if saved { "Saved — type to replace" } else { "" };
    let entered = ui.input_text(label, buffer).hint(hint).flags(flags).enter_returns_true(true).build();
    ui.same_line();
    if (ui.small_button("Save") || entered) && !buffer.trim().is_empty() {
        let value = std::mem::take(buffer);
        m.actions.push(Action::SaveSecret(secret, Redacted(value)));
    }
    if saved {
        ui.same_line();
        if ui.small_button("Remove") {
            m.actions.push(Action::DeleteSecret(secret));
        }
    }
}

fn settings(ui: &Ui, fonts: Fonts, m: &mut Model) {
    ui.spacing();
    ui.text_disabled("VATSIM");
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    ui.set_next_item_width(120.0 * unit);
    ui.input_text("Your CID", &mut m.ui.cid_input).hint("e.g. 1234567").build();
    if ui.is_item_deactivated_after_edit() {
        let input = m.ui.cid_input.trim();
        m.settings.vatsim.cid = input.parse().ok();
        if m.settings.vatsim.cid.is_none() && !input.is_empty() {
            m.ui.cid_input.clear();
        }
        m.actions.push(Action::SaveSettings);
    }
    if m.cid_detected {
        ui.same_line();
        ui.text_colored(theme::OK, "Found automatically");
    }
    ui.text_disabled("Found automatically from your live callsign or SimBrief plan; you can also type it. No password needed.");
    if ui.checkbox("Notify me when my airports' ATIS or METAR changes", &mut m.settings.vatsim.weather_alerts) {
        m.actions.push(Action::SaveSettings);
    }
    if ui.checkbox("Notify me when a friend connects", &mut m.settings.vatsim.friend_alerts) {
        m.actions.push(Action::SaveSettings);
    }

    ui.spacing();
    ui.text_disabled("SIMBRIEF & HOPPIE (stored in your macOS Keychain)");
    secret_row(ui, m, Secret::SimbriefUsername, "SimBrief username or Pilot ID", m.simbrief_user_saved, false);
    secret_row(ui, m, Secret::HoppieLogon, "Hoppie logon code", m.hoppie_ready, true);
    ui.text_disabled("Get a free Hoppie logon code at hoppie.nl/acars.");

    ui.spacing();
    ui.text_disabled("INTEGRATIONS");
    if ui.checkbox("xPilot companion mode", &mut m.settings.integrations.xpilot_companion) {
        m.actions.push(Action::SaveSettings);
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(
            "Shows xPilot's VATSIM connection in Sidetone and makes Sidetone's push-to-talk key xPilot's too.\n\
             Read-only towards the network: Sidetone never sends anything through xPilot.",
        );
    }
    if m.settings.integrations.xpilot_companion {
        ui.same_line();
        if !m.xpilot.detected() {
            ui.text_colored(theme::WARN, "xPilot plugin not found");
        } else if m.state.connection_via.is_some() && matches!(m.state.connection, sidetone_core::state::Connection::Connected { .. }) {
            ui.text_colored(theme::OK, format!("Connected · {}", m.state.connection.label()));
        } else {
            ui.text_disabled("xPilot found · not connected");
        }
    } else {
        ui.same_line();
        ui.text_disabled("Use alongside xPilot until Sidetone connects natively");
    }

    ui.spacing();
    ui.text_disabled("APPEARANCE");
    let mut scale = m.settings.ui.font_scale * 100.0;
    if ui.slider_f32("UI scale %", &mut scale, 75.0, 150.0) {
        m.settings.ui.font_scale = (scale / 5.0).round() * 5.0 / 100.0;
    }
    if ui.is_item_deactivated_after_edit() {
        m.actions.push(Action::SaveSettings);
    }

    ui.spacing();
    ui.text_disabled("PANEL");
    let mut opacity = m.settings.panel.idle_opacity * 100.0;
    if ui.slider_f32("Idle opacity %", &mut opacity, 20.0, 100.0) {
        m.settings.panel.idle_opacity = opacity / 100.0;
    }
    if ui.is_item_deactivated_after_edit() {
        m.actions.push(Action::SaveSettings);
    }
    if ui.button("Reset panel position") {
        m.actions.push(Action::ResetPanelPosition);
    }
    ui.same_line();
    if ui.button(if m.settings.panel.visible { "Hide panel" } else { "Show panel" }) {
        m.actions.push(Action::TogglePanel);
    }

    ui.spacing();
    ui.text_disabled("PUSH-TO-TALK");
    if ui.checkbox("Use X-Plane's ATC push-to-talk (sim/operation/contact_atc)", &mut m.settings.audio.ptt_uses_xplane_atc_command) {
        m.actions.push(Action::SaveSettings);
    }
    ui.text_disabled("Bind sidetone/ptt in X-Plane's keyboard or joystick settings for a dedicated key.");
    ui.text(if m.state.ptt_pressed { "PTT: transmitting" } else { "PTT: idle" });

    ui.spacing();
    ui.separator();
    let pos = ui.cursor_screen_pos();
    let height = ui.current_font_size();
    let width = {
        let draw = ui.get_window_draw_list();
        theme::wordmark(ui, &draw, fonts, pos, height, theme::TEXT_DIM)
    };
    ui.dummy([width, height]);
    ui.same_line();
    ui.text_disabled(format!("v{}", crate::VERSION));
    let p = m.state.perf;
    ui.text_disabled(format!(
        "Performance: {:.3} ms per frame on the sim thread (panel {:.3}, window {:.3}, logic {:.3} at 20 Hz). Network work runs on background threads.",
        p.panel_ms + p.window_ms + p.loop_ms / 3.0,
        p.panel_ms,
        p.window_ms,
        p.loop_ms
    ));
    ui.text_disabled("Station names & sectors: VATSpy data project (CC BY-SA 4.0). Font: Inter (SIL OFL 1.1).");
}
