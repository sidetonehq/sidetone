# Architecture

Sidetone is a single X-Plane plugin (`Sidetone.xpl`) written in Rust. This document explains
how it is put together and the rules that keep it fast and safe inside the simulator.

## Goals

1. **Never cost the pilot frames.** The sim's main thread only draws and applies small state
   changes; everything slow happens elsewhere.
2. **Never crash the simulator.** A bug in Sidetone must end up in a log file, not take X-Plane
   down.
3. **Keep simulator code thin.** Logic lives in crates with no X-Plane dependency so it can be
   unit tested on any machine — and reused if Sidetone ever supports another simulator.

## Crates

```
                ┌───────────────────────────── sidetone-plugin (cdylib → Sidetone.xpl)
                │                                   entry points, wiring, screens
                │
   ┌────────────┼──────────────┬──────────────────────┬────────────────────┐
   ▼            ▼              ▼                      ▼                    ▼
sidetone-ui  sidetone-core  sidetone-vatsim      sidetone-services   sidetone-xplm
ImGui host   state, settings  data feed, naming,  SimBrief, Hoppie,   safe X-Plane
GL renderer  alerts, parsing  coverage, events    Keychain            SDK wrappers
   │                                                                       │
   └──────────────────────────────► sidetone-xplm ──► sidetone-xplm-sys (bindgen, SDK 4.3)
```

| Crate | Depends on X-Plane? | Tested by |
|---|---|---|
| `sidetone-core` | No | unit tests |
| `sidetone-vatsim` | No | unit tests + live examples |
| `sidetone-services` | No | unit tests |
| `sidetone-xplm-sys`, `sidetone-xplm` | Yes | manual testing in X-Plane |
| `sidetone-ui` | Yes | manual testing in X-Plane |
| `sidetone-plugin` | Yes | manual testing in X-Plane |

## Threads

| Thread | Does | Talks to the sim? |
|---|---|---|
| X-Plane main thread | draw callbacks, the 20 Hz flight loop, commands, menus | **Only this one** |
| `sidetone-vatsim` worker | data feed + transceivers (15 s), METARs, events, stats, VATSpy and boundary downloads, route-ATC computation | No |
| `sidetone-services` worker | Hoppie ACARS polling/sending, SimBrief import | No |

Workers never call the X-Plane SDK. They post events onto a channel (`sidetone_core::bus`);
the flight loop drains it and applies the results to the single `Model` the screens read.
Workers are joined when the plugin is disabled, so no Sidetone code runs after X-Plane unloads
the plugin.

## Safety

- Every function X-Plane calls into is wrapped in `sidetone_xplm::guard`, which catches Rust
  panics, logs them (collapsing repeats) and returns a safe value.
- Each screen's build function is guarded separately so a UI bug cannot leave a Dear ImGui
  frame half-open.
- Secrets (Hoppie logon code, SimBrief username) are stored only in the macOS Keychain. The
  UI passes them in a `Redacted` wrapper whose `Debug` output never shows the value.

## Rendering

X-Plane 12 on macOS renders with Metal and offers plugins a legacy (2.1-class) OpenGL
compatibility context. Sidetone uses [Dear ImGui](https://github.com/ocornut/imgui) through
`dear-imgui-rs` and a small fixed-function renderer (`sidetone-ui/src/renderer.rs`), in the
same spirit as ImGui's own `imgui_impl_opengl2`:

- One ImGui context per X-Plane window (the panel and the main window).
- Text is rasterised at the display's native pixel density (2× on Retina).
- **Frame caching:** when nobody is interacting with a window, the UI is rebuilt at most 15
  times a second; in between, the previous frame's geometry is redrawn. Hovering, typing or
  clicking rebuilds every frame.
- Long lists (the online ATC table) use ImGui's list clipper and a cached, pre-sorted row list,
  so hundreds of stations cost the same as a screenful.

Settings → About shows Sidetone's measured CPU time per frame.

## Keyboard

Sidetone takes X-Plane's keyboard focus **only** while a text field is active, and gives it
back on Enter, Escape, when X-Plane reports focus loss, or — as a safety net — after 5 seconds
without typing while the mouse is outside the window. While it holds the keyboard the panel
says so. Cmd+C/V/X/A/Z use the system clipboard.

Leaving a field never discards what was typed. ImGui's Escape means "cancel and revert", so
the host never forwards Escape: Escape, focus loss and the idle safety net all end editing with
`ClearActiveID`, which keeps the text and lets the field report "deactivated after edit" (and save).

## Data sources

See [VATSIM.md](VATSIM.md) for exactly what Sidetone fetches and how often.

## Adding a screen

1. Add `src/ui/<screen>.rs` in `sidetone-plugin` with a `build(ui, &mut Model)` function.
2. Read from `Model`; never call network or X-Plane APIs from UI code. To change something,
   push an `Action` (`app.rs`), which the flight loop applies.
   For a main window tab, add it in `ui/main_window.rs`; to let another screen switch to it, add
   a variant to `ui::Tab` and set `m.ui.select_tab`.
3. Put any logic worth testing in `sidetone-core` (or the relevant data crate) with tests.
