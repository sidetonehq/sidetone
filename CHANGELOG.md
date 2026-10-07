# Changelog

## 0.1.0 — unreleased

First public preview for X-Plane 12 on macOS (Apple Silicon and Intel). Everything here runs on
VATSIM's public data alongside your current pilot client; Sidetone doesn't connect to the network
as a pilot yet.

### Panel and window

- A small panel in the corner: callsign and connection, both radios with the station you'd hear
  ("Gatwick Ground", 8.33 kHz aware, nearest transmitter wins), a transponder check, the latest
  message, and a transmit light. Translucent until you hover it.
- The main window opens from the panel: docked, popped out, in VR, or attached under the panel
  as a sidebar that moves with it. Every screen has a compact layout for narrow windows, and
  the window can't be made narrower than the panel.
- Import flight (SimBrief) in the header on every tab, which starts a new flight.

### Flight tab

- The whole flight on one page, in cards you can fold away.
- **Get set up**: a first-run checklist (CID, push-to-talk test, SimBrief, xPilot) that ticks
  itself off.
- **Who to call** right now, one click from your radio.
- **Flight**: departure and arrival with their ATIS letter, QNH and event badges (the full ATIS
  and METAR on hover, click to keep open), and the route as filed on VATSIM with Copy and a
  warning if it differs from SimBrief.
- **Clearance**: who to call until you're cleared, then your clearance as tiles in readback
  order. The squawk is checked against your transponder and the ATIS letter against the
  current one (green tick, or amber value and warning). Fills itself from SimBrief, VATSIM, the
  ATIS and the METAR; anything you type wins. Folds away at takeoff.
- **ATC along route**: who's staffed along the way with one-click tuning, and "Switch to
  UNICOM" when nobody covers where you are.
- **Arrival**: runway and STAR from SimBrief, ATIS letter, QNH and transition level kept
  current, plus the approach, stand and frequency you're told. Opens at takeoff.
- **Notes**, kept until your next import.

### ATC, Messages and Settings

- **ATC**: every station within 300 nm (or the whole network), grouped by type and nearest
  first, with search, controller info and one-click COM1/COM2 tuning. Stations that share a
  name say where they are ("Langen Radar · Dusseldorf APP"), and hover shows where a spoken name
  came from.
- **Messages**: notices and dot commands (`.com1 122.8`, `.x 7000`, `.metar EGLL`, `.clear`).
- ATIS letter and METAR change notices for your airports, and VATSIM events at them.
- **Settings**: UI scale, panel opacity, push-to-talk on X-Plane's Contact ATC key or a
  dedicated command, your pilot and ATC hours, and the cost per frame.
- Your VATSIM CID is found automatically from your live callsign or SimBrief plan.
- Optional **xPilot companion mode**: xPilot's connection in the panel, receive lights, SELCAL
  and one shared push-to-talk key. Read-only towards the network.

### Under the hood

- No effect on frame rate: network work runs on background threads and the UI redraws from a
  cache when you're not using it.
- Sidetone only holds the keyboard while you type, and hands it back on Enter, Escape or when you
  move away (Cmd+C/V/X/A/Z work).
- Your SimBrief Pilot ID is kept in the macOS Keychain, never in a file.
- CPDLC and PDC stay with your aircraft's own ACARS (Hoppie).
