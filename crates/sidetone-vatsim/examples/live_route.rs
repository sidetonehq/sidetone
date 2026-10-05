//! Live check of route ATC + events: `cargo run -p sidetone-vatsim --example live_route -- EGLL LEMD`.

use sidetone_vatsim::worker::{Request, RouteQuery, Update, Worker};
use std::sync::mpsc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (dep, arr) = (args.first().cloned().unwrap_or("EGLL".into()), args.get(1).cloned().unwrap_or("LEMD".into()));
    let (tx, rx) = mpsc::channel();
    let worker = Worker::spawn(std::env::temp_dir().join("sidetone-cache"), move |u| {
        let _ = tx.send(u);
    });
    worker.request(Request::SetRoute(RouteQuery { departure: Some(dep.clone()), arrival: Some(arr.clone()), points: vec![] }));
    let mut got = (false, false);
    for u in rx {
        match u {
            Update::Events(e) => {
                println!("{} upcoming events; first: {:?}", e.len(), e.first().map(|e| &e.name));
                got.0 = true;
            }
            Update::RouteAtc(legs) if !legs.is_empty() => {
                for l in legs.iter() {
                    println!("{:<22} {:?}", l.label, l.online.iter().map(|o| format!("{} {}", o.1, o.2)).collect::<Vec<_>>());
                }
                got.1 = true;
            }
            _ => {}
        }
        if got == (true, true) {
            break;
        }
    }
}
