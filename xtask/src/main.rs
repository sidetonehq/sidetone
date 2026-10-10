//! Build helper: `cargo xtask bundle|install|version [--debug] [--xplane <path>]`.
//!
//! Produces the X-Plane plugin layout `dist/Sidetone/mac_x64/Sidetone.xpl` (the folder is called
//! `mac_x64` even for universal binaries). Builds arm64 + x86_64 and merges them with `lipo` when
//! both Rust targets are installed; otherwise builds the host architecture only.
//!
//! Signing: ad-hoc by default. With `SIDETONE_SIGN_IDENTITY` set (e.g. "Developer ID
//! Application: …", in the keychain), signs for distribution instead: hardened runtime and a
//! secure timestamp, as notarization requires.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, exit};

const TARGETS: [&str; 2] = ["aarch64-apple-darwin", "x86_64-apple-darwin"];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let task = args.first().map(String::as_str).unwrap_or("help");
    let release = !args.iter().any(|a| a == "--debug");
    let xplane = args.iter().position(|a| a == "--xplane").and_then(|i| args.get(i + 1)).map(PathBuf::from);

    match task {
        "bundle" => {
            bundle(release);
        }
        "install" => {
            let plugin = bundle(release);
            install(&plugin, xplane);
        }
        // The workspace version, for release checks (a tag must match it).
        "version" => println!("{}", env!("CARGO_PKG_VERSION")),
        _ => {
            eprintln!("usage: cargo xtask <bundle|install|version> [--debug] [--xplane <X-Plane folder>]");
            exit(2);
        }
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| panic!("failed to run {cmd:?}: {e}"));
    if !status.success() {
        eprintln!("command failed: {cmd:?}");
        exit(1);
    }
}

fn installed_targets() -> Vec<&'static str> {
    let sysroot = Command::new("rustc").args(["--print", "sysroot"]).output().expect("rustc");
    let sysroot = PathBuf::from(String::from_utf8_lossy(&sysroot.stdout).trim());
    TARGETS.into_iter().filter(|t| sysroot.join("lib/rustlib").join(t).exists()).collect()
}

/// Builds the plugin and returns the `dist/Sidetone` folder.
fn bundle(release: bool) -> PathBuf {
    let root = root();
    let profile = if release { "release" } else { "debug" };
    let targets = installed_targets();
    if targets.len() < TARGETS.len() {
        eprintln!(
            "warning: building {} only. For a universal binary install rustup and run `rustup target add {}`.",
            targets.join(", "),
            TARGETS.iter().filter(|t| !targets.contains(t)).cloned().collect::<Vec<_>>().join(" ")
        );
    }

    let mut slices = Vec::new();
    for target in &targets {
        let mut cmd = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
        cmd.current_dir(&root).args(["build", "-p", "sidetone-plugin", "--target", target]);
        if release {
            cmd.arg("--release");
        }
        run(&mut cmd);
        slices.push(root.join("target").join(target).join(profile).join("libsidetone.dylib"));
    }

    let out_dir = root.join("dist/Sidetone/mac_x64");
    std::fs::create_dir_all(&out_dir).unwrap();
    let xpl = out_dir.join("Sidetone.xpl");
    let _ = std::fs::remove_file(&xpl);
    if slices.len() == 1 {
        std::fs::copy(&slices[0], &xpl).unwrap();
    } else {
        let mut lipo = Command::new("lipo");
        lipo.arg("-create").args(&slices).arg("-output").arg(&xpl);
        run(&mut lipo);
    }
    run(Command::new("install_name_tool").args(["-id", "Sidetone.xpl"]).arg(&xpl));
    sign(&xpl);

    let readme = root.join("dist/Sidetone/README.txt");
    std::fs::write(readme, format!("Sidetone {}\nhttps://github.com/sidetonehq/sidetone\n", env!("CARGO_PKG_VERSION"))).unwrap();
    println!("bundled {}", xpl.display());
    root.join("dist/Sidetone")
}

/// Signs the plugin: for distribution when `SIDETONE_SIGN_IDENTITY` names a Developer ID
/// certificate, else ad-hoc (enough for a local build).
fn sign(xpl: &Path) {
    match env::var("SIDETONE_SIGN_IDENTITY").ok().filter(|id| !id.trim().is_empty()) {
        Some(identity) => {
            run(Command::new("codesign").args(["--force", "--options", "runtime", "--timestamp", "--sign", &identity]).arg(xpl));
            run(Command::new("codesign").args(["--verify", "--strict", "--verbose=2"]).arg(xpl));
            println!("signed with {identity}");
        }
        None => run(Command::new("codesign").args(["--force", "--sign", "-"]).arg(xpl)),
    }
}

fn install(plugin: &Path, xplane: Option<PathBuf>) {
    let xplane =
        xplane.or_else(|| env::var_os("XPLANE_PATH").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from(env::var("HOME").unwrap()).join("X-Plane 12"));
    let plugins = xplane.join("Resources/plugins");
    if !plugins.is_dir() {
        eprintln!("{} is not an X-Plane folder (pass --xplane or set XPLANE_PATH)", xplane.display());
        exit(1);
    }
    let dest = plugins.join("Sidetone");
    let _ = std::fs::remove_dir_all(&dest);
    run(Command::new("cp").arg("-R").arg(plugin).arg(&dest));
    println!("installed to {}", dest.display());
}
