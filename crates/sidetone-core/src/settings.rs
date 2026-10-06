//! User settings, persisted as TOML in X-Plane's `Output/preferences/Sidetone.toml`.
//! Secrets (passwords, Hoppie logon code) never go here — they live in the macOS Keychain.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub panel: PanelSettings,
    pub main_window: WindowSettings,
    pub audio: AudioSettings,
    pub ui: UiSettings,
    pub vatsim: VatsimSettings,
    /// The current flight's clearance & notes card.
    pub flight: crate::clearance::FlightNotes,
    pub integrations: Integrations,
    pub setup: SetupSettings,
}

/// First-run guidance: the "Get set up" checklist and the panel's welcome line.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetupSettings {
    /// The pilot hid the checklist (Settings can bring it back).
    pub dismissed: bool,
    /// Push-to-talk has been pressed at least once, so it's bound.
    pub ptt_tested: bool,
    /// The main window has been opened, so the panel stops saying "click to get started".
    pub opened_window: bool,
}

/// Optional bridges to other software. All off by default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Integrations {
    /// Show xPilot's connection in Sidetone and forward push-to-talk to it. Read-only towards
    /// the network: Sidetone never sends traffic through xPilot.
    pub xpilot_companion: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VatsimSettings {
    /// Your VATSIM CID: finds your flight plan in the feed and your stats.
    pub cid: Option<u32>,
    /// Manual departure/arrival; empty = use your VATSIM flight plan.
    pub departure: String,
    pub arrival: String,
    /// Post ATIS letter / METAR changes for your airports to the panel.
    pub weather_alerts: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelSettings {
    pub visible: bool,
    /// Background opacity when the mouse is elsewhere (0.0–1.0). Fully opaque on hover.
    pub idle_opacity: f32,
    /// Saved top-left position in global boxels; `None` = default top-left placement.
    pub position: Option<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub position: Option<(i32, i32)>,
    pub size: (i32, i32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Also treat X-Plane's built-in `sim/operation/contact_atc` as push-to-talk.
    pub ptt_uses_xplane_atc_command: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub font_scale: f32,
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings { visible: true, idle_opacity: 0.7, position: None }
    }
}

impl Default for WindowSettings {
    fn default() -> Self {
        WindowSettings { position: None, size: (720, 500) }
    }
}

impl Default for VatsimSettings {
    fn default() -> Self {
        VatsimSettings { cid: None, departure: String::new(), arrival: String::new(), weather_alerts: true }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings { ptt_uses_xplane_atc_command: true }
    }
}

impl Default for UiSettings {
    fn default() -> Self {
        UiSettings { font_scale: 1.0 }
    }
}

impl Settings {
    /// Loads settings, falling back to defaults (and logging why) if the file is missing or invalid.
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str::<Settings>(&text) {
                Ok(settings) => settings.sanitized(),
                Err(e) => {
                    log::warn!("Ignoring invalid settings file {}: {e}", path.display());
                    Settings::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("Could not read settings {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Writes atomically (temp file + rename) so a crash never leaves a half-written file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    fn sanitized(mut self) -> Settings {
        self.panel.idle_opacity = self.panel.idle_opacity.clamp(0.2, 1.0);
        self.ui.font_scale = self.ui.font_scale.clamp(0.75, 2.0);
        self.main_window.size.0 = self.main_window.size.0.max(400);
        self.main_window.size.1 = self.main_window.size.1.max(260);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let dir = std::env::temp_dir().join(format!("sidetone-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Sidetone.toml");
        let mut s = Settings::default();
        s.panel.position = Some((100, 900));
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partial_files_fill_defaults_and_clamp() {
        let s: Settings = toml::from_str("[panel]\nidle_opacity = 5.0\n").unwrap();
        let s = s.sanitized();
        assert_eq!(s.panel.idle_opacity, 1.0);
        assert!(s.panel.visible);
        assert!(s.audio.ptt_uses_xplane_atc_command);
    }

    #[test]
    fn missing_file_gives_defaults() {
        assert_eq!(Settings::load(Path::new("/nonexistent/Sidetone.toml")), Settings::default());
    }
}
