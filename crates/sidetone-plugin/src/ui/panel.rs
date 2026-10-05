//! The minimal top-right panel: status, both radios with station names, and a message ticker.
//! Translucent until hovered; drag anywhere to move, click to open the main window.

use super::Model;
use crate::app::{Action, PANEL_SIZE};
use sidetone_core::radio::{format_com_khz, format_squawk};
use sidetone_core::state::{Connection, CoverageHint, Radio};
use sidetone_ui::HostFrame;
use sidetone_ui::imgui::{DrawListMut, MouseButton, Ui};
use sidetone_ui::theme;

/// How long a new message stays in the ticker, in seconds.
const TICKER_SECONDS: f32 = 8.0;

pub fn build(frame: &mut HostFrame, m: &mut Model) {
    let ui = frame.ui;
    let [w, h] = ui.window_size();
    let origin = ui.window_pos();
    let s = h / PANEL_SIZE.1 as f32; // panel boxels → ImGui units
    let opacity = if frame.hovered { 1.0 } else { m.settings.panel.idle_opacity };
    let fade = |c: [f32; 4]| [c[0], c[1], c[2], c[3] * (0.55 + 0.45 * opacity)];

    let draw = ui.get_window_draw_list();
    let mut bg = theme::SURFACE;
    bg[3] = 0.92 * opacity;
    draw.add_rect(origin, [origin[0] + w, origin[1] + h], bg).filled(true).rounding(12.0 * s).build();
    draw.add_rect(origin, [origin[0] + w, origin[1] + h], [1.0, 1.0, 1.0, 0.07 * opacity]).rounding(12.0 * s).build();

    let pad = 12.0 * s;
    let left = origin[0] + pad;
    let right = origin[0] + w - pad;
    let rows = [origin[1] + 8.0 * s, origin[1] + 32.0 * s, origin[1] + 57.0 * s];

    // Row 1: the Sidetone mark doubles as the connection light, then callsign; XPDR + unread on the right.
    let (light, label) = match &m.state.connection {
        Connection::Connected { .. } => (theme::OK, m.state.connection.label().to_string()),
        Connection::Connecting => (theme::WARN, m.state.connection.label().to_string()),
        Connection::Disconnected => (theme::TEXT_DIM, "Offline".to_string()),
    };
    theme::mark(&draw, [left + 3.0 * s, rows[0] + 9.0 * s], 16.0 * s, fade(light));
    draw.add_text([left + 20.0 * s, rows[0]], fade(theme::TEXT), &label);
    if m.state.network.feed_error.is_some() {
        let x = left + 28.0 * s + ui.calc_text_size(&label)[0];
        draw.add_text([x, rows[0]], fade(theme::WARN), "· VATSIM data unavailable");
    }

    let mut x_right = right;
    if m.state.unread > 0 {
        let badge = m.state.unread.min(99).to_string();
        let bw = ui.calc_text_size(&badge)[0] + 12.0 * s;
        draw.add_rect([x_right - bw, rows[0] - 1.0 * s], [x_right, rows[0] + 17.0 * s], fade(theme::ACCENT)).filled(true).rounding(9.0 * s).build();
        draw.add_text([x_right - bw + 6.0 * s, rows[0]], [0.02, 0.06, 0.06, 1.0], &badge);
        x_right -= bw + 8.0 * s;
    }
    let xpdr = &m.state.transponder;
    let warning = m.state.transponder_warning(&m.settings.flight.squawk);
    let mut chip = format!("XPDR {} {}", format_squawk(xpdr.code), if xpdr.mode.is_mode_c() { "C" } else { xpdr.mode.label() });
    if let Some(w) = &warning {
        chip.push_str(&format!(" · {w}"));
    }
    let chip_w = ui.calc_text_size(&chip)[0] + 12.0 * s;
    let chip_color = match (&warning, xpdr.mode.is_mode_c()) {
        (Some(_), _) => theme::WARN,
        (None, true) => theme::OK,
        (None, false) => theme::TEXT_DIM,
    };
    draw.add_rect([x_right - chip_w, rows[0] - 2.0 * s], [x_right, rows[0] + 18.0 * s], fade([chip_color[0], chip_color[1], chip_color[2], 0.16]))
        .filled(true)
        .rounding(5.0 * s)
        .build();
    draw.add_text([x_right - chip_w + 6.0 * s, rows[0]], fade(chip_color), &chip);

    // Row 2: COM1 | COM2, each with the station you'd hear.
    let half = (right - left) / 2.0;
    radio_cell(ui, &draw, [left, rows[1]], half - 8.0 * s, s, "COM1", &m.state.com1, m.state.ptt_pressed, &fade);
    radio_cell(ui, &draw, [left + half + 8.0 * s, rows[1]], half - 8.0 * s, s, "COM2", &m.state.com2, false, &fade);

    // Row 3, by priority: typing indicator, a fresh message, the frequency hint, idle.
    let ticker = if m.state.keyboard_captured {
        ("Typing in Sidetone · Esc or Enter returns keys to X-Plane".to_string(), theme::ACCENT)
    } else if let Some(msg) = m.state.latest_message(frame.now, TICKER_SECONDS) {
        (format!("{}: {}", msg.from, msg.text), theme::TEXT)
    } else {
        match &m.state.coverage_hint {
            Some(CoverageHint::Tune { name, khz }) => (format!("{name} {} covers you · click to open ATC", format_com_khz(*khz)), theme::ACCENT),
            Some(CoverageHint::Unicom) => ("No ATC overhead · monitor UNICOM 122.800".to_string(), theme::WARN),
            None => ("No new messages".to_string(), theme::TEXT_DIM),
        }
    };
    draw.add_text([left, rows[2]], fade(ticker.1), elide(ui, &ticker.0, right - left));

    // A click anywhere (that wasn't a drag) opens the main window.
    if ui.is_window_hovered() && ui.is_mouse_released(MouseButton::Left) {
        m.actions.push(Action::ToggleMainWindow);
    }
}

#[allow(clippy::too_many_arguments)]
fn radio_cell(
    ui: &Ui,
    draw: &DrawListMut,
    pos: [f32; 2],
    width: f32,
    s: f32,
    name: &str,
    radio: &Radio,
    transmitting: bool,
    fade: &dyn Fn([f32; 4]) -> [f32; 4],
) {
    let [x, y] = pos;
    let name_color = if transmitting {
        theme::DANGER
    } else if radio.powered {
        theme::ACCENT
    } else {
        theme::TEXT_DIM
    };
    draw.add_text([x, y], fade(name_color), name);
    let mut x = x + ui.calc_text_size(name)[0] + 6.0 * s;
    if !radio.powered {
        draw.add_text([x, y], fade(theme::TEXT_DIM), "OFF");
        return;
    }
    let freq = format_com_khz(radio.active_khz);
    draw.add_text([x, y], fade(theme::TEXT), &freq);
    x += ui.calc_text_size("000.000")[0] + 6.0 * s;
    let room = pos[0] + width - x;
    let (label, color) = match &radio.station {
        Some(st) if st.out_of_range => (format!("{} (far)", st.name), theme::TEXT_DIM),
        Some(st) => (st.name.clone(), theme::TEXT),
        None => ("No ATC".to_string(), theme::TEXT_DIM),
    };
    draw.add_text([x, y], fade(color), elide(ui, &label, room));
}

/// Truncates `text` with an ellipsis to fit `width`.
pub fn elide(ui: &Ui, text: &str, width: f32) -> String {
    if ui.calc_text_size(text)[0] <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if ui.calc_text_size(format!("{out}…"))[0] > width {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}
