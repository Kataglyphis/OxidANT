//! The cxx bridge BeschleunigerBallett's C++ engine calls; Corrosion's `corrosion_add_cxxbridge` generates its C++ half.
//!
//! One bridge file per area, each in the C++ namespace `kataglyphis::oxidant::<file stem>`: [`gltf`], [`settings`]
//! and [`diagnostics`]. Every bridged function runs inside `guarded` and returns its errors as values, since the
//! engine builds without exceptions; a path arrives as UTF-8 bytes. `contract.rs` holds every bridge file to that.

// The owner's hard gate on function size; clippy.toml holds the limit.
#![warn(clippy::too_many_lines)]

#[cfg(test)]
mod contract;
pub mod diagnostics;
pub mod gltf;
pub mod settings;

/// Runs one bridged call: a panic would abort at cxx's boundary, so it becomes `on_panic(reason)` here.
/// Caught under `panic = "unwind"`, which the dev, test and `bridge` profiles have; `release` aborts instead.
pub(crate) fn guarded<T>(call: impl FnOnce() -> T, on_panic: impl FnOnce(String) -> T) -> T {
    diagnostics::install();
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(call))
        .unwrap_or_else(|panic| on_panic(panic_reason(panic.as_ref())))
}

/// The panic fallback of a call that has no error to report: its type's empty value.
pub(crate) fn nothing<T: Default>(_reason: String) -> T {
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
}
