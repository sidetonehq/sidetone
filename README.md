# Sidetone

[![CI](https://github.com/sidetonehq/sidetone/actions/workflows/ci.yml/badge.svg)](https://github.com/sidetonehq/sidetone/actions/workflows/ci.yml)
[![Licence: Apache-2.0](https://img.shields.io/badge/licence-Apache--2.0-blue.svg)](LICENSE)

A native VATSIM pilot client for X-Plane 12 on macOS: a plugin that lives inside the sim, with a
small panel in the top-left corner and a full window one click away.

> **Status:** pre-release. Everything that uses VATSIM's *public* data works today alongside
> your current pilot client. Connecting to the network as a pilot (FSD + voice) needs VATSIM's
> approval of Sidetone as a client; until then that part is developed against a private test
> server only. See [docs/VATSIM.md](docs/VATSIM.md) and the [roadmap](docs/ROADMAP.md).

> **Coming soon:** Windows support for X-Plane 12, and a version for Microsoft Flight
> Simulator. Sidetone's core, VATSIM data and services code is already simulator- and
> platform-independent, so both build on the same foundation.

## Features

- **Panel**: your callsign, COM1/COM2 with the station you'd hear by its spoken name
  ("Heathrow Tower"), transponder check, messages and a "who should I be talking to" hint.
- **Get set up**: a checklist on first open (CID, push-to-talk test, SimBrief, Hoppie, xPilot)
  that ticks itself off as you go.
- **Flight**: who to call now, ATIS letters and METARs for your airports with change alerts,
  event badges, and **ATC along your route** with one-click tuning.
- **ATC**: every online controller, grouped by type and nearest first, with search and one-click tuning.
- **Clearance & notes**: your SimBrief plan, then squawk, initial altitude, SID, runway,
  departure frequency, QNH, ATIS, stand and notes, auto-filled from PDC/CPDLC clearances,
  SimBrief, VATSIM and METARs. Shows your filed route and warns if it differs from SimBrief.
- **CPDLC & PDC** via Hoppie ACARS: logon, handovers, WILCO/UNABLE/STANDBY, requests.
- **SimBrief** import, **VATSIM events**, pilot hours. Your VATSIM CID is found automatically
  from your live callsign or SimBrief plan.
- **xPilot companion mode** (optional, off by default): until Sidetone connects natively, show
  xPilot's connection in the panel, light up COM receive indicators, and use one push-to-talk
  key for both. Read-only towards the network.
- Keyboard-friendly: Sidetone only holds the keyboard while you type in a field (Cmd+C/V/X/A/Z
  supported) and hands it back on Enter, Escape or when you move away.
- Light on the sim: idle UI rebuilds at 15 Hz with cached geometry, logic at 20 Hz, all network
  work on background threads. Settings shows the measured cost per frame.

## Install

Download the latest zip from [Releases](https://github.com/sidetonehq/sidetone/releases), unzip
it, and copy the `Sidetone` folder into `X-Plane 12/Resources/plugins/`. Builds are not yet
notarized, so macOS may ask you to allow the plugin the first time (System Settings → Privacy &
Security → Allow Anyway).

## Build

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
