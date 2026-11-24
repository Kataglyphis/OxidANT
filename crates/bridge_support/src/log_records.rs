//! The log records bridged calls leave on their thread: the engine drains them into its own log after each call, so
//! it logs each record on the thread whose call made it. Each bridge maps them onto its own `LogRecord` type.

use std::cell::RefCell;
use std::fmt::Write as _;

/// One record a bridged call logged at info or above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub level: log::Level,
    /// The module that logged it, such as `kataglyphis_assets::gltf_loader`.
    pub target: String,
    pub message: String,
}

/// A thread whose calls the engine never drains holds this many records, then counts the rest.
pub const KEPT_RECORDS: usize = 1024;

struct Records {
    kept: Vec<LogRecord>,
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
            self.kept.push(LogRecord {
                level: record.level(),
                target: record.target().to_owned(),
                message: message(record),
            });
        } else {
            self.dropped += 1;
        }
    }
}

/// Takes the records the calling thread's bridged calls logged, oldest first; a last warning counts those dropped.
pub fn take() -> Vec<LogRecord> {
    RECORDS.with_borrow_mut(|records| {
        let mut kept = std::mem::take(&mut records.kept);
        if records.dropped > 0 {
            kept.push(LogRecord {
                level: log::Level::Warn,
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
pub fn install() {
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
    fn a_record_stays_on_the_thread_that_logged_it() {
        install();
        take();
        log::warn!(target: "test", "from here");
        let elsewhere = std::thread::spawn(|| {
            log::error!(target: "test", "from there");
            take()
        })
        .join()
        .unwrap();
        let here = take();
        assert_eq!(
            here,
            [LogRecord {
                level: log::Level::Warn,
                target: "test".to_owned(),
                message: "from here".to_owned(),
            }]
        );
        assert_eq!(elsewhere.len(), 1);
        assert_eq!(elsewhere[0].level, log::Level::Error);
        assert!(take().is_empty(), "taking empties the thread's records");
    }

    #[test]
    fn debug_and_trace_records_are_not_kept() {
        take();
        install();
        for level in [log::Level::Info, log::Level::Debug, log::Level::Trace] {
            COLLECTOR.log(&record(level, format_args!("{level}")));
        }
        let kept = take();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].level, log::Level::Info);
        assert!(log::max_level() >= log::LevelFilter::Info);
        COLLECTOR.flush();
    }

    #[test]
    fn a_thread_that_is_never_drained_keeps_a_bounded_count() {
        let kept = std::thread::spawn(|| {
            for n in 0..KEPT_RECORDS + 3 {
                COLLECTOR.log(&record(log::Level::Warn, format_args!("{n}")));
            }
            take()
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
        assert_eq!(note.target, "kataglyphis_bridge_support::log_records");
        assert_eq!(note.level, log::Level::Warn);
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
            take()
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
}
