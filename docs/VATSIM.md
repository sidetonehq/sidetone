# Sidetone and the VATSIM network

This page states plainly what Sidetone does and does not do with VATSIM, for pilots and for
VATSIM's technical team.

## Current status

Sidetone is **not yet an approved VATSIM pilot client** and therefore **does not connect to the
VATSIM network** (FSD or Audio for VATSIM). Pilots who use it today connect with an approved
client as usual; Sidetone works alongside it.

The network client is being developed against a private, local test server only. It will be
enabled for the live network only after VATSIM approval, with a client identifier and key
issued by VATSIM, and never committed to this repository.

## What Sidetone reads today (public, read-only)

| Data | Endpoint | Frequency |
|---|---|---|
| Feed URLs | `status.vatsim.net/status.json` | once at start-up |
| Pilots, controllers, ATIS, prefiles | `data.vatsim.net/v3/vatsim-data.json` | every 15 s (the feed's own refresh) |
| Transmitter frequencies and positions | `data.vatsim.net/v3/transceivers-data.json` | every 15 s |
| METARs for the pilot's departure/arrival | `metar.vatsim.net` | every 5 min, or when the airports change |
| Events | `my.vatsim.net/api/v2/events/latest` | hourly |
| The pilot's own hours | `api.vatsim.net/v2/members/{cid}/stats` | hourly, only if the pilot enters their CID |
| Airport/FIR names and FIR boundaries | VATSpy data project (GitHub) | weekly, cached on disk |

Requests identify themselves with a `Sidetone/<version>` user agent. If the data feed is
unavailable, Sidetone backs off to 30 s and reports the problem once.

## What Sidetone never does

- It never connects to FSD or AFV without VATSIM approval.
- It never relays its own traffic through another client's connection. (Doing so would make
  it an unapproved client by proxy.) Any optional integration with an approved client is
  read-only: status display and push-to-talk forwarding.
- It never transmits synthetic or altered speech.
- It never sends audio anywhere. Planned speech-to-text runs entirely on the pilot's Mac.
- It collects no telemetry.

## Pilot client requirements (planned)

The native client is being designed to support everything the Code of Conduct expects of a
pilot: transponder and Mode C, holding the assigned squawk, text messaging and UNICOM 122.800,
text-only and receive-only modes, flight plan filing, the unattended-connection limit, a single
connection per pilot, and unaltered speech.

Several of these are already supported in the read-only features: the panel warns when the
transponder doesn't match the assigned squawk or isn't reporting altitude while airborne, and
suggests UNICOM when no controller covers the aircraft.

## Third-party services

- **Hoppie ACARS** (CPDLC and pre-departure clearances) is a separate network. Sidetone follows
  Hoppie's polling etiquette (no polling before the first message, then 45–75 s, briefly 20 s
  after sending).
- **SimBrief** is used only to import the pilot's own latest flight plan.

## Contact

Questions from VATSIM staff are welcome via the repository's issues or the maintainer's contact
details in the repository profile.
