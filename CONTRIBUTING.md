# Contributing

Thanks for helping! Issues and pull requests are welcome.

## Setup

- macOS with Xcode command-line tools.
- Rust via [rustup](https://rustup.rs) (stable). For universal builds:
  `rustup target add aarch64-apple-darwin x86_64-apple-darwin`.
- X-Plane 12 for testing the plugin. The X-Plane SDK is vendored in `third_party/XPSDK`.

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo xtask install            # build and copy into ~/X-Plane 12 (or --xplane <path>)
```

The live examples hit VATSIM's public APIs and are handy when working on naming or coverage:

```sh
cargo run -p sidetone-vatsim --example live_names
cargo run -p sidetone-vatsim --example live_route -- EGLL LEMD
```

## Rules of the road

- **Only the main thread touches X-Plane.** Network and heavy work go on a worker and come
  back through the event bus. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
- **Every callback from X-Plane is guarded** (`sidetone_xplm::guard`); don't add a new
  `extern "C"` entry point without one.
- **Logic goes where it can be tested.** If it doesn't need X-Plane, put it in
  `sidetone-core`, `sidetone-vatsim` or `sidetone-services` with unit tests.
- **No secrets in code, logs or settings files.** Use the Keychain helpers and the
  `Redacted` wrapper.
- **Respect the networks we use.** Keep polling intervals as documented in
  [docs/VATSIM.md](docs/VATSIM.md). Never connect an unapproved build to VATSIM's live
  network.
- CI must pass: formatting, clippy with warnings denied, tests and a universal bundle.

## Licence

By contributing you agree your contributions are licensed under the Apache License 2.0.
