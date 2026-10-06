<h1 align="center">Sidetone</h1>

<p align="center">
  <strong>A native VATSIM pilot client that lives inside X-Plane 12.</strong><br>
  Know who to call, what to say and what you were told, without leaving the cockpit.
</p>

<p align="center">
  <a href="https://github.com/sidetonehq/sidetone/actions/workflows/ci.yml"><img src="https://github.com/sidetonehq/sidetone/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-Apache--2.0-blue.svg" alt="Licence: Apache-2.0"></a>
  <img src="https://img.shields.io/badge/X--Plane-12-0b7285.svg" alt="X-Plane 12">
  <img src="https://img.shields.io/badge/macOS-12%2B%20·%20Apple%20Silicon%20%2B%20Intel-555.svg" alt="macOS 12+, Apple Silicon and Intel">
</p>

<p align="center">
  <img src="docs/images/hero.jpg" alt="Sidetone's panel and main window over an Airbus cockpit at FL270, showing your airports' ATIS and METARs and ATC along the route from Gatwick to Schiphol">
</p>

> **Status: pre-release.** Everything built on VATSIM's *public* data works today, alongside
> your current pilot client (with an optional xPilot companion mode). Connecting to the network
> as a pilot (FSD + voice) needs VATSIM's approval of Sidetone as a client; until then that part
> is developed against a private test server only. See [Sidetone and VATSIM](docs/VATSIM.md)
> and the [roadmap](docs/ROADMAP.md).

## Why Sidetone

- **Lives in the sim.** No second app, no alt-tabbing. A small panel sits in the corner and the
  full window is one click away, docked, popped out or in VR.
- **Talks like ATC.** Frequencies become spoken station names ("Gatwick Ground", "Schiphol
  Approach"), so you know who you'll hear before you tune.
- **Knows your flight.** Import SimBrief and Sidetone follows your airports and route: ATIS,
  METARs, who's staffed along the way, and a ready-to-say clearance request.
- **Costs you no frames.** All network work runs on background threads; the UI redraws from a
  cache when you're not touching it. Settings shows the measured cost per frame.

## A tour

### The panel

<p align="center">
  <img src="docs/images/panel.png" width="560" alt="The Sidetone panel: callsign VIR243 via xPilot, transponder 7014 Mode C, COM1 121.540 Gatwick Ground, COM2 129.955 No ATC">
</p>

Your callsign and connection, both radios with the station you'd actually hear (8.33 kHz aware,
nearest transmitter wins, "No ATC in range" when it's too far), a transponder check against your
assigned squawk, and a message ticker that also tells you who to call next. The mark lights red
while you transmit. Translucent until you hover it; drag it anywhere.

<p align="center">
  <img src="docs/images/in-cockpit.jpg" alt="The Sidetone panel in the top-left corner of an Airbus cockpit, everything else left clear">
</p>

### Flight: who to call, and what's ahead

<img src="docs/images/flight.png" width="480" align="right" alt="The Flight tab: Gatwick and Schiphol with ATIS letters and METARs, and ATC along the route with Gatwick Ground and Schiphol Approach online and three unstaffed areas offering Switch to UNICOM">

- **Your airports**: ATIS letters (split arrival/departure ATIS too), METARs and event badges,
  with a notice when either changes.
- **ATC along your route**: every area you'll cross, who's staffed, one-click tuning, and
  "Switch to UNICOM" where nobody is.
- **Who covers you** right now, with the frequency one click away.
- **Get set up**: on first run, a checklist (CID, push-to-talk test, SimBrief, Hoppie, xPilot)
  that ticks itself off as you go.

<br clear="right">

### ATC: everyone online

<img src="docs/images/atc.png" width="480" align="right" alt="The ATC tab: 75 stations, colour-coded by type, with callsign, frequency, distance and COM1/COM2 tune buttons; Gatwick Ground is highlighted as tuned on COM1">

Every controller within 300 nm (or the whole network), grouped by type and nearest first.
Search by callsign, name or frequency, hover a station for its controller info and where its
spoken name came from, and tune COM1 or COM2 in one click. Hundreds of stations scroll as
smoothly as a screenful.

<br clear="right">

### Clearance and CPDLC

- **Request clearance**: who to call, your stand, and the words to say, e.g. *"Gatwick
  Ground, VIR243, A339, stand 560, information Echo, request clearance to Schiphol."*
- **Your clearance**: squawk, initial altitude, SID, runway, transition level, QNH, ATIS and
  stand, filled in from PDC/CPDLC clearances, SimBrief, VATSIM and METARs, and kept current.
  Anything you type wins.
- **Flight plan**: your SimBrief plan and the route as filed on VATSIM, with a warning if they
  differ.
- **CPDLC & PDC** via Hoppie ACARS: logon, handovers, WILCO/UNABLE/STANDBY and requests.

### And

- **SimBrief import** that starts a new flight, **VATSIM events** at your airports, and your
  pilot and ATC hours.
- Your **VATSIM CID is found automatically** from your live callsign or SimBrief plan.
- **xPilot companion mode** (optional): until Sidetone connects natively, show xPilot's
  connection in the panel, light up receive indicators and share one push-to-talk key.
  Read-only towards the network.
- **Keyboard-friendly**: Sidetone only holds the keyboard while you type in a field (Cmd+C/V/X/A/Z
  supported) and hands it back on Enter, Escape or when you move away, keeping what you typed.

## Install

1. Download the latest zip from [Releases](https://github.com/sidetonehq/sidetone/releases).
2. Unzip it and copy the `Sidetone` folder into `X-Plane 12/Resources/plugins/`.
3. Start X-Plane and click the panel to get started.

Builds are not yet notarized, so macOS may ask you to allow the plugin the first time (System
Settings → Privacy & Security → Allow Anyway). SimBrief and Hoppie are optional; their IDs are
stored in your macOS Keychain.

> **Coming soon:** Windows support for X-Plane 12, and a version for Microsoft Flight
> Simulator. Sidetone's core, VATSIM data and services code is already simulator- and
> platform-independent, so both build on the same foundation.

## Build from source

Requires Rust (stable) and Xcode command-line tools. For a universal (Apple Silicon + Intel)
build, use rustup and `rustup target add aarch64-apple-darwin x86_64-apple-darwin`.

```sh
cargo test --workspace          # unit tests
cargo xtask bundle              # dist/Sidetone/mac_x64/Sidetone.xpl
cargo xtask install             # bundle + copy into ~/X-Plane 12 (or --xplane <path>)
```

Releases are built by CI: pushing a tag like `v0.1.0` publishes a universal zip.

Logs: `X-Plane 12/Output/Sidetone/Sidetone.log`. Settings: `Output/preferences/Sidetone.toml`.
Secrets (Hoppie logon code, SimBrief Pilot ID) live in the macOS Keychain.

## Layout

| Crate | Purpose |
|---|---|
| `sidetone-xplm-sys` / `sidetone-xplm` | X-Plane SDK 4.3 bindings and safe wrappers |
| `sidetone-ui` | Dear ImGui hosted in X-Plane windows (legacy-GL renderer, input, theme) |
| `sidetone-core` | State, settings, alerts, clearance parsing — no X-Plane dependency |
| `sidetone-vatsim` | VATSIM data feed, station naming, coverage, events |
| `sidetone-services` | SimBrief, Hoppie CPDLC, Keychain |
| `sidetone-plugin` | The plugin: wiring and screens |

## Documentation

- [Architecture](docs/ARCHITECTURE.md): crates, threads, rendering, safety rules
- [Sidetone and VATSIM](docs/VATSIM.md): exactly what is fetched, how often, and what is never done
- [Roadmap](docs/ROADMAP.md) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md)

## Licences and credits

Sidetone is Apache-2.0. Station names and sectors use the
[VATSpy data project](https://github.com/vatsimnetwork/vatspy-data-project) (CC BY-SA 4.0),
downloaded at runtime. The Inter font is SIL OFL 1.1. The X-Plane SDK is © Laminar Research
under its own permissive licence.
