# Roadmap

## Done

- **Foundation:** Rust plugin for X-Plane 12 (SDK 4.3), Dear ImGui UI inside the sim, minimal
  top-left panel, main window, pop-out and VR support, crash containment, logging.
- **Public VATSIM data:** spoken station names for tuned frequencies, online ATC list with
  one-click tuning, ATIS/METAR watch with alerts, ATC along the route, coverage hint, events,
  pilot hours.
- **Planning:** SimBrief import, a one-page flight summary, who to call for your clearance, and
  clearance entry with auto-fill.
- **Polish:** keyboard focus hand-back and clipboard, UI scale, frame caching.

## Next

- **Native network client** (pending VATSIM approval): login, position reporting, text and
  private messages, flight plan filing, transponder, other aircraft via
  [XPMP2](https://github.com/TwinFan/XPMP2) with model matching, TCAS. Developed against a
  local test server until approved.
- **Voice:** Audio for VATSIM via AFV-Native, push-to-talk on X-Plane's own ATC key or a
  dedicated command, audio device selection.
- **On-device transcription** of ATC audio (Apple Silicon), private to the pilot's Mac.

## Near future

- **Windows** support for X-Plane 12. The plugin is Rust and Dear ImGui throughout; the
  macOS-specific pieces (Keychain storage, build and packaging) get Windows equivalents
  (Credential Manager, `win_x64` build).
- **Microsoft Flight Simulator** support. The core, VATSIM data and services crates are
  already simulator-independent; an MSFS front end would reuse them alongside a SimConnect
  integration.

## Later

- Optional premium features may be offered in future, subject to VATSIM's licensing for any
  feature that uses VATSIM data. The client itself stays free.
