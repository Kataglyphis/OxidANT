// build.rs — the `desktop` cfg alias.

fn main() {
    // `desktop` gates the ESP32 bridge and the webcam engine; Android and wasm have no serial port and no GStreamer.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target_os.as_str(), "linux" | "windows" | "macos") {
        println!("cargo:rustc-cfg=desktop");
    }
    println!("cargo:rustc-check-cfg=cfg(desktop)");
}
