// build.rs — the `desktop` cfg alias + CXX bridge (when applicable).

fn main() {
    // `desktop` gates the ESP32 bridge and the webcam engine; Android and wasm have no serial port and no GStreamer.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target_os.as_str(), "linux" | "windows" | "macos") {
        println!("cargo:rustc-cfg=desktop");
    }
    println!("cargo:rustc-check-cfg=cfg(desktop)");

    // CXX bridge: cfg!() in a build script sees the host, so read the target arch from CARGO_CFG_*.
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "wasm32" {
        cxx_build::bridge("src/native_only.rs")
            .flag_if_supported("-std=c++17")
            .compile("kataglyphis_cxx");
    }
}
