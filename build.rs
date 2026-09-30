// build.rs — cfg aliases + CXX bridge (when applicable).

/// Whether feature `name` is on for the crate; `cfg!()` here sees the build script's own features.
fn has_feature(name: &str) -> bool {
    // Cargo upper-cases the feature name and replaces `-` with `_`.
    let var = format!("CARGO_FEATURE_{}", name.to_uppercase().replace('-', "_"));
    std::env::var_os(&var).is_some()
}

fn main() {
    // Cfg aliases: `onnx` means any ONNX backend.
    if has_feature("onnx_tract") || has_feature("onnxruntime") {
        println!("cargo:rustc-cfg=onnx");
    }

    // `gui_wgpu_backend` means either wgpu GUI feature, regardless of host OS.
    if has_feature("gui_windows") || has_feature("gui_linux") {
        println!("cargo:rustc-cfg=gui_wgpu_backend");
    }

    // CXX bridge: cfg!() in a build script sees the host, so read the target arch from CARGO_CFG_*.
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if target_arch != "wasm32" {
        cxx_build::bridge("src/native_only.rs")
            .flag_if_supported("-std=c++17")
            .compile("kataglyphis_cxx");
    }
}
