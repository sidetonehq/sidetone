//! The full Sidetone window: Flight, ATC, Clearance, CPDLC, Messages, Events and Settings tabs.

use super::{Model, Tab};
use crate::app::Action;
use sidetone_core::radio::{format_com_khz, format_squawk};
use sidetone_services::keychain::Secret;
use sidetone_ui::imgui::{TabBarFittingPolicy, TabBarOptions, TabItemFlags, Ui};
use sidetone_ui::theme;
use sidetone_ui::{Fonts, HostFrame};

pub fn build(frame: &mut HostFrame, m: &mut Model) {
    let ui = frame.ui;
    let label = if frame.popped_out { "Dock" } else { "Pop out" };
    let style = ui.clone_style();
    let w = ui.calc_text_size(label)[0] + style.frame_padding()[0] * 2.0;
    let button_x = ui.window_size()[0] - w - style.window_padding()[0];
    header(ui, frame.fonts, m, button_x - style.item_spacing()[0] * 2.0);
    ui.same_line();
    ui.set_cursor_pos([button_x, ui.cursor_pos()[1]]);
    if ui.small_button(label) {
        frame.set_popped_out(!frame.popped_out);
    }
    ui.separator();

    // Shrink tab labels rather than hiding tabs behind scroll arrows in narrow windows.
    let tab_options = TabBarOptions::new().fitting_policy(TabBarFittingPolicy::Shrink);
    if let Some(_bar) = ui.tab_bar_with_flags("tabs", tab_options) {
        let select = m.ui.select_tab.take();
        let flags = |tab| if select == Some(tab) { TabItemFlags::SET_SELECTED } else { TabItemFlags::NONE };
        if let Some(_t) = ui.tab_item_with_flags("Flight", None, flags(Tab::Flight)) {
            super::flight::build(ui, m);
        }
        if let Some(_t) = ui.tab_item_with_flags("ATC", None, flags(Tab::Atc)) {
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
        if let Some(_t) = ui.tab_item("Events") {
            super::events::build(ui, m);
        }
        if let Some(_t) = ui.tab_item("Settings") {
            settings(ui, frame.fonts, m);
        }
    }
}

fn header(ui: &Ui, fonts: Fonts, m: &Model, right_limit: f32) {
    let pos = ui.cursor_screen_pos();
    let height = ui.current_font_size() * 1.25;
    let width = {
        let draw = ui.get_window_draw_list();
        theme::wordmark(ui, &draw, fonts, pos, height, theme::TEXT)
    };
    ui.dummy([width, height]);
    ui.same_line();
    ui.align_text_to_frame_padding();
    let connected = matches!(m.state.connection, sidetone_core::state::Connection::Connected { .. });
    let via = m.state.connection_via.filter(|_| connected).map(|v| format!(" via {v}")).unwrap_or_default();
    let traffic = m.state.nearby_aircraft.filter(|n| connected && *n > 0).map(|n| format!("   {n} aircraft")).unwrap_or_default();
    let status = format!(
        "{}{via}   COM1 {}   COM2 {}   XPDR {} {}{traffic}",
        m.state.connection.label(),
        format_com_khz(m.state.com1.active_khz),
        format_com_khz(m.state.com2.active_khz),
        format_squawk(m.state.transponder.code),
        m.state.transponder.mode.label(),
    );
    // Never run under the Pop out button in narrow windows.
    let room = right_limit - ui.cursor_pos()[0];
    ui.text_disabled(super::panel::elide(ui, &status, room));
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

fn settings(ui: &Ui, fonts: Fonts, m: &mut Model) {
    ui.spacing();
    ui.text_disabled("VATSIM");
    super::cid_field(ui, m, "Your CID");
    if m.cid_detected {
        ui.same_line();
        ui.text_colored(theme::OK, "Found automatically");
    }
    ui.text_disabled("Found automatically from your live callsign or SimBrief plan; you can also type it. No password needed.");
    if let (Some(cid), Some((stats_cid, stats))) = (m.settings.vatsim.cid, &m.state.network.stats)
        && *stats_cid == cid
    {
        ui.text(format!("Your hours: pilot {:.0} h, ATC {:.0} h", stats.pilot, stats.atc));
    }
    if ui.checkbox("Notify me when my airports' ATIS or METAR changes", &mut m.settings.vatsim.weather_alerts) {
        m.actions.push(Action::SaveSettings);
    }

    ui.spacing();
    ui.text_disabled("SIMBRIEF & HOPPIE (stored in your macOS Keychain)");
    super::secret_field(ui, m, Secret::SimbriefUsername, "SimBrief Pilot ID", m.simbrief_user_saved, false);
    super::secret_field(ui, m, Secret::HoppieLogon, "Hoppie logon code", m.hoppie_ready, true);
    ui.text_disabled("Pilot ID: simbrief.com → Account Settings (your username works too). Hoppie code: free at hoppie.nl/acars.");

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
    ui.text_disabled("GETTING STARTED");
    if ui.button("Show setup checklist") {
        m.settings.setup.dismissed = false;
        m.ui.select_tab = Some(Tab::Flight);
        m.actions.push(Action::SaveSettings);
    }
    ui.same_line();
    ui.text_disabled("Appears at the top of the Flight tab.");

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
