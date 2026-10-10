//! Blocking HTTP for the worker thread. Never call these from X-Plane's main thread.

use crate::feed::{DataFeed, TransceiverEntry};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

pub const STATUS_URL: &str = "https://status.vatsim.net/status.json";
pub const DATA_URL: &str = "https://data.vatsim.net/v3/vatsim-data.json";
pub const TRANSCEIVERS_URL: &str = "https://data.vatsim.net/v3/transceivers-data.json";
pub const METAR_URL: &str = "https://metar.vatsim.net";
pub const VATSPY_URL: &str = "https://raw.githubusercontent.com/vatsimnetwork/vatspy-data-project/master/VATSpy.dat";
pub const STATS_URL: &str = "https://api.vatsim.net/v2/members";
pub const BOUNDARIES_URL: &str = "https://raw.githubusercontent.com/vatsimnetwork/vatspy-data-project/master/Boundaries.geojson";
pub const EVENTS_URL: &str = "https://my.vatsim.net/api/v2/events/latest";
/// Approach and departure airspace shapes, from the SimAware TRACON Project's latest release.
pub const TRACON_URL: &str = "https://github.com/vatsimnetwork/simaware-tracon-project/releases/latest/download/TRACONBoundaries.geojson";
/// VATGlasses sector data: the list of files, and each file (CC BY-NC-SA 4.0, see `sectors`).
pub const VATGLASSES_TREE_URL: &str = "https://api.github.com/repos/lennycolton/vatglasses-data/git/trees/main?recursive=1";
pub const VATGLASSES_RAW_URL: &str = "https://raw.githubusercontent.com/lennycolton/vatglasses-data/main/";

/// Feeds are a few MB; allow headroom.
const BODY_LIMIT: u64 = 64 * 1024 * 1024;

pub type Result<T> = std::result::Result<T, String>;

pub struct Client {
    agent: ureq::Agent,
    data_url: String,
    transceivers_url: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Status {
    data: StatusData,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StatusData {
    v3: Vec<String>,
    transceivers: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct MemberStats {
    pub id: u32,
    pub pilot: f64,
    pub atc: f64,
}

impl Client {
    pub fn new() -> Client {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .user_agent(concat!("Sidetone/", env!("CARGO_PKG_VERSION"), " (+https://github.com/sidetonehq/sidetone)"))
            .build()
            .into();
        Client { agent, data_url: DATA_URL.into(), transceivers_url: TRANSCEIVERS_URL.into() }
    }

    fn text(&self, url: &str) -> Result<String> {
        let mut response = self.agent.get(url).call().map_err(|e| format!("GET {url}: {e}"))?;
        response.body_mut().with_config().limit(BODY_LIMIT).read_to_string().map_err(|e| format!("GET {url}: {e}"))
    }

    fn json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        let text = self.text(url)?;
        serde_json::from_str(&text).map_err(|e| format!("{url}: {e}"))
    }

    /// Follows VATSIM's bootstrap guidance: read feed URLs from status.json (keeps defaults on failure).
    pub fn bootstrap(&mut self) {
        match self.json::<Status>(STATUS_URL) {
            Ok(status) => {
                if let Some(url) = status.data.v3.first() {
                    self.data_url = url.clone();
                }
                if let Some(url) = status.data.transceivers.first() {
                    self.transceivers_url = url.clone();
                }
            }
            Err(e) => log::warn!("VATSIM status bootstrap failed, using default URLs: {e}"),
        }
    }

    pub fn data_feed(&self) -> Result<DataFeed> {
        self.json(&self.data_url)
    }

    pub fn transceivers(&self) -> Result<Vec<TransceiverEntry>> {
        self.json(&self.transceivers_url)
    }

    /// METARs for the given ICAO codes, keyed by ICAO.
    pub fn metars(&self, icaos: &[String]) -> Result<HashMap<String, String>> {
        if icaos.is_empty() {
            return Ok(HashMap::new());
        }
        let text = self.text(&format!("{METAR_URL}/{}", icaos.join(",")))?;
        Ok(parse_metars(&text))
    }

    pub fn member_stats(&self, cid: u32) -> Result<MemberStats> {
        self.json(&format!("{STATS_URL}/{cid}/stats"))
    }

    pub fn vatspy(&self) -> Result<String> {
        self.text(VATSPY_URL)
    }

    pub fn boundaries(&self) -> Result<String> {
        self.text(BOUNDARIES_URL)
    }

    pub fn tracons(&self) -> Result<String> {
        self.text(TRACON_URL)
    }

    /// The VATGlasses data files ("data/ed.json"), one per line.
    pub fn vatglasses_listing(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Tree {
            tree: Vec<Entry>,
        }
        #[derive(Deserialize)]
        struct Entry {
            path: String,
        }
        let tree: Tree = serde_json::from_str(&self.text(VATGLASSES_TREE_URL)?).map_err(|e| format!("VATGlasses file list: {e}"))?;
        let paths: Vec<String> = tree.tree.into_iter().map(|e| e.path).filter(|p| p.starts_with("data/") && p.ends_with(".json")).collect();
        if paths.is_empty() {
            return Err("VATGlasses file list is empty".into());
        }
        Ok(paths.join("\n"))
    }

    /// One VATGlasses data file, by its path in the listing.
    pub fn vatglasses_file(&self, path: &str) -> Result<String> {
        self.text(&format!("{VATGLASSES_RAW_URL}{path}"))
    }

    pub fn events(&self) -> Result<Vec<crate::events::Event>> {
        crate::events::parse(&self.text(EVENTS_URL)?, crate::time::now_unix())
    }
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

/// One METAR per line, each starting with its ICAO code.
pub fn parse_metars(text: &str) -> HashMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.split_whitespace().next().filter(|id| id.len() == 4).map(|id| (id.to_string(), l.to_string())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar_lines() {
        let m = parse_metars("EGLL 051720Z AUTO 25011KT 9999 NCD 21/14 Q1022\nENBR 051720Z 25009KT 9999 FEW027 11/07 Q1007\n");
        assert_eq!(m.len(), 2);
        assert!(m["ENBR"].contains("Q1007"));
    }
}
