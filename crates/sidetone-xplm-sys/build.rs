use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let sdk = manifest.join("../../third_party/XPSDK").canonicalize().expect("X-Plane SDK not found in third_party/XPSDK");
    let headers = sdk.join("CHeaders/XPLM");

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed={}", headers.display());

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", headers.display()))
        .clang_args(["-DAPL=1", "-DIBM=0", "-DLIN=0"])
        .clang_args(["-DXPLM200", "-DXPLM210", "-DXPLM300", "-DXPLM301", "-DXPLM303"])
        .clang_args(["-DXPLM400", "-DXPLM410", "-DXPLM411", "-DXPLM420", "-DXPLM430"])
        .allowlist_function("XPLM.*")
        .allowlist_type("XPLM.*")
        .allowlist_var("xplm.*|XPLM.*")
        .prepend_enum_name(false)
        .layout_tests(false)
        .generate_comments(false)
        .generate()
        .expect("bindgen failed for XPLM headers");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("xplm.rs")).unwrap();

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-search=framework={}", sdk.join("Libraries/Mac").display());
        println!("cargo:rustc-link-lib=framework=XPLM");
    }
}
