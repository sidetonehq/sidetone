//! Sidetone's screens. Each `build` function draws one window from the shared [`Model`].

pub mod arrival;
pub mod atc;
pub mod clearance;
pub mod editor;
pub mod flight;
pub mod main_window;
pub mod panel;
pub mod setup;

use crate::app::{Action, Redacted};
use sidetone_core::settings::Settings;
use sidetone_core::state::{AppState, Message};
use sidetone_core::watch::Watcher;
use sidetone_services::keychain::Secret;
use sidetone_services::simbrief::Plan;
use sidetone_ui::imgui::Ui;
use sidetone_ui::theme;
use sidetone_vatsim::geo::LatLon;

pub struct Model {
    pub state: AppState,
    pub settings: Settings,
    pub actions: Vec<Action>,
    pub chat_input: String,
    pub watcher: Watcher,
    pub nearest_checked_at: Option<LatLon>,
    pub requested_airports: Vec<String>,
    pub requested_cid: Option<u32>,
    pub ui: UiState,
    pub simbrief: Option<Box<Plan>>,
    /// "Importing…" or the last error.
    pub simbrief_status: Option<String>,
    /// Whether a SimBrief Pilot ID is in the Keychain (the value never sits in the model).
    pub simbrief_user_saved: bool,
    pub requested_route: sidetone_vatsim::worker::RouteQuery,
    /// Settings changed outside an explicit action; saved on the next loop.
    pub settings_dirty: bool,
    /// Cached, filtered and sorted ATC list (rebuilt at most once a second).
    pub atc_rows: Vec<(usize, Option<f64>)>,
    pub atc_rows_key: (usize, String, bool, i64),
    pub was_vr: bool,
    pub xpilot: crate::bridge::xpilot::XpilotBridge,
    /// The CID shown in Settings was found automatically.
    pub cid_detected: bool,
    /// On the ground at the last loop, to spot takeoff (`None` until the first reading).
    pub was_on_ground: Option<bool>,
    /// Where "Attach to panel" last put the main window, to tell a drag of the window (the
    /// panel follows) from a move of the panel (the window follows).
    pub attached_at: Option<sidetone_xplm::window::Rect>,
    /// The main window's fonts, set at the start of each of its frames. (Each window has its
    /// own ImGui context, so these are only valid while the main window is being built.)
    pub fonts: Option<sidetone_ui::Fonts>,
}

/// Transient widget state (search boxes, input fields).
#[derive(Default)]
pub struct UiState {
    pub atc_search: String,
    pub atc_show_all: bool,
    pub cid_input: String,
    /// A tab to bring to the front on the next frame (e.g. "See all" on the Flight tab).
    pub select_tab: Option<Tab>,
    /// Open the Enter clearance or Arrival popup on the next frame.
    pub open_editor: Option<editor::Form>,
    /// ATIS/METAR texts the pilot clicked open on the Flight tab (by key, e.g. "atis:EGKK_ATIS").
    pub expanded: std::collections::HashSet<String>,
    /// Flight tab sections folded shut, by the pilot or by Sidetone (the clearance after takeoff).
    pub collapsed: std::collections::HashSet<Fold>,
    /// Open the SimBrief Pilot ID popup on the next frame (Import flight without an ID saved).
    pub open_import: bool,
    pub simbrief_user_input: String,
}

impl Model {
    pub fn new(settings: Settings) -> Model {
        let cid_input = settings.vatsim.cid.map(|c| c.to_string()).unwrap_or_default();
        Model {
            state: AppState::default(),
            settings,
            actions: Vec::new(),
            chat_input: String::new(),
            watcher: Watcher::default(),
            nearest_checked_at: None,
            requested_airports: Vec::new(),
            requested_cid: None,
            ui: UiState { cid_input, ..Default::default() },
            simbrief: None,
            simbrief_status: None,
            simbrief_user_saved: false,
            requested_route: Default::default(),
            settings_dirty: false,
            atc_rows: Vec::new(),
            atc_rows_key: (0, String::new(), false, 0),
            was_vr: false,
            xpilot: Default::default(),
            cid_detected: false,
            was_on_ground: None,
            fonts: None,
            attached_at: None,
        }
    }

    /// Adds a local notice to the chat (not counted as unread).
    pub fn system_message(&mut self, text: impl Into<String>, now: f32) {
        self.state.messages.push(Message { from: "Sidetone".into(), text: text.into(), detail: None, received_at: now });
    }
}

impl Model {
    /// Push-to-talk was pressed, so it's bound: ticks the setup checklist (saved once).
    pub fn note_ptt_works(&mut self) {
        if self.state.ptt_pressed && !self.settings.setup.ptt_tested {
            self.settings.setup.ptt_tested = true;
            self.settings_dirty = true;
        }
    }

    /// Your flight plan as filed on VATSIM (what ATC sees), if it has a route.
    pub fn filed_plan(&self) -> Option<sidetone_vatsim::feed::FlightPlan> {
        let cid = self.settings.vatsim.cid?;
        let plan = self.state.network.snapshot.as_ref()?.feed.flight_plan_for(cid)?.clone();
        (!plan.route.trim().is_empty()).then_some(plan)
    }

    /// The route to fly and where it came from: as filed on VATSIM, else from SimBrief.
    pub fn route_text(&self) -> Option<(String, &'static str)> {
        if let Some(plan) = self.filed_plan() {
            return Some((plan.route.trim().to_string(), "Filed on VATSIM"));
        }
        let plan = self.simbrief.as_ref().filter(|p| !p.route.trim().is_empty())?;
        Some((plan.route.trim().to_string(), "SimBrief"))
    }

    /// The departure ATIS letter on the network now, and the station broadcasting it.
    pub fn departure_atis(&self) -> Option<(String, String)> {
        let dep = self.state.route.departure.as_deref()?;
        let snapshot = self.state.network.snapshot.as_ref()?;
        let s = sidetone_vatsim::stations::departure_atis(&snapshot.stations, dep)?;
        Some((s.atis_code.clone()?, s.callsign.clone()))
    }

    /// Whether the ATIS letter in your clearance is the departure ATIS's current one: (true,
    /// why) when it is, (false, why) when it's outdated, nothing until both are known.
    pub fn atis_check(&self) -> Option<Check> {
        let (current, _) = self.departure_atis()?;
        let noted = self.settings.flight.atis.trim().to_ascii_uppercase();
        if noted.is_empty() {
            return None;
        }
        let spell = |c: &str| sidetone_core::radio::phonetic(c).map(String::from).unwrap_or_else(|| c.to_string());
        Some(if noted.eq_ignore_ascii_case(&current) {
            (true, format!("Information {} is current", spell(&current)))
        } else {
            (false, format!("Your clearance has information {}; it's now {}", spell(&noted), spell(&current)))
        })
    }

    /// Same as `atis_check`, for the arrival ATIS letter you noted.
    pub fn arrival_atis_check(&self) -> Option<Check> {
        let arr = self.state.route.arrival.as_deref()?;
        let snapshot = self.state.network.snapshot.as_ref()?;
        let current = sidetone_vatsim::stations::arrival_atis(&snapshot.stations, arr)?.atis_code.clone()?;
        let noted = self.settings.flight.arrival.atis.trim().to_ascii_uppercase();
        if noted.is_empty() {
            return None;
        }
        let spell = |c: &str| sidetone_core::radio::phonetic(c).map(String::from).unwrap_or_else(|| c.to_string());
        Some(if noted.eq_ignore_ascii_case(&current) {
            (true, format!("Information {} is current", spell(&current)))
        } else {
            (false, format!("You noted information {}; it's now {}", spell(&noted), spell(&current)))
        })
    }

    /// Whether the transponder shows your assigned squawk, with why.
    pub fn squawk_check(&self) -> Option<Check> {
        let assigned = self.settings.flight.squawk.trim();
        if assigned.is_empty() {
            return None;
        }
        if !sidetone_core::radio::is_valid_squawk(assigned) {
            return Some((false, "A squawk is four digits, 0 to 7".into()));
        }
        let current = sidetone_core::radio::format_squawk(self.state.transponder.code);
        Some(if current == assigned { (true, format!("Transponder set to {assigned}")) } else { (false, format!("Transponder is {current}; set {assigned}")) })
    }

    pub fn recompute_route(&mut self) {
        let feed = self.state.network.snapshot.as_ref().map(|s| &s.feed);
        self.state.route = sidetone_core::watch::route(&self.settings, self.simbrief.as_deref(), feed);
    }
}

/// Main window tabs that other screens can switch to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Flight,
    Atc,
}

/// The Flight tab's collapsible sections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fold {
    Flight,
    Clearance,
    RouteAtc,
    Arrival,
    Notes,
}

/// A section heading: small dim caps with breathing room, used on every tab for consistency.
pub fn section(ui: &sidetone_ui::imgui::Ui, title: &str) {
    ui.spacing();
    ui.spacing();
    ui.text_disabled(title);
    ui.separator();
}

/// A card: one section of a tab in a softly raised, rounded box, apart from its neighbours,
/// like the panel. It grows to fit what `f` draws, or takes `height` when given (to fill the
/// rest of the tab). Returns `f`'s result, or nothing while the card is scrolled out of view.
pub fn card<R>(ui: &Ui, id: &str, height: Option<f32>, f: impl FnOnce() -> R) -> Option<R> {
    use sidetone_ui::imgui::{ChildFlags, StyleVar};
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    // A little room between cards.
    let [x, y] = ui.cursor_pos();
    ui.set_cursor_pos([x, y + CARD_GAP * unit]);
    // Kept pushed while `f` runs, so code inside that reads the window padding gets the card's.
    let padding = ui.clone_style().window_padding();
    let _pad = ui.push_style_var(StyleVar::WindowPadding([padding[0] * 0.85, padding[1] * 0.8]));
    let mut flags = ChildFlags::BORDERS | ChildFlags::ALWAYS_USE_WINDOW_PADDING;
    if height.is_none() {
        flags |= ChildFlags::AUTO_RESIZE_Y;
    }
    ui.child_window(id).size([0.0, height.unwrap_or(0.0)]).child_flags(flags).build(ui, f)
}

/// Extra space between cards, on top of the item spacing.
const CARD_GAP: f32 = 4.0;

/// A card's heading when it doesn't fold: the same dim caps as `fold`, with `action` (a pill)
/// on the right. Returns whether the action was clicked.
pub fn card_title(ui: &Ui, title: &str, action: Option<&str>) -> bool {
    ui.align_text_to_frame_padding();
    let room = ui.content_region_avail()[0] - action.map_or(0.0, |a| pill_width(ui, a) + ui.clone_style().item_spacing()[0]);
    ui.text_disabled(panel::elide(ui, title, room));
    let Some(action) = action else { return false };
    ui.same_line_with_pos(ui.cursor_pos()[0] + ui.content_region_avail()[0] - pill_width(ui, action));
    pill(ui, action)
}

/// Below this width (in UI-scale-independent units) screens switch to their compact layout,
/// e.g. when the window is attached under the panel.
const NARROW_WIDTH: f32 = 600.0;

/// The window is narrow: stack instead of lining things up in columns. Uses the window's
/// display width, so it holds inside popups too.
pub fn narrow(ui: &Ui) -> bool {
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    ui.io().display_size()[0] / unit < NARROW_WIDTH
}

/// A collapsible card heading with small action buttons on the right, each `(label, tooltip)`.
/// Click the title to fold it. Returns whether it's open and which button was clicked.
pub fn fold(ui: &Ui, m: &mut Model, id: Fold, title: &str, actions: &[(&str, &str)]) -> (bool, Option<usize>) {
    use sidetone_ui::imgui::{StyleColor, TreeNodeFlags};
    let was_open = !m.ui.collapsed.contains(&id);
    // The title gives way ("…") to the buttons on its right in narrow windows.
    let style = ui.clone_style();
    // Framed headers reach half the window padding past the content edge; this padding puts the
    // arrow back on the content edge (in line with the dots and labels below) and keeps the
    // title close to it.
    let padding = [(style.window_padding()[0] * 0.5 - 1.0).max(0.0), style.frame_padding()[1]];
    let actions_width: f32 = actions.iter().map(|(label, _)| pill_width(ui, label) + style.item_spacing()[0]).sum();
    let arrow = ui.current_font_size() + padding[0] * 3.0;
    let title = panel::elide(ui, title, ui.content_region_avail()[0] - actions_width - arrow);
    ui.set_next_item_open(was_open);
    let open = {
        // A quiet header: dim caps like every other section, a faint highlight on hover.
        let _bg = ui.push_style_color(StyleColor::Header, [0.0, 0.0, 0.0, 0.0]);
        let _hover = ui.push_style_color(StyleColor::HeaderHovered, [1.0, 1.0, 1.0, 0.04]);
        let _active = ui.push_style_color(StyleColor::HeaderActive, [1.0, 1.0, 1.0, 0.07]);
        let _text = ui.push_style_color(StyleColor::Text, theme::TEXT_DIM);
        let _pad = ui.push_style_var(sidetone_ui::imgui::StyleVar::FramePadding(padding));
        ui.collapsing_header(format!("{title}###{id:?}"), TreeNodeFlags::ALLOW_OVERLAP | TreeNodeFlags::SPAN_AVAIL_WIDTH)
    };
    if open != was_open {
        if open {
            m.ui.collapsed.remove(&id);
        } else {
            m.ui.collapsed.insert(id);
        }
    }
    let mut clicked = None;
    if !actions.is_empty() {
        let width: f32 = actions.iter().map(|(label, _)| pill_width(ui, label)).sum::<f32>() + style.item_spacing()[0] * (actions.len() - 1) as f32;
        // The header ended the line, so the cursor is at the left edge of the content area.
        let right = ui.cursor_pos()[0] + ui.content_region_avail()[0];
        ui.same_line_with_pos(right - width);
        for (i, (label, tooltip)) in actions.iter().enumerate() {
            if i > 0 {
                ui.same_line();
            }
            if pill(ui, label) {
                clicked = Some(i);
            }
            if !tooltip.is_empty() && ui.is_item_hovered() {
                ui.tooltip_text(tooltip);
            }
        }
    }
    (open, clicked)
}

/// Flowing layout, called before each item after the first on a line: keeps the item on this
/// line when `width` fits before `right`, else starts a new line at `left` (screen x). This is
/// what keeps narrow windows tidy instead of running items off the edge.
pub fn flow(ui: &Ui, left: f32, right: f32, width: f32) {
    ui.same_line();
    if ui.cursor_screen_pos()[0] + width > right {
        ui.new_line();
        let y = ui.cursor_screen_pos()[1];
        ui.set_cursor_screen_pos([left, y]);
    }
}

/// Width of the COM1/COM2 pair from `tune_buttons`, for `flow`.
pub fn tune_buttons_width(ui: &Ui) -> f32 {
    pill_width(ui, "COM1") + pill_width(ui, "COM2") + ui.clone_style().item_spacing()[0]
}

/// The one button style: a compact pill a little taller than a text line, so icons fit, and
/// sitting on the text baseline like ImGui's small buttons. Labels must be unique in their
/// window; icon-only ones need a "##name" suffix.
pub fn pill(ui: &Ui, label: &str) -> bool {
    use sidetone_ui::imgui::{StyleVar, sys};
    let padding = pill_padding(ui);
    let _pad = ui.push_style_var(StyleVar::FramePadding(padding));
    let Ok(label) = std::ffi::CString::new(label) else { return false };
    // SAFETY: called while this window's ImGui frame is being built; the label outlives the call.
    unsafe { sys::igButtonEx(label.as_ptr(), sys::ImVec2_c { x: 0.0, y: 0.0 }, sys::ImGuiButtonFlags_AlignTextBaseLine) }
}

/// Width a `pill` with this label takes, for right-aligning rows of them.
pub fn pill_width(ui: &Ui, label: &str) -> f32 {
    let shown = label.split("##").next().unwrap_or(label);
    ui.calc_text_size(shown)[0] + pill_padding(ui)[0] * 2.0
}

fn pill_padding(ui: &Ui) -> [f32; 2] {
    let frame = ui.clone_style().frame_padding();
    [frame[0] * 0.8, frame[1] * 0.5]
}

/// A small button that stays highlighted while `on` (e.g. an ATIS text shown open).
pub fn toggle_button(ui: &Ui, label: &str, on: bool) -> bool {
    use sidetone_ui::imgui::StyleColor;
    let _c = on.then(|| ui.push_style_color(StyleColor::Button, [0.20, 0.78, 0.72, 0.45]));
    pill(ui, label)
}

/// A green tick or amber warning for a checked value, explaining itself on hover.
pub fn status_icon(ui: &Ui, (ok, why): &Check) {
    if *ok {
        ui.text_colored(theme::OK, theme::icon::CHECK);
    } else {
        ui.text_colored(theme::WARN, theme::icon::ALERT);
    }
    if ui.is_item_hovered() {
        wrapped_tooltip(ui, why);
    }
}

/// A checked value: true when it checks out, and why (shown on hover).
pub type Check = (bool, String);

/// A labelled value for `value_tiles`, optionally with a check. An empty value shows "—".
pub type Value<'a> = (&'a str, String, Option<Check>);

/// Label/value tiles in an even grid: a small dim caption above a larger semibold value, so the
/// eye runs down the values. As many columns as fit (four at most, which fit even the narrowest window), in the
/// order given, so a field is always in the same place: empty ones show "—" rather than
/// closing up. A failed check colours the value amber, with the warning after it; a passed one
/// adds a tick. Long values end in "…".
pub fn value_tiles(ui: &Ui, m: &Model, values: &[Value]) {
    let unit = ui.current_font_size() / theme::FONT_SIZE;
    let size = ui.current_font_size();
    let [spacing, spacing_y] = ui.clone_style().item_spacing();
    let left = ui.cursor_pos()[0];
    let avail = ui.content_region_avail()[0];
    let columns = ((avail / (TILE_MIN_WIDTH * unit)).floor() as usize).clamp(1, 4);
    let cell = avail / columns as f32;
    let icon_w = ui.calc_text_size(theme::icon::CHECK)[0] + spacing;
    let fonts = m.fonts;
    for row in values.chunks(columns) {
        let top = ui.cursor_pos()[1];
        let mut bottom = top;
        for (c, (label, value, status)) in row.iter().enumerate() {
            ui.set_cursor_pos([left + c as f32 * cell, top]);
            ui.group(|| {
                caption(ui, m, label);
                let _font = ui.push_font_with_size(fonts.map(|f| f.semibold), size * 1.1);
                if value.is_empty() {
                    ui.text_colored(theme::TEXT_DIM, "—");
                    return;
                }
                let failed = matches!(status, Some((false, _)));
                let room = cell - spacing - status.as_ref().map_or(0.0, |_| icon_w);
                ui.text_colored(if failed { theme::WARN } else { theme::TEXT }, panel::elide(ui, value, room));
                if let Some(status) = status {
                    if ui.is_item_hovered() {
                        wrapped_tooltip(ui, &status.1);
                    }
                    ui.same_line();
                    status_icon(ui, status);
                }
            });
            bottom = bottom.max(ui.cursor_pos()[1]);
        }
        below(ui, left, bottom - spacing_y, TILE_ROW_GAP * unit);
    }
}

/// Continues the layout at (`x`, `y`) after positioned items, leaving `gap` before the next
/// line. It submits an item there: ImGui aborts if a window or card ends right after
/// `set_cursor_pos` without one.
pub fn below(ui: &Ui, x: f32, y: f32, gap: f32) {
    ui.set_cursor_pos([x, y]);
    ui.dummy([0.0, gap]);
}

/// Narrowest a value tile gets before the grid drops a column: four fit the narrowest window
/// (the panel's width), so fields sit in the same places at every width.
const TILE_MIN_WIDTH: f32 = 90.0;

/// A field's name: small dim caps above (or beside) its value, the same everywhere.
pub fn caption(ui: &Ui, m: &Model, text: &str) {
    let _font = ui.push_font_with_size(m.fonts.map(|f| f.regular), ui.current_font_size() * 0.8);
    ui.text_colored(theme::TEXT_DIM, text.to_ascii_uppercase());
}

/// Extra space between rows of value tiles.
const TILE_ROW_GAP: f32 = 4.0;

/// Text in the semibold face, for what the eye looks for: ICAO codes, frequencies, clearance values.
pub fn strong(ui: &Ui, m: &Model, color: [f32; 4], text: &str) {
    let _font = m.fonts.map(|f| ui.push_font(f.semibold));
    ui.text_colored(color, text);
}

/// A tooltip for long text (ATIS, METAR), wrapped to a readable width.
pub fn wrapped_tooltip(ui: &Ui, text: &str) {
    ui.tooltip(|| {
        let _wrap = ui.push_text_wrap_pos(ui.current_font_size() * 32.0);
        ui.text(text);
    });
}

/// "COM1" / "COM2" buttons that tune `khz`; the radio already on it is highlighted.
pub fn tune_buttons(ui: &sidetone_ui::imgui::Ui, m: &mut Model, khz: i32) {
    use sidetone_core::dot_command::Com;
    use sidetone_ui::imgui::StyleColor;
    let _id = ui.push_id(khz as usize);
    for (com, label, active) in [(Com::One, "COM1", m.state.com1.active_khz), (Com::Two, "COM2", m.state.com2.active_khz)] {
        let tuned = active == khz;
        let _c = tuned.then(|| ui.push_style_color(StyleColor::Button, [0.20, 0.78, 0.72, 0.45]));
        if pill(ui, label) && !tuned {
            m.actions.push(Action::Tune { com, khz });
        }
        if com == Com::One {
            ui.same_line();
        }
    }
}

/// The VATSIM CID box; saved when you leave the field.
pub fn cid_field(ui: &Ui, m: &mut Model, label: &str, width: f32) {
    ui.set_next_item_width(width);
    ui.input_text(label, &mut m.ui.cid_input).hint("e.g. 1234567").build();
    if ui.is_item_deactivated_after_edit() {
        let input = m.ui.cid_input.trim();
        m.settings.vatsim.cid = input.parse().ok();
        if m.settings.vatsim.cid.is_none() && !input.is_empty() {
            m.ui.cid_input.clear();
        }
        m.actions.push(Action::SaveSettings);
    }
}

/// A Keychain-backed field: type and Save, or Remove once saved. The value never stays in the model.
/// The buttons follow the field on its line, or wrap under it in a narrow window.
pub fn secret_field(ui: &Ui, m: &mut Model, secret: Secret, label: &str, saved: bool, width: f32) {
    let _id = ui.push_id(label);
    let (buffer, example) = match secret {
        Secret::SimbriefUsername => (&mut m.ui.simbrief_user_input, "e.g. 123456"),
    };
    let left = ui.cursor_screen_pos()[0];
    let right = left + ui.content_region_avail()[0];
    ui.set_next_item_width(width);
    let hint = if saved { "Saved · type to replace" } else { example };
    let entered = ui.input_text(label, buffer).hint(hint).enter_returns_true(true).build();
    flow(ui, left, right, pill_width(ui, "Save"));
    if (pill(ui, "Save") || entered) && !buffer.trim().is_empty() {
        let value = std::mem::take(buffer);
        m.actions.push(Action::SaveSecret(secret, Redacted(value)));
    }
    if saved {
        flow(ui, left, right, pill_width(ui, "Remove"));
        if pill(ui, "Remove") {
            m.actions.push(Action::DeleteSecret(secret));
        }
    }
}
