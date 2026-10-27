#[path = "../../third_party/ANTfrastructure/shared/rust/windows_icon.rs"]
mod windows_icon;

fn main() {
    let icon = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../images/logo.ico");
    windows_icon::add_windows_icon("kataglyphis_cli", &icon);
}
