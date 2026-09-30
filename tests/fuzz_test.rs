use proptest::prelude::*;

// Example function to fuzz - replace with your actual API function.
fn parse_and_process_data(data: &str) -> bool {
    // Dummy logic with an edge case, the kind a fuzzer catches.
    if data.contains("panic_keyword") {
        // In real code this might be an actual bug you want to find.
        return false;
    }

    // Process input
    let _len = data.len();
    true
}

// Proptest generates random inputs for fuzz testing our function across Windows and Linux natively.
proptest! {
    #[test]
    fn test_parse_and_process_fuzz(s in "\\PC*") {
        // Assert that parsing the generated data doesn't crash or behave unexpectedly.
        let result = parse_and_process_data(&s);

        // Any string without the bug condition must pass.
        if !s.contains("panic_keyword") {
            prop_assert!(result);
        }
    }
}
