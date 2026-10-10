//! The aircraft datarefs Sidetone reads and writes.

use sidetone_core::dot_command::Com;
use sidetone_core::radio::TransponderMode;
use sidetone_core::state::AppState;
use sidetone_vatsim::geo::LatLon;
use sidetone_xplm::dataref::DataRef;

pub struct Sim {
    com1_active: Option<DataRef>,
    com1_standby: Option<DataRef>,
    com2_active: Option<DataRef>,
    com2_standby: Option<DataRef>,
    com1_power: Option<DataRef>,
    com2_power: Option<DataRef>,
    avionics: Option<DataRef>,
    xpdr_code: Option<DataRef>,
    xpdr_mode: Option<DataRef>,
    latitude: Option<DataRef>,
    longitude: Option<DataRef>,
    elevation_m: Option<DataRef>,
    on_ground: Option<DataRef>,
    vr: Option<DataRef>,
    audio_com: Option<DataRef>,
    /// A tune to check a few frames on: aircraft with their own radio panel overwrite the
    /// active frequency, so then it's set as standby and swapped (com, kHz, frames left).
    pending_tune: std::cell::Cell<Option<(Com, i32, u8)>>,
}

fn find(name: &str) -> Option<DataRef> {
    let r = DataRef::find(name);
    if r.is_none() {
        log::warn!("Dataref {name} not found");
    }
    r
}

impl Sim {
    pub fn new() -> Sim {
        Sim {
            com1_active: find("sim/cockpit2/radios/actuators/com1_frequency_hz_833"),
            com1_standby: find("sim/cockpit2/radios/actuators/com1_standby_frequency_hz_833"),
            com2_active: find("sim/cockpit2/radios/actuators/com2_frequency_hz_833"),
            com2_standby: find("sim/cockpit2/radios/actuators/com2_standby_frequency_hz_833"),
            com1_power: find("sim/cockpit2/radios/actuators/com1_power"),
            com2_power: find("sim/cockpit2/radios/actuators/com2_power"),
            avionics: find("sim/cockpit2/switches/avionics_power_on"),
            xpdr_code: find("sim/cockpit2/radios/actuators/transponder_code"),
            xpdr_mode: find("sim/cockpit2/radios/actuators/transponder_mode"),
            latitude: find("sim/flightmodel/position/latitude"),
            longitude: find("sim/flightmodel/position/longitude"),
            elevation_m: find("sim/flightmodel/position/elevation"),
            on_ground: find("sim/flightmodel/failures/onground_any"),
            vr: find("sim/graphics/VR/enabled"),
            pending_tune: std::cell::Cell::new(None),
            audio_com: find("sim/cockpit2/radios/actuators/audio_com_selection"),
        }
    }

    /// Copies the current radio and transponder state into `state`.
    pub fn read(&self, state: &mut AppState) {
        let get = |r: &Option<DataRef>| r.map(|r| r.get_i32()).unwrap_or(0);
        if let Some((com, khz, frames)) = self.pending_tune.get() {
            let (active, standby, flip) = match com {
                Com::One => (self.com1_active, self.com1_standby, "sim/radios/com1_standy_flip"),
                Com::Two => (self.com2_active, self.com2_standby, "sim/radios/com2_standy_flip"),
            };
            if get(&active) == khz {
                self.pending_tune.set(None);
            } else if frames > 0 {
                self.pending_tune.set(Some((com, khz, frames - 1)));
            } else {
                // The aircraft put its own frequency back: tune it the way its panel does.
                log::info!("COM{} kept its frequency; tuning {khz} through standby and swap", if com == Com::One { 1 } else { 2 });
                if let Some(r) = standby {
                    r.set_i32(khz);
                }
                if let Some(c) = sidetone_xplm::command::Command::find(flip) {
                    c.once();
                }
                self.pending_tune.set(None);
            }
        }
        let avionics = self.avionics.is_none() || get(&self.avionics) != 0;
        state.com1.active_khz = get(&self.com1_active);
        state.com1.standby_khz = get(&self.com1_standby);
        state.com1.powered = avionics && (self.com1_power.is_none() || get(&self.com1_power) != 0);
        state.com2.active_khz = get(&self.com2_active);
        state.com2.standby_khz = get(&self.com2_standby);
        state.com2.powered = avionics && (self.com2_power.is_none() || get(&self.com2_power) != 0);
        state.transponder.code = get(&self.xpdr_code);
        state.transponder.mode = TransponderMode::from_dataref(get(&self.xpdr_mode));
        if let (Some(lat), Some(lon)) = (self.latitude, self.longitude) {
            state.position = Some(LatLon { lat: lat.get_f64(), lon: lon.get_f64() });
        }
        state.altitude_ft = self.elevation_m.map(|r| r.get_f64() * 3.28084).unwrap_or(0.0);
        state.on_ground = self.on_ground.is_none_or(|r| r.get_i32() != 0);
        // 6 = COM1, 7 = COM2 (X-Plane's audio panel transmit selector).
        state.tx_com = if self.audio_com.is_some_and(|r| r.get_i32() == 7) { 2 } else { 1 };
    }

    pub fn vr_enabled(&self) -> bool {
        self.vr.is_some_and(|r| r.get_i32() != 0)
    }

    pub fn tune(&self, com: Com, khz: i32) {
        let target = match com {
            Com::One => self.com1_active,
            Com::Two => self.com2_active,
        };
        if let Some(r) = target {
            r.set_i32(khz);
        }
        self.pending_tune.set(Some((com, khz, 3)));
    }

    pub fn squawk(&self, code: i32) {
        if let Some(r) = self.xpdr_code {
            r.set_i32(code);
        }
    }
}
