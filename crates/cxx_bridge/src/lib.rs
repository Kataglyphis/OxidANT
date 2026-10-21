//! The cxx bridge BeschleunigerBallett's C++ engine calls; Corrosion's `corrosion_add_cxxbridge` generates its C++ half.

#[cxx::bridge]
mod ffi {
    extern "Rust" {
        fn rusty_cxxbridge_integer() -> i32;
    }
}

/// The value the engine logs at start-up, which proves the Rust link works.
pub fn rusty_cxxbridge_integer() -> i32 {
    42
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bridge_function_returns_42() {
        assert_eq!(super::rusty_cxxbridge_integer(), 42);
    }
}
