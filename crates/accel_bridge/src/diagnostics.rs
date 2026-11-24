//! What the engine reports about OxidANT, in the C++ namespace `kataglyphis::oxidant::accel::diagnostics`: its
//! version, which a package's `--version` smoke test prints, and the log records bridged calls leave on their thread,
//! which the engine drains into its own log after each call.

use kataglyphis_bridge_support::log_records;

use crate::{guarded, nothing};

#[allow(unsafe_code)]
#[cxx::bridge(namespace = "kataglyphis::oxidant::accel::diagnostics")]
pub mod ffi {
    /// The `log` crate's levels and numbers.
    #[derive(Debug)]
    enum LogLevel {
        Error = 1,
        Warn = 2,
        Info = 3,
        Debug = 4,
        Trace = 5,
    }

    /// One record a bridged call logged.
    #[derive(Debug, PartialEq)]
    struct LogRecord {
        level: LogLevel,
        /// The module that logged it, such as `kataglyphis_text::genai`.
        target: String,
        message: String,
    }

    extern "Rust" {
        /// OxidANT's version, the one its workspace `Cargo.toml` names.
        fn oxidant_version() -> String;
        /// Takes the records the calling thread's bridged calls logged at info and above, oldest first.
        fn take_log_records() -> Vec<LogRecord>;
    }
}

/// Never unwinds into C++, like every bridged call.
pub fn oxidant_version() -> String {
    guarded(|| env!("CARGO_PKG_VERSION").to_owned(), nothing)
}

/// The records wait per thread, so the engine logs each one on the thread whose call made it.
pub fn take_log_records() -> Vec<ffi::LogRecord> {
    guarded(
        || log_records::take().into_iter().map(record).collect(),
        nothing,
    )
}

fn record(from: log_records::LogRecord) -> ffi::LogRecord {
    ffi::LogRecord {
        level: level(from.level),
        target: from.target,
        message: from.message,
    }
}

fn level(level: log::Level) -> ffi::LogLevel {
    match level {
        log::Level::Error => ffi::LogLevel::Error,
        log::Level::Warn => ffi::LogLevel::Warn,
        log::Level::Info => ffi::LogLevel::Info,
        log::Level::Debug => ffi::LogLevel::Debug,
        log::Level::Trace => ffi::LogLevel::Trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_workspace_one() {
        assert_eq!(oxidant_version(), env!("CARGO_PKG_VERSION"));
        assert!(!oxidant_version().is_empty());
    }

    #[test]
    fn a_bridged_call_leaves_its_records_on_its_own_thread() {
        take_log_records();
        guarded(|| log::warn!(target: "test", "from here"), nothing::<()>);
        let elsewhere = std::thread::spawn(|| {
            guarded(|| log::error!(target: "test", "from there"), nothing::<()>);
            take_log_records()
        })
        .join()
        .unwrap();
        assert_eq!(
            take_log_records(),
            [ffi::LogRecord {
                level: ffi::LogLevel::Warn,
                target: "test".to_owned(),
                message: "from here".to_owned(),
            }]
        );
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(elsewhere[0].level, ffi::LogLevel::Error);
    }

    #[test]
    fn every_level_keeps_its_name_and_number() {
        for (from, expected) in [
            (log::Level::Error, ffi::LogLevel::Error),
            (log::Level::Warn, ffi::LogLevel::Warn),
            (log::Level::Info, ffi::LogLevel::Info),
            (log::Level::Debug, ffi::LogLevel::Debug),
            (log::Level::Trace, ffi::LogLevel::Trace),
        ] {
            assert_eq!(level(from), expected);
            assert_eq!(usize::from(level(from).repr), from as usize);
            assert_eq!(format!("{:?}", level(from)), format!("{from:?}"));
        }
    }
}
