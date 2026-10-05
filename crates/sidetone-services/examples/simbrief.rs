//! Fetch a SimBrief plan through the worker: `cargo run -p sidetone-services --example simbrief -- <username or pilot id>`.

use sidetone_services::worker::{Request, Update, Worker};
use std::sync::mpsc;

fn main() {
    let user = std::env::args().nth(1).unwrap_or_default();
    let (tx, rx) = mpsc::channel();
    let worker = Worker::spawn(move |u| {
        let _ = tx.send(u);
    });
    worker.request(Request::FetchSimBrief { username: user });
    for update in rx {
        if let Update::SimBrief(result) = update {
            match result {
                Ok(plan) => println!("OK {} {} -> {} ({} fixes, cruise {})", plan.callsign, plan.origin, plan.destination, plan.fixes.len(), plan.cruise_label()),
                Err(e) => println!("ERR {e}"),
            }
            break;
        }
    }
}
