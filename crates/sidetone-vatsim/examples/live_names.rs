//! Prints live station names: `cargo run -p sidetone-vatsim --example live_names`.

use sidetone_vatsim::worker::{Update, Worker};
use std::sync::mpsc;

fn main() {
    let (tx, rx) = mpsc::channel();
    let cache = std::env::temp_dir().join("sidetone-cache");
    let _worker = Worker::spawn(cache, move |u| {
        let _ = tx.send(u);
    });
    for update in rx {
        if let Update::Snapshot(snap) = update {
            println!("{} stations", snap.stations.len());
            for s in &snap.stations {
                println!("{:<14} {:>8}  {}", s.callsign, sidetone_vatsim::freq::channel_to_hz(s.frequency_khz) as f64 / 1e6, s.name);
            }
            break;
        }
    }
}
