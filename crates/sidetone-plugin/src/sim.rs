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
        }
    }

    /// Copies the current radio and transponder state into `state`.
    pub fn read(&self, state: &mut AppState) {
        let get = |r: &Option<DataRef>| r.map(|r| r.get_i32()).unwrap_or(0);
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
    }

    pub fn squawk(&self, code: i32) {
        if let Some(r) = self.xpdr_code {
            r.set_i32(code);
        }
    }
}
