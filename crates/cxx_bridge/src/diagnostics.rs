//! What the engine reports about OxidANT, in the C++ namespace `kataglyphis::oxidant::diagnostics`: its version, and
//! the log records bridged calls leave on their thread, which the engine drains into its own log after each call.

use std::cell::RefCell;
use std::fmt::Write as _;

use crate::{guarded, nothing};

#[cxx::bridge(namespace = "kataglyphis::oxidant::diagnostics")]
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
        /// The module that logged it, such as `kataglyphis_assets::gltf_loader`.
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
    guarded(drain, nothing)
}

/// A thread whose calls the engine never drains holds this many records, then counts the rest.
const KEPT_RECORDS: usize = 1024;

struct Records {
    kept: Vec<ffi::LogRecord>,
    dropped: usize,
}

thread_local! {
    static RECORDS: RefCell<Records> = const {
        RefCell::new(Records {
            kept: Vec::new(),
            dropped: 0,
        })
    };
}

impl Records {
    fn push(&mut self, record: &log::Record<'_>) {
        if self.kept.len() < KEPT_RECORDS {
            self.kept.push(ffi::LogRecord {
                level: level(record.level()),
                target: record.target().to_owned(),
                message: message(record),
            });
        } else {
            self.dropped += 1;
        }
    }
}

fn drain() -> Vec<ffi::LogRecord> {
    RECORDS.with_borrow_mut(|records| {
        let mut kept = std::mem::take(&mut records.kept);
        if records.dropped > 0 {
            kept.push(ffi::LogRecord {
                level: ffi::LogLevel::Warn,
                target: module_path!().to_owned(),
                message: format!("{} more log records were dropped", records.dropped),
            });
            records.dropped = 0;
        }
        kept
    })
}

// A Display impl that fails keeps what it wrote: `to_string` would panic instead.
fn message(record: &log::Record<'_>) -> String {
    let mut message = String::new();
    let _ = write!(message, "{}", record.args());
    message
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

/// The process's `log` logger in the engine; it keeps each record on the thread that logged it.
struct Collector;

impl log::Log for Collector {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    // Never panics: a thread being torn down has no records, and one logged while another formats is dropped.
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            let _ = RECORDS
                .try_with(|records| records.try_borrow_mut().map(|mut kept| kept.push(record)));
        }
    }

    fn flush(&self) {}
}

static COLLECTOR: Collector = Collector;

/// The first bridged call installs the collector, so the engine needs no start-up call; a logger already set stays.
pub(crate) fn install() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        if log::set_logger(&COLLECTOR).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    });
}

#[cfg(test)]
mod tests {
    use log::Log as _;

    use super::*;

    fn record<'a>(level: log::Level, args: std::fmt::Arguments<'a>) -> log::Record<'a> {
        log::Record::builder()
            .level(level)
            .target("test")
            .args(args)
            .build()
    }

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
        let here = take_log_records();
        assert_eq!(
            here,
            [ffi::LogRecord {
                level: ffi::LogLevel::Warn,
                target: "test".to_owned(),
                message: "from here".to_owned(),
            }]
        );
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(elsewhere[0].level, ffi::LogLevel::Error);
        assert!(
            take_log_records().is_empty(),
            "taking empties the thread's records"
        );
    }

    #[test]
    fn debug_and_trace_records_are_not_kept() {
        take_log_records();
        install();
        for level in [log::Level::Info, log::Level::Debug, log::Level::Trace] {
            COLLECTOR.log(&record(level, format_args!("{level}")));
        }
        let kept = take_log_records();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].level, ffi::LogLevel::Info);
        assert!(log::max_level() >= log::LevelFilter::Info);
        COLLECTOR.flush();
    }

    #[test]
    fn a_thread_that_is_never_drained_keeps_a_bounded_count() {
        let kept = std::thread::spawn(|| {
            for n in 0..KEPT_RECORDS + 3 {
                COLLECTOR.log(&record(log::Level::Warn, format_args!("{n}")));
            }
            take_log_records()
        })
        .join()
        .unwrap();
        assert_eq!(kept.len(), KEPT_RECORDS + 1);
        assert_eq!(
            kept[KEPT_RECORDS - 1].message,
            (KEPT_RECORDS - 1).to_string()
        );
        let note = kept.last().unwrap();
        assert_eq!(note.message, "3 more log records were dropped");
        assert_eq!(note.target, "kataglyphis_cxx_bridge::diagnostics");
    }

    #[test]
    fn a_record_logged_while_another_formats_is_dropped_not_a_panic() {
        struct Nested;
        impl std::fmt::Display for Nested {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                COLLECTOR.log(&record(log::Level::Warn, format_args!("nested")));
                f.write_str("outer")
            }
        }
        let kept = std::thread::spawn(|| {
            COLLECTOR.log(&record(log::Level::Warn, format_args!("{}", Nested)));
            take_log_records()
        })
        .join()
        .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].message, "outer");
    }

    #[test]
    fn a_display_that_fails_keeps_what_it_wrote() {
        struct Failing;
        impl std::fmt::Display for Failing {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("partial")?;
                Err(std::fmt::Error)
            }
        }
        let text = message(&record(log::Level::Warn, format_args!("{Failing} rest")));
        assert_eq!(text, "partial");
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
