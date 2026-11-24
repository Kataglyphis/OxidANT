//! What both C++ bridges share: [`guarded`], which turns a panic into the call's fallback value, the log records a
//! bridged call leaves on its thread ([`log_records`]), and the source checks of the bridge contract ([`contract`]).
//! `kataglyphis_cxx_bridge` (BeschleunigerBallett's engine) and `kataglyphis_accel_bridge` (AccelerANTgine's) link it.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]
// Both bridges link it into a C++ engine, where its sanitizers and FuzzTest cannot reach.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

pub mod contract;
pub mod log_records;

/// Runs one bridged call: a panic would abort at cxx's boundary, so it becomes `on_panic(reason)` here.
/// Caught under `panic = "unwind"`, which the dev, test and `bridge` profiles have; `release` aborts instead.
pub fn guarded<T>(call: impl FnOnce() -> T, on_panic: impl FnOnce(String) -> T) -> T {
    log_records::install();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
        .unwrap_or_else(|panic| on_panic(panic_reason(panic.as_ref())))
}

/// The panic fallback of a call that has no error to report: its type's empty value.
pub fn nothing<T: Default>(_reason: String) -> T {
    T::default()
}

/// `panic!`'s message, or nothing for a payload that is not text.
fn panic_reason(panic: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = panic.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    panic.downcast_ref::<String>().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_names_its_reason() {
        let text = std::panic::catch_unwind(|| panic!("static reason")).unwrap_err();
        assert_eq!(panic_reason(text.as_ref()), "static reason");
        let owned = std::panic::catch_unwind(|| panic!("{}", String::from("owned"))).unwrap_err();
        assert_eq!(panic_reason(owned.as_ref()), "owned");
        let other = std::panic::catch_unwind(|| std::panic::panic_any(7)).unwrap_err();
        assert_eq!(panic_reason(other.as_ref()), "");
    }

    #[test]
    fn a_call_returns_its_value_and_a_panic_its_fallback() {
        assert_eq!(guarded(|| 7, |_| 0), 7);
        let reason = guarded(|| panic!("broken accessor"), |reason| reason);
        assert_eq!(reason, "broken accessor");
        let empty: Vec<u8> = guarded(|| panic!("no value"), nothing);
        assert!(empty.is_empty());
        assert_eq!(nothing::<String>("ignored".to_owned()), "");
    }

    #[test]
    fn a_guarded_call_installs_the_collector() {
        guarded(|| (), nothing::<()>);
        assert!(log::max_level() >= log::LevelFilter::Info);
    }
}
