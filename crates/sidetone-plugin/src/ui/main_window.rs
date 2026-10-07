//! The full Sidetone window: a header with Import flight, then Flight, ATC, Messages and
//! Settings tabs.

use super::{Model, Tab};
use crate::app::Action;
use sidetone_services::keychain::Secret;
use sidetone_ui::imgui::{TabBarFittingPolicy, TabBarOptions, TabItemFlags, Ui, WindowFlags};
use sidetone_ui::theme;
use sidetone_ui::{Fonts, HostFrame};

pub fn build(frame: &mut HostFrame, m: &mut Model) {
    let ui = frame.ui;
    m.fonts = Some(frame.fonts);
    // Narrow windows keep just the icons; the tooltips still say what they do.
    let narrow = super::narrow(ui);
    let (dock_icon, dock_word) = if frame.popped_out { (theme::icon::DOCK, "Dock") } else { (theme::icon::EXTERNAL, "Pop out") };
    let label = if narrow { dock_icon.to_string() } else { format!("{dock_icon} {dock_word}") };
    let label = label.as_str();
    let busy = m.simbrief_status.as_deref() == Some("Importing…");
    let import = match (busy, narrow) {
        (true, _) => "Importing…".to_string(),
        (false, true) => theme::icon::DOWNLOAD.to_string(),
        (false, false) => format!("{} Import flight", theme::icon::DOWNLOAD),
    };
    let import = import.as_str();
    let style = ui.clone_style();
    let width = |text: &str| super::pill_width(ui, text);
    // Sidetone draws its own frame, so the header carries the close button (an OS window,
    // when popped out, has its own).
    let close = format!("{}##close", theme::icon::CLOSE);
    let close_w = if frame.popped_out { 0.0 } else { width(&close) + style.item_spacing()[0] };
    let button_x = ui.window_size()[0] - width(label) - style.window_padding()[0] - close_w;
    let import_x = button_x - style.item_spacing()[0] - width(import);
    header(ui, frame.fonts, m, import_x - style.item_spacing()[0] * 2.0);
    ui.same_line();
    ui.set_cursor_pos([import_x, ui.cursor_pos()[1]]);
    {
        let _disabled = ui.begin_disabled_with_cond(busy);
        if super::pill(ui, import) {
            if m.simbrief_user_saved {
                m.actions.push(Action::ImportSimBrief);
            } else {
                m.ui.open_import = true;
            }
        }
    }
    if ui.is_item_hovered() {
        ui.tooltip_text("Import your latest SimBrief flight plan and start a new flight");
    }
    ui.same_line();
    ui.set_cursor_pos([button_x, ui.cursor_pos()[1]]);
    if super::pill(ui, label) {
        frame.set_popped_out(!frame.popped_out);
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(if frame.popped_out { "Dock back into X-Plane" } else { "Pop out into its own window" });
    }
    if !frame.popped_out {
        ui.same_line();
        if super::pill(ui, &close) {
            frame.close();
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Close (the panel opens it again)");
        }
    }
    ui.separator();
    // Everything above the line is the handle for dragging the window around.
    frame.set_drag_height(ui.cursor_pos()[1]);

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
        let chat_label = match m.state.unread {
            0 => "Messages###chat".to_string(),
            n => format!("Messages ({n})###chat"),
        };
        if let Some(_t) = ui.tab_item(chat_label) {
            m.state.unread = 0;
            chat(ui, m);
        }
        if let Some(_t) = ui.tab_item("Settings") {
            settings(ui, frame.fonts, m);
        }
    }
    super::editor::build(ui, m);
    import_popup(ui, m);
}

/// Import flight without a SimBrief Pilot ID saved: ask for it once, then import.
fn import_popup(ui: &Ui, m: &mut Model) {
    const POPUP: &str = "Import flight";
    if std::mem::take(&mut m.ui.open_import) {
        ui.open_popup(POPUP);
    }
    let flags = WindowFlags::ALWAYS_AUTO_RESIZE | WindowFlags::NO_MOVE | WindowFlags::NO_SAVED_SETTINGS;
    let Some(_popup) = ui.begin_modal_popup_config(POPUP).flags(flags).begin() else { return };
    {
        let _wrap = ui.push_text_wrap_pos(ui.current_font_size() * 26.0);
        ui.text("Sidetone imports your latest SimBrief flight plan.");
        ui.text_disabled("Your Pilot ID is on simbrief.com under Account Settings (your username works too). It's kept in your macOS Keychain.");
    }
    ui.spacing();
    super::secret_field(ui, m, Secret::SimbriefUsername, "##import_id", m.simbrief_user_saved, 180.0 * ui.current_font_size() / theme::FONT_SIZE);
    ui.spacing();
    {
        let _disabled = ui.begin_disabled_with_cond(!m.simbrief_user_saved);
        if ui.button("Import") {
            m.actions.push(Action::ImportSimBrief);
            ui.close_current_popup();
        }
    }
    ui.same_line();
    if ui.button("Cancel") {
        ui.close_current_popup();
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
    // Radios and transponder are on the panel; the header just says who and how you're connected.
    let status = format!("{}{via}", m.state.connection.label());
    // Never run under the buttons in narrow windows.
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
                "VATSIM" => theme::facility::CENTER,
                _ => theme::ACCENT,
            };
            ui.text_colored(color, &msg.from);
            ui.same_line();
            ui.text_wrapped(&msg.text);
            if let Some(detail) = &msg.detail
                && ui.is_item_hovered()
            {
                super::wrapped_tooltip(ui, detail);
            }
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

/// Slider values shown as whole percentages ("100%").
fn percent() -> sidetone_ui::imgui::NumericFormat<'static, f32> {
    sidetone_ui::imgui::NumericFormat::new("%.0f%%").expect("a valid printf format")
}

fn settings(ui: &Ui, fonts: Fonts, m: &mut Model) {
    use sidetone_ui::theme::icon;
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let narrow = super::narrow(ui);
    let label_w = 110.0 * unit;
    // One setting per row: label on the left (above the control when narrow), explanations
    // on hover rather than paragraphs. Returns the width the control should take.
    let row = |ui: &Ui, label: &str, help: &str| -> f32 {
        ui.align_text_to_frame_padding();
        ui.text(label);
        if !help.is_empty() && ui.is_item_hovered() {
            super::wrapped_tooltip(ui, help);
        }
        if !narrow {
            ui.same_line_with_pos(label_w);
        }
        ui.content_region_avail()[0].min(220.0 * unit)
    };
    let tip = |ui: &Ui, help: &str| {
        if ui.is_item_hovered() {
            super::wrapped_tooltip(ui, help);
        }
    };
    let save = |m: &mut Model| m.actions.push(Action::SaveSettings);

    super::card(ui, "vatsim", None, || {
        super::card_title(ui, "VATSIM", None);
        let width = row(ui, "CID", "Your VATSIM member number. Found automatically from your live callsign or SimBrief plan, or type it. No password needed.");
        super::cid_field(ui, m, "##cid", width.min(130.0 * unit));
        if m.cid_detected {
            ui.same_line();
            ui.text_colored(theme::OK, icon::CHECK);
            tip(ui, "Found automatically");
        }
        if let (Some(cid), Some((stats_cid, stats))) = (m.settings.vatsim.cid, &m.state.network.stats)
            && *stats_cid == cid
        {
            row(ui, "Hours", "");
            ui.text(format!("Pilot {:.0} h · ATC {:.0} h", stats.pilot, stats.atc));
        }
        if ui.checkbox("ATIS and METAR change notices", &mut m.settings.vatsim.weather_alerts) {
            save(m);
        }
        tip(ui, "A message when the ATIS letter or METAR at your departure or arrival changes.");
    });

    super::card(ui, "simbrief", None, || {
        super::card_title(ui, "SIMBRIEF", None);
        let width = row(ui, "Pilot ID", "On simbrief.com under Account Settings (your username works too). Kept in your macOS Keychain.");
        super::secret_field(ui, m, Secret::SimbriefUsername, "##simbrief_id", m.simbrief_user_saved, width.min(170.0 * unit));
    });

    super::card(ui, "window_and_panel", None, || {
        super::card_title(ui, "WINDOW AND PANEL", None);
        let width = row(ui, "UI scale", "Size of all of Sidetone's text and controls.");
        let mut scale = m.settings.ui.font_scale * 100.0;
        ui.set_next_item_width(width);
        if ui.slider_config("##scale", 75.0, 150.0).display_format(percent()).build(&mut scale) {
            m.settings.ui.font_scale = (scale / 5.0).round() * 5.0 / 100.0;
        }
        if ui.is_item_deactivated_after_edit() {
            save(m);
        }
        let width = row(ui, "Panel opacity", "How see-through the panel is when the mouse isn't over it.");
        let mut opacity = m.settings.panel.idle_opacity * 100.0;
        ui.set_next_item_width(width);
        if ui.slider_config("##opacity", 20.0, 100.0).display_format(percent()).build(&mut opacity) {
            m.settings.panel.idle_opacity = opacity / 100.0;
        }
        if ui.is_item_deactivated_after_edit() {
            save(m);
        }
        if ui.checkbox("Attach this window under the panel", &mut m.settings.main_window.attach_to_panel) {
            save(m);
        }
        tip(ui, "Sits right under the panel at the same width and moves with it, like a sidebar. Drag its bottom edge to change the height.");
        let left = ui.cursor_screen_pos()[0];
        let right = left + ui.content_region_avail()[0];
        if super::pill(ui, "Reset panel position") {
            m.actions.push(Action::ResetPanelPosition);
        }
        let toggle = if m.settings.panel.visible { "Hide panel" } else { "Show panel" };
        super::flow(ui, left, right, super::pill_width(ui, toggle));
        if super::pill(ui, toggle) {
            m.actions.push(Action::TogglePanel);
        }
    });

    super::card(ui, "push_to_talk", None, || {
        super::card_title(ui, "PUSH-TO-TALK", None);
        if ui.checkbox("X-Plane's Contact ATC key", &mut m.settings.audio.ptt_uses_xplane_atc_command) {
            save(m);
        }
        tip(
            ui,
            "Use X-Plane's own Contact ATC key as push-to-talk. For a dedicated key, bind \"Sidetone: push-to-talk (hold)\" in X-Plane's keyboard or joystick settings.",
        );
        row(ui, "Status", "");
        if m.state.ptt_pressed {
            ui.text_colored(theme::OK, "Transmitting");
        } else {
            ui.text_disabled("Idle");
        }
    });

    super::card(ui, "xpilot", None, || {
        super::card_title(ui, "XPILOT", None);
        if ui.checkbox("Companion mode", &mut m.settings.integrations.xpilot_companion) {
            save(m);
        }
        tip(
            ui,
            "Until Sidetone connects natively: shows xPilot's VATSIM connection here and makes Sidetone's push-to-talk key xPilot's too. Read-only towards the network.",
        );
        if m.settings.integrations.xpilot_companion {
            let (text, color) = if !m.xpilot.detected() {
                ("xPilot not found".to_string(), theme::WARN)
            } else if m.state.connection_via.is_some() && matches!(m.state.connection, sidetone_core::state::Connection::Connected { .. }) {
                (format!("Connected · {}", m.state.connection.label()), theme::OK)
            } else {
                ("Not connected".to_string(), theme::TEXT_DIM)
            };
            let left = ui.cursor_screen_pos()[0];
            let right = left + ui.content_region_avail()[0];
            super::flow(ui, left, right, ui.calc_text_size(&text)[0]);
            ui.text_colored(color, text);
        }
    });

    super::card(ui, "about", None, || {
        super::card_title(ui, "ABOUT", None);
        let pos = ui.cursor_screen_pos();
        let height = ui.current_font_size();
        let width = {
            let draw = ui.get_window_draw_list();
            theme::wordmark(ui, &draw, fonts, pos, height, theme::TEXT_DIM)
        };
        ui.dummy([width, height]);
        ui.same_line();
        let p = m.state.perf;
        ui.text_disabled(format!("v{} · {:.2} ms per frame", crate::VERSION, p.panel_ms + p.window_ms + p.loop_ms / 3.0));
        tip(
            ui,
            &format!(
                "Sidetone's cost on X-Plane's sim thread: panel {:.3} ms, window {:.3} ms, logic {:.3} ms at 20 Hz. Network work runs on background threads.",
                p.panel_ms, p.window_ms, p.loop_ms
            ),
        );
        if super::pill(ui, "Show setup checklist") {
            m.settings.setup.dismissed = false;
            m.ui.select_tab = Some(Tab::Flight);
            save(m);
        }
        tip(ui, "Brings back the Get set up card at the top of the Flight tab.");
        {
            let _wrap = ui.push_text_wrap_pos(0.0);
            ui.text_disabled("Station names and sectors: VATSpy data project (CC BY-SA 4.0) · Inter (SIL OFL 1.1) · Lucide icons (ISC)");
        }
    });
}
