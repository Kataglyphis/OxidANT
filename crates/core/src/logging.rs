// src/logging.rs — Logger initialisation and the `success!` macro.

use std::io::Write;

/// Initialise the `env_logger` with a loguru-style format (`HH:MM:SS | LEVEL | msg`).
/// Without `RUST_LOG`, `KATAGLYPHIS_LOG_LEVEL` sets the level and wgpu/naga are clamped to WARN.
pub fn init_logger() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default());

    builder.format(|buf, record| {
        let ts = chrono::Local::now().format("%H:%M:%S");
        let level = if record.target() == "SUCCESS" {
            "SUCCESS"
        } else {
            match record.level() {
                log::Level::Error => "ERROR",
                log::Level::Warn => "WARN",
                log::Level::Info => "INFO",
                log::Level::Debug => "DEBUG",
                log::Level::Trace => "TRACE",
            }
        };
        writeln!(buf, "{ts} | {level:<8} | {}", record.args())
    });

    if std::env::var_os("RUST_LOG").is_none() {
        let level = crate::config::log_level();

        builder.filter_level(level);

        // Suppress very chatty modules.
        for module in &["wgpu", "wgpu_core", "wgpu_hal", "naga"] {
            builder.filter_module(module, log::LevelFilter::Warn);
        }
    }

    builder.init();
}

/// Log at INFO with target `"SUCCESS"`, which the formatter labels `SUCCESS`.
#[macro_export]
macro_rules! success {
    ($($arg:tt)*) => {
        log::info!(target: "SUCCESS", $($arg)*);
    };
}
