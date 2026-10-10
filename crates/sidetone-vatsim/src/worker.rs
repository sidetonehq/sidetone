//! Background thread that keeps VATSIM data fresh and reports [`Update`]s.
//!
//! Schedule: data + transceivers every 15 s (the feed's own refresh rate), METARs every 5 min
//! or when the watched airports change, member stats on request and hourly, VATSpy weekly (cached).

use crate::boundaries::Boundaries;
use crate::coverage::{self, RouteLeg};
use crate::events::Event;
use crate::feed::DataFeed;
use crate::geo::LatLon;
use crate::http::{Client, MemberStats};
use crate::sectors::{self, Sectors};
use crate::stations::{self, Station};
use crate::vatspy::VatSpy;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

const FEED_INTERVAL: Duration = Duration::from_secs(15);
const FEED_RETRY: Duration = Duration::from_secs(30);
const METAR_INTERVAL: Duration = Duration::from_secs(300);
const STATS_INTERVAL: Duration = Duration::from_secs(3600);
const VATSPY_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 3600);
const EVENTS_INTERVAL: Duration = Duration::from_secs(3600);

/// One consistent view of the network.
#[derive(Debug)]
pub struct Snapshot {
    pub feed: DataFeed,
    pub stations: Vec<Station>,
    pub received: SystemTime,
}

#[derive(Clone, Debug)]
pub enum Update {
    Snapshot(Arc<Snapshot>),
    VatSpy(Arc<VatSpy>),
    Metars(HashMap<String, String>),
    Stats(u32, MemberStats),
    Boundaries(Arc<Boundaries>),
    Events(Arc<Vec<Event>>),
    /// Airspaces along the current route, refreshed with every snapshot.
    RouteAtc(Arc<Vec<RouteLeg>>),
    /// The feed could not be fetched; `None` when it recovers.
    FeedError(Option<String>),
    /// Approach and departure airspace shapes (SimAware TRACON Project).
    Tracons(Arc<crate::tracon::Tracons>),
    /// Sector floors, ceilings and owners for the areas you're in or flying through (VATGlasses).
    Sectors(Arc<Sectors>),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RouteQuery {
    pub departure: Option<String>,
    pub arrival: Option<String>,
    /// Route geometry; when empty, a straight line between the airports is used.
    pub points: Vec<LatLon>,
    /// Planned cruise altitude, for who owns the airspace at each point of the route.
    pub cruise_ft: Option<u32>,
}

#[derive(Debug)]
pub enum Request {
    /// Airports whose METARs to keep fresh (replaces the previous list).
    WatchAirports(Vec<String>),
    /// Member whose stats to fetch.
    SetCid(Option<u32>),
    /// Route for the "ATC along my route" list.
    SetRoute(RouteQuery),
}

enum Message {
    Request(Request),
    Stop,
}

pub struct Worker {
    tx: Sender<Message>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the worker. `on_update` runs on the worker thread and must not touch X-Plane.
    pub fn spawn(cache_dir: PathBuf, on_update: impl Fn(Update) + Send + 'static) -> Worker {
        let (tx, rx) = unbounded();
        let handle = std::thread::Builder::new().name("sidetone-vatsim".into()).spawn(move || run(rx, cache_dir, on_update)).expect("spawn VATSIM worker");
        Worker { tx, handle: Some(handle) }
    }

    pub fn request(&self, request: Request) {
        let _ = self.tx.send(Message::Request(request));
    }
}

impl Drop for Worker {
    /// Joins the thread: the plugin's code must not be unloaded while it still runs.
    /// Worst case this waits for one in-flight request's 10 s timeout.
    fn drop(&mut self) {
        let _ = self.tx.send(Message::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct State {
    client: Client,
    cache_dir: PathBuf,
    vatspy: Option<Arc<VatSpy>>,
    boundaries: Option<Arc<Boundaries>>,
    tracons: Option<Arc<crate::tracon::Tracons>>,
    /// VATGlasses: the list of its files, and those loaded so far.
    sectors: Sectors,
    sector_listing: Option<Vec<String>>,
    sector_files: std::collections::HashSet<String>,
    last_snapshot: Option<Arc<Snapshot>>,
    route: RouteQuery,
    airports: Vec<String>,
    cid: Option<u32>,
    last_feed_stamp: String,
    feed_failing: bool,
    next_feed: Instant,
    next_metar: Instant,
    next_stats: Instant,
    next_events: Instant,
}

fn run(rx: Receiver<Message>, cache_dir: PathBuf, on_update: impl Fn(Update)) {
    let now = Instant::now();
    let mut s = State {
        client: Client::new(),
        cache_dir,
        vatspy: None,
        boundaries: None,
        tracons: None,
        sectors: Sectors::default(),
        sector_listing: None,
        sector_files: Default::default(),
        last_snapshot: None,
        route: RouteQuery::default(),
        airports: Vec::new(),
        cid: None,
        last_feed_stamp: String::new(),
        feed_failing: false,
        next_feed: now,
        next_metar: now + METAR_INTERVAL,
        next_stats: now + STATS_INTERVAL,
        next_events: now,
    };
    s.client.bootstrap();
    if let Some(v) = load_vatspy(&s.client, &s.cache_dir) {
        let v = Arc::new(v);
        s.vatspy = Some(v.clone());
        on_update(Update::VatSpy(v));
    }
    if let Some(b) = load_boundaries(&s.client, &s.cache_dir) {
        let b = Arc::new(b);
        s.boundaries = Some(b.clone());
        on_update(Update::Boundaries(b));
    }
    if let Some(t) = load_tracons(&s.client, &s.cache_dir) {
        log::info!("{} approach and departure airspaces (SimAware TRACON Project)", t.items.len());
        let t = Arc::new(t);
        s.tracons = Some(t.clone());
        on_update(Update::Tracons(t));
    }

    loop {
        let now = Instant::now();
        if now >= s.next_feed {
            poll_feed(&mut s, &on_update);
        }
        if now >= s.next_metar {
            poll_metars(&mut s, &on_update);
        }
        if now >= s.next_stats {
            poll_stats(&mut s, &on_update);
        }
        if now >= s.next_events {
            s.next_events = now + EVENTS_INTERVAL;
            match s.client.events() {
                Ok(events) => on_update(Update::Events(Arc::new(events))),
                Err(e) => log::warn!("Events unavailable: {e}"),
            }
        }
        let wake = s.next_feed.min(s.next_metar).min(s.next_stats).min(s.next_events);
        match rx.recv_timeout(wake.saturating_duration_since(Instant::now())) {
            Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Message::Request(Request::WatchAirports(mut list))) => {
                list.sort();
                list.dedup();
                if list != s.airports {
                    s.airports = list;
                    s.next_metar = Instant::now();
                    load_sectors(&mut s, &on_update);
                }
            }
            Ok(Message::Request(Request::SetRoute(route))) => {
                if route != s.route {
                    s.route = route;
                    if !load_sectors(&mut s, &on_update) {
                        publish_route_atc(&s, &on_update);
                    }
                }
            }
            Ok(Message::Request(Request::SetCid(cid))) => {
                if cid != s.cid {
                    s.cid = cid;
                    s.next_stats = Instant::now();
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

fn poll_feed(s: &mut State, on_update: &impl Fn(Update)) {
    let result = s.client.data_feed().and_then(|feed| Ok((feed, s.client.transceivers()?)));
    match result {
        Ok((feed, transceivers)) => {
            s.next_feed = Instant::now() + FEED_INTERVAL;
            if s.feed_failing {
                s.feed_failing = false;
                on_update(Update::FeedError(None));
            }
            if feed.general.update_timestamp == s.last_feed_stamp {
                return;
            }
            s.last_feed_stamp = feed.general.update_timestamp.clone();
            let stations = stations::build(&feed, &transceivers, s.vatspy.as_deref());
            let snapshot = Arc::new(Snapshot { feed, stations, received: SystemTime::now() });
            s.last_snapshot = Some(snapshot.clone());
            on_update(Update::Snapshot(snapshot));
            publish_route_atc(s, on_update);
        }
        Err(e) => {
            s.next_feed = Instant::now() + FEED_RETRY;
            if !s.feed_failing {
                log::warn!("VATSIM data feed unavailable: {e}");
                s.feed_failing = true;
                on_update(Update::FeedError(Some(e)));
            }
        }
    }
}

/// Recomputes the along-route ATC list (worker thread, so it never costs a frame).
fn publish_route_atc(s: &State, on_update: &impl Fn(Update)) {
    let (Some(snapshot), Some(vatspy), Some(boundaries)) = (&s.last_snapshot, &s.vatspy, &s.boundaries) else { return };
    if s.route.departure.is_none() && s.route.arrival.is_none() && s.route.points.is_empty() {
        on_update(Update::RouteAtc(Arc::new(Vec::new())));
        return;
    }
    let mut points = s.route.points.clone();
    if points.is_empty() {
        let pos = |id: &Option<String>| id.as_deref().and_then(|i| vatspy.airport(i)).map(|a| a.position);
        points.extend(pos(&s.route.departure));
        points.extend(pos(&s.route.arrival));
    }
    let sectors = (!s.sectors.is_empty()).then_some(&s.sectors);
    let profile = coverage::Profile { tracons: s.tracons.as_deref(), sectors, cruise_ft: s.route.cruise_ft };
    let legs = coverage::along_route(s.route.departure.as_deref(), s.route.arrival.as_deref(), &points, &snapshot.stations, vatspy, boundaries, profile);
    on_update(Update::RouteAtc(Arc::new(legs)));
}

fn poll_metars(s: &mut State, on_update: &impl Fn(Update)) {
    s.next_metar = Instant::now() + METAR_INTERVAL;
    match s.client.metars(&s.airports) {
        Ok(m) => on_update(Update::Metars(m)),
        Err(e) => log::warn!("METAR fetch failed: {e}"),
    }
}

fn poll_stats(s: &mut State, on_update: &impl Fn(Update)) {
    s.next_stats = Instant::now() + STATS_INTERVAL;
    let Some(cid) = s.cid else { return };
    match s.client.member_stats(cid) {
        Ok(stats) => on_update(Update::Stats(cid, stats)),
        Err(e) => log::warn!("Stats for {cid} unavailable: {e}"),
    }
}

/// Returns a cached file's text, downloading it when missing or older than a week.
fn cached(name: &str, cache_dir: &std::path::Path, download: impl FnOnce() -> crate::http::Result<String>) -> Option<String> {
    let path = cache_dir.join(name);
    let fresh = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age < VATSPY_MAX_AGE);
    if !fresh {
        match download() {
            Ok(text) => {
                let _ = std::fs::create_dir_all(cache_dir);
                if let Err(e) = std::fs::write(&path, &text) {
                    log::warn!("Could not cache {name}: {e}");
                }
                return Some(text);
            }
            Err(e) => log::warn!("{name} download failed (using cache if present): {e}"),
        }
    }
    std::fs::read_to_string(&path).ok()
}

/// Loads the VATGlasses files for the airports watched and the FIRs along the route (cached a
/// week). Sends the sectors and refreshes the route's ATC when any were added; false if none.
fn load_sectors(s: &mut State, on_update: &impl Fn(Update)) -> bool {
    let mut icaos: Vec<String> = s.airports.clone();
    icaos.extend(s.route.departure.iter().chain(&s.route.arrival).cloned());
    if let Some(b) = &s.boundaries {
        // The route's points, or without them a straight line between the airports.
        let mut points = s.route.points.clone();
        let end = |id: &Option<String>| id.as_deref().and_then(|i| s.vatspy.as_ref()?.airport(i)).map(|a| a.position);
        if points.is_empty()
            && let (Some(a), Some(z)) = (end(&s.route.departure), end(&s.route.arrival))
        {
            points = (0..=20).map(|i| f64::from(i) / 20.0).map(|t| LatLon { lat: a.lat + (z.lat - a.lat) * t, lon: a.lon + (z.lon - a.lon) * t }).collect();
        }
        for p in &points {
            icaos.extend(b.containing(*p).map(|b| b.id.split('-').next().unwrap_or(&b.id).to_string()));
        }
    }
    icaos.sort();
    icaos.dedup();
    if icaos.is_empty() {
        return false;
    }
    if s.sector_listing.is_none() {
        let client = &s.client;
        s.sector_listing = cached("vatglasses-files.txt", &s.cache_dir, || client.vatglasses_listing()).map(|t| t.lines().map(String::from).collect());
    }
    let Some(listing) = &s.sector_listing else { return false };
    let mut added = 0;
    for path in sectors::files_for(listing, &icaos) {
        if s.sector_files.contains(&path) {
            continue;
        }
        // Tried once per run, loaded or not: no hammering GitHub when it fails.
        s.sector_files.insert(path.clone());
        let client = &s.client;
        let Some(text) = cached(&format!("vatglasses-{}.json", sectors::file_name(&path).replace('/', "_")), &s.cache_dir, || client.vatglasses_file(&path))
        else {
            continue;
        };
        match s.sectors.add(&sectors::file_name(&path), &text) {
            Ok(()) => added += 1,
            Err(e) => log::warn!("{e}"),
        }
    }
    if added == 0 {
        return false;
    }
    log::info!("Sector levels and owners for {} more area(s) (VATGlasses)", added);
    on_update(Update::Sectors(Arc::new(s.sectors.clone())));
    publish_route_atc(s, on_update);
    true
}

fn load_vatspy(client: &Client, cache_dir: &std::path::Path) -> Option<VatSpy> {
    cached("VATSpy.dat", cache_dir, || client.vatspy()).map(|t| VatSpy::parse(&t))
}

fn load_tracons(client: &Client, cache_dir: &std::path::Path) -> Option<crate::tracon::Tracons> {
    let text = cached("TRACONBoundaries.geojson", cache_dir, || client.tracons())?;
    crate::tracon::Tracons::parse(&text).map_err(|e| log::warn!("{e}")).ok()
}

fn load_boundaries(client: &Client, cache_dir: &std::path::Path) -> Option<Boundaries> {
    let text = cached("Boundaries.geojson", cache_dir, || client.boundaries())?;
    Boundaries::parse(&text).map_err(|e| log::warn!("{e}")).ok()
}
