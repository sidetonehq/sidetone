//! Sidetone's look: dark glass panels, rounded corners, one teal accent.

use dear_imgui_rs::{Context, DrawListMut, DrawSegmentCount, FontConfig, FontId, FontSource, PolylineFlags, StbTrueTypeFontData, Style, StyleColor, Ui};

pub const ACCENT: [f32; 4] = [0.20, 0.78, 0.72, 1.0];
pub const TEXT: [f32; 4] = [0.93, 0.95, 0.97, 1.0];
pub const TEXT_DIM: [f32; 4] = [0.58, 0.63, 0.69, 1.0];
pub const OK: [f32; 4] = [0.33, 0.85, 0.49, 1.0];
pub const WARN: [f32; 4] = [0.98, 0.74, 0.27, 1.0];
pub const DANGER: [f32; 4] = [0.96, 0.36, 0.36, 1.0];
pub const SURFACE: [f32; 4] = [0.07, 0.09, 0.11, 1.0];
pub const SURFACE_RAISED: [f32; 4] = [0.12, 0.15, 0.18, 1.0];
/// A card (one section of a tab), a step above the window background.
pub const SURFACE_CARD: [f32; 4] = [0.095, 0.115, 0.14, 1.0];
/// Buttons and fields: a step above a card, so they read on it.
const CONTROL: [f32; 4] = [0.15, 0.185, 0.22, 1.0];

/// Station types, kept apart from the status colours above so amber and red only ever mean
/// "act on this". Soft and distinct, in contact order from the outside in.
pub mod facility {
    pub const CENTER: [f32; 4] = [0.49, 0.62, 0.96, 1.0];
    pub const APPROACH: [f32; 4] = [0.70, 0.56, 0.95, 1.0];
    pub const TOWER: [f32; 4] = [0.94, 0.56, 0.47, 1.0];
    pub const GROUND: [f32; 4] = [0.84, 0.74, 0.55, 1.0];
    pub const ATIS: [f32; 4] = [0.45, 0.80, 0.80, 1.0];
}

/// Lucide icons (ISC licence, assets/fonts/Lucide-LICENSE.txt), merged into both fonts. Only
/// these glyphs are bundled (Lucide-Subset.ttf); add a codepoint there before using a new one.
pub mod icon {
    pub const COPY: &str = "\u{e09e}";
    pub const DOWNLOAD: &str = "\u{e0b2}";
    pub const EXTERNAL: &str = "\u{e0b9}";
    pub const DOCK: &str = "\u{e11b}";
    pub const EDIT: &str = "\u{e1f9}";
    pub const LIST: &str = "\u{e106}";
    pub const ALERT: &str = "\u{e193}";
    pub const CHECK: &str = "\u{e226}";
    pub const CLOSE: &str = "\u{e1b2}";
}

/// Base font size in boxels before Retina scaling.
pub const FONT_SIZE: f32 = 14.0;

/// Icon size, relative to the text it's merged into (scales with it). Lucide glyphs fill
/// nearly their whole box while Inter's capitals are 60% of its, so this draws an icon about
/// 1.15× the capital height.
const ICON_SIZE: f32 = 10.5;

/// Inter's capital height as a share of its pixel size (cap height 1490 over ascent + descent
/// 2478 font units), for centring icons on capitals.
const INTER_CAP_SHARE: f32 = 1490.0 / 2478.0;

/// How far down to move icons so their centre sits on the capitals' centre. Both glyph boxes hang
/// from the same baseline: Lucide's icons are centred in a box ICON_SIZE tall, Inter's capitals
/// fill FONT_SIZE × INTER_CAP_SHARE above the baseline, so the gap is half the difference.
const ICON_OFFSET_Y: f32 = (ICON_SIZE - FONT_SIZE * INTER_CAP_SHARE) / 2.0;

fn with_alpha(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

/// Applies the theme on top of `pristine` (the context's original style), scaled by `scale`
/// ImGui units per boxel. Starting from the pristine copy keeps repeated rescaling exact.
pub fn apply(context: &mut Context, pristine: &Style, scale: f32) {
    let style = context.style_mut();
    *style = pristine.clone();
    style.set_window_rounding(10.0);
    style.set_child_rounding(8.0);
    style.set_frame_rounding(6.0);
    style.set_popup_rounding(8.0);
    style.set_grab_rounding(6.0);
    style.set_tab_rounding(6.0);
    style.set_scrollbar_rounding(6.0);
    style.set_window_border_size(0.0);
    style.set_frame_border_size(0.0);
    style.set_window_padding([12.0, 10.0]);
    style.set_frame_padding([10.0, 5.0]);
    style.set_item_spacing([8.0, 6.0]);
    style.set_scrollbar_size(10.0);

    let colors = [
        (StyleColor::Text, TEXT),
        (StyleColor::TextDisabled, TEXT_DIM),
        (StyleColor::WindowBg, SURFACE),
        (StyleColor::ChildBg, SURFACE_CARD),
        (StyleColor::PopupBg, SURFACE_RAISED),
        (StyleColor::Border, [1.0, 1.0, 1.0, 0.08]),
        (StyleColor::FrameBg, CONTROL),
        (StyleColor::FrameBgHovered, [0.18, 0.22, 0.26, 1.0]),
        (StyleColor::FrameBgActive, [0.21, 0.26, 0.31, 1.0]),
        (StyleColor::Button, CONTROL),
        (StyleColor::ButtonHovered, with_alpha(ACCENT, 0.35)),
        (StyleColor::ButtonActive, with_alpha(ACCENT, 0.55)),
        (StyleColor::Header, with_alpha(ACCENT, 0.18)),
        (StyleColor::HeaderHovered, with_alpha(ACCENT, 0.30)),
        (StyleColor::HeaderActive, with_alpha(ACCENT, 0.45)),
        (StyleColor::Tab, SURFACE),
        (StyleColor::TabHovered, with_alpha(ACCENT, 0.30)),
        (StyleColor::TabSelected, SURFACE_RAISED),
        (StyleColor::TabSelectedOverline, ACCENT),
        (StyleColor::Separator, [1.0, 1.0, 1.0, 0.08]),
        (StyleColor::ScrollbarBg, [0.0, 0.0, 0.0, 0.0]),
        (StyleColor::ScrollbarGrab, [1.0, 1.0, 1.0, 0.12]),
        (StyleColor::ScrollbarGrabHovered, [1.0, 1.0, 1.0, 0.2]),
        (StyleColor::CheckMark, ACCENT),
        (StyleColor::SliderGrab, ACCENT),
        (StyleColor::TextSelectedBg, with_alpha(ACCENT, 0.35)),
        (StyleColor::InputTextCursor, ACCENT),
    ];
    for (slot, value) in colors {
        style.set_color(slot, value);
    }
    style.scale_all_sizes(scale);
    style.set_font_size_base(FONT_SIZE * scale);
}

/// Inter (SIL Open Font License 1.1, see assets/fonts/Inter-LICENSE.txt), embedded in the plugin.
const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/fonts/Lucide-Subset.ttf");

/// Fonts available to UI code. The first one added is ImGui's default.
#[derive(Clone, Copy, Debug)]
pub struct Fonts {
    pub regular: FontId,
    pub semibold: FontId,
}

fn add(context: &Context, bytes: &[u8]) -> FontId {
    let atlas = context.font_atlas();
    match StbTrueTypeFontData::from_slice(bytes) {
        Ok(data) => {
            // Inter keeps alternate glyphs in the Private Use Area (U+E09E is a case minus), which
            // would win over the merged icons; leave that range to Lucide.
            let text = FontSource::stb_truetype_with_size(data, FONT_SIZE).with_config(FontConfig::new().glyph_exclude_ranges(&[(0xE000, 0xF8FF)]));
            match StbTrueTypeFontData::from_slice(LUCIDE) {
                // Icons a little taller than the capitals and centred on them, with a fixed
                // width so they line up.
                Ok(icons) => {
                    let config = FontConfig::new().merge_mode(true).glyph_offset([0.0, ICON_OFFSET_Y]).glyph_min_advance_x(ICON_SIZE + 1.0);
                    atlas.add_font(&[text, FontSource::stb_truetype_with_size(icons, ICON_SIZE).with_config(config)])
                }
                Err(e) => {
                    log::warn!("Bundled icon font rejected ({e}); icons won't show");
                    atlas.add_font(&[text])
                }
            }
        }
        Err(e) => {
            log::warn!("Bundled font rejected ({e}); using ImGui default");
            atlas.add_font(&[FontSource::default_font_with_size(FONT_SIZE)])
        }
    }
}

/// Loads the bundled Inter fonts, each with the Lucide icons merged in.
pub fn load_fonts(context: &Context) -> Fonts {
    let regular = add(context, INTER_REGULAR);
    let semibold = add(context, INTER_SEMIBOLD);
    Fonts { regular, semibold }
}

/// The Sidetone mark: a transmitting dot with two radio-wave arcs, centred on the dot.
pub fn mark(draw: &DrawListMut, center: [f32; 2], size: f32, color: [f32; 4]) {
    let dot = size * 0.16;
    draw.add_circle(center, dot, color).filled(true).build();
    let span = std::f32::consts::FRAC_PI_4;
    for (i, alpha) in [(1.0f32, 0.85f32), (2.0, 0.45)] {
        let r = dot + size * 0.2 * i;
        draw.path_arc_to(center, r, -span, span, DrawSegmentCount::count(12));
        draw.path_stroke([color[0], color[1], color[2], color[3] * alpha], PolylineFlags::NONE, (size * 0.07).max(1.0));
    }
}

/// The wordmark: lowercase "sidetone" in Inter SemiBold, with the i's dot replaced by the mark.
/// `height` is the font size; returns the width drawn.
pub fn wordmark(ui: &Ui, draw: &DrawListMut, fonts: Fonts, pos: [f32; 2], height: f32, text_color: [f32; 4]) -> f32 {
    const WORD: &str = "s\u{131}detone"; // dotless i
    let _font = ui.push_font_with_size(Some(fonts.semibold), height);
    let s_w = ui.calc_text_size("s")[0];
    let i_w = ui.calc_text_size("\u{131}")[0];
    draw.add_text(pos, text_color, WORD);
    let center = [pos[0] + s_w + i_w * 0.5, pos[1] + height * 0.2];
    mark(draw, center, height * 0.9, ACCENT);
    ui.calc_text_size(WORD)[0] + height * 0.25
}
