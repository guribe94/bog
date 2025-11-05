//! Fuzz target for fixed-point conversions
//!
//! This fuzzer tests the from_f64_checked() function with arbitrary f64 inputs,
//! looking for panics, crashes, or undefined behavior.

#![no_main]

use libfuzzer_sys::fuzz_target;
use bog_core::core::fixed_point;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to f64 (this gives us arbitrary f64 values including NaN, infinity, etc.)
    if data.len() >= 8 {
        let value = f64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        // Test from_f64_checked - should never panic
        let result = fixed_point::from_f64_checked(value);

        // Verify result consistency
        match result {
            Ok(fixed) => {
                // If conversion succeeded, round-trip should be close
                let back = fixed_point::to_f64(fixed);

                // For normal (non-NaN) values, verify round-trip is reasonable
                if value.is_finite() {
                    let error = (value - back).abs();
                    // Error should be bounded by precision
                    assert!(error < value.abs() * 1e-6 + 1e-8,
                        "Round-trip error too large: {} -> {} -> {} (error: {})",
                        value, fixed, back, error);
                }
            }
            Err(_) => {
                // If conversion failed, value should be NaN, infinite, or out of range
                assert!(
                    value.is_nan() ||
                    value.is_infinite() ||
                    value > fixed_point::MAX_SAFE_F64 ||
                    value < fixed_point::MIN_SAFE_F64,
                    "Conversion failed but value {} appears valid", value
                );
            }
        }

        // Test legacy from_f64 - should not panic for reasonable values
        if value.is_finite() &&
           value >= fixed_point::MIN_SAFE_F64 &&
           value <= fixed_point::MAX_SAFE_F64 {
            let _legacy = fixed_point::from_f64(value);
        }
    }
});
