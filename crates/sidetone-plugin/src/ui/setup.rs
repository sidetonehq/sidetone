//! The "Get set up" card at the top of the Flight tab. Each step ticks itself off as soon as
//! its value is saved; Hide puts it away (Settings → Show setup checklist brings it back).

use super::{Model, cid_field, secret_field};
use crate::app::Action;
use sidetone_core::setup::{Item, Progress, Step, checklist, ready};
use sidetone_services::keychain::Secret;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;

pub fn build(ui: &Ui, m: &mut Model) {
    if m.settings.setup.dismissed {
        return;
    }
    let items = checklist(&Progress {
        cid: m.settings.vatsim.cid.is_some(),
        ptt_tested: m.settings.setup.ptt_tested,
        simbrief: m.simbrief_user_saved,
        hoppie: m.hoppie_ready,
        xpilot_found: m.xpilot.detected(),
        xpilot_on: m.settings.integrations.xpilot_companion,
    });
    let done = items.iter().filter(|i| i.done).count();
    let status = if ready(&items) { "ready to fly".to_string() } else { format!("{done} of {} done", items.len()) };

    ui.spacing();
    ui.spacing();
    ui.text_disabled(format!("GET SET UP · {status}"));
    let style = ui.clone_style();
    let hide_w = ui.calc_text_size("Hide")[0] + style.frame_padding()[0] * 2.0;
    ui.same_line_with_pos(ui.cursor_pos()[0] + ui.content_region_avail()[0] - hide_w);
    if ui.small_button("Hide") {
        m.settings.setup.dismissed = true;
        m.actions.push(Action::SaveSettings);
    }
    ui.separator();

    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let col = 150.0 * unit;
    for item in items {
        let _id = ui.push_id(item.step as i32);
        row(ui, m, item, col);
    }
}

fn row(ui: &Ui, m: &mut Model, item: Item, col: f32) {
    let label = match item.step {
        Step::Cid => "VATSIM CID",
        Step::PushToTalk => "Push-to-talk",
        Step::Simbrief => "SimBrief",
        Step::Hoppie => "Hoppie CPDLC",
        Step::Xpilot => "xPilot",
    };
    ui.align_text_to_frame_padding();
    if item.done {
        ui.text_colored(theme::OK, "✓");
    } else {
        ui.text_disabled("○");
    }
    ui.same_line();
    ui.text(label);
    ui.same_line_with_pos(col);
    // A group keeps wrapped lines aligned with the column instead of the row start.
    let _group = ui.begin_group();

    match (item.step, item.done) {
        (Step::Cid, true) => {
            let cid = m.settings.vatsim.cid.map(|c| c.to_string()).unwrap_or_default();
            ui.text(cid);
            ui.same_line();
            ui.text_disabled(if m.cid_detected { "found automatically" } else { "change it in Settings" });
        }
        (Step::Cid, false) => {
            cid_field(ui, m, "##setup_cid");
            ui.same_line();
            ui.text_disabled("Found automatically once you connect or import SimBrief");
        }
        (Step::PushToTalk, true) => ui.text_disabled("Working"),
        (Step::PushToTalk, false) if m.state.ptt_pressed => ui.text_colored(theme::OK, "Transmitting…"),
        (Step::PushToTalk, false) => {
            ui.text_wrapped("Bind \"Sidetone: push-to-talk (hold)\" in X-Plane's Keyboard or Joystick settings (X-Plane's Contact ATC key works too), then press it to test.");
        }
        (Step::Simbrief, true) | (Step::Hoppie, true) => ui.text_disabled("Saved in your Keychain"),
        (Step::Simbrief, false) => {
            secret_field(ui, m, Secret::SimbriefUsername, "##setup_simbrief", false, false);
            ui.same_line();
            ui.text_disabled("Optional · imports your plan and airports");
        }
        (Step::Hoppie, false) => {
            secret_field(ui, m, Secret::HoppieLogon, "##setup_hoppie", false, true);
            ui.same_line();
            ui.text_disabled("Optional · free at hoppie.nl/acars");
        }
        (Step::Xpilot, true) => ui.text_disabled("Companion mode on"),
        (Step::Xpilot, false) => {
            if ui.small_button("Turn on companion mode") {
                m.settings.integrations.xpilot_companion = true;
                m.actions.push(Action::SaveSettings);
            }
            ui.same_line();
            ui.text_disabled("Optional · shows xPilot's connection here");
        }
    }
}
