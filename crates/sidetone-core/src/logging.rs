//! `log` backend: every record goes to `Sidetone.log`; warnings and errors are also
//! forwarded over the bus so the main thread can echo them into X-Plane's Log.txt.

use crate::bus::{BusSender, Event};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

struct Logger {
    file: Mutex<Option<File>>,
    sim: Mutex<Option<BusSender>>,
}

static LOGGER: Logger = Logger { file: Mutex::new(None), sim: Mutex::new(None) };

/// Installs the logger (idempotent — X-Plane can reload plugins in the same process).
pub fn init(log_file: &Path, sim: BusSender) {
    if let Some(dir) = log_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = OpenOptions::new().create(true).write(true).truncate(true).open(log_file).ok();
    *LOGGER.file.lock().unwrap() = file;
    *LOGGER.sim.lock().unwrap() = Some(sim);
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Debug);
}

/// Detaches the bus and closes the file; later log calls become no-ops.
pub fn shutdown() {
    LOGGER.flush();
    *LOGGER.sim.lock().unwrap() = None;
    *LOGGER.file.lock().unwrap() = None;
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let line = format!("{secs:.3} {:<5} [{}] {}", record.level(), record.target(), record.args());
        if let Ok(mut file) = self.file.lock()
            && let Some(file) = file.as_mut()
        {
            let _ = writeln!(file, "{line}");
        }
        if record.level() <= Level::Warn
            && let Ok(sim) = self.sim.lock()
            && let Some(sim) = sim.as_ref()
        {
            sim.send(Event::SimLog(format!("[Sidetone] {} {}", record.level(), record.args())));
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock()
            && let Some(file) = file.as_mut()
        {
            let _ = file.flush();
        }
    }
}
