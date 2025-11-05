//! Fuzz target for u64 to i64 fixed-point conversions
//!
//! This fuzzer tests from_u64_checked() with arbitrary u64 values,
//! especially focusing on the i64::MAX boundary.

#![no_main]

use libfuzzer_sys::fuzz_target;
use bog_core::core::fixed_point;

fuzz_target!(|data: &[u8]| {
    // Convert bytes to u64
    if data.len() >= 8 {
        let value = u64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        // Test from_u64_checked
        let result = fixed_point::from_u64_checked(value);

        match result {
            Ok(converted) => {
                // If conversion succeeded, value must be <= i64::MAX
                assert!(value <= i64::MAX as u64,
                    "Conversion succeeded for value {} > i64::MAX", value);

                // Converted value should match
                assert_eq!(converted, value as i64,
                    "Conversion incorrect: {} -> {} (expected {})",
                    value, converted, value as i64);

                // Converted value should be non-negative
                assert!(converted >= 0,
                    "u64 {} converted to negative i64 {}", value, converted);
            }
            Err(_) => {
                // If conversion failed, value must be > i64::MAX
                assert!(value > i64::MAX as u64,
                    "Conversion failed for value {} <= i64::MAX", value);
            }
        }

        // Test legacy from_u64 for values in range
        if value <= i64::MAX as u64 {
            let legacy = fixed_point::from_u64(value);
            assert_eq!(legacy, value as i64,
                "Legacy conversion incorrect: {} -> {}", value, legacy);
        }

        // Test to_u64 round-trip for safe i64 values
        if value <= i64::MAX as u64 {
            let as_i64 = value as i64;
            let back = fixed_point::to_u64(as_i64);
            assert_eq!(back, value,
                "to_u64 round-trip failed: {} -> {} -> {}", value, as_i64, back);
        }

        // Test to_u64 with negative values (should clamp to 0)
        if data.len() >= 16 {
            let signed_value = i64::from_le_bytes([
                data[8], data[9], data[10], data[11],
                data[12], data[13], data[14], data[15],
            ]);

            let result = fixed_point::to_u64(signed_value);

            if signed_value < 0 {
                assert_eq!(result, 0,
                    "to_u64({}) should clamp to 0, got {}", signed_value, result);
            } else {
                assert_eq!(result, signed_value as u64,
                    "to_u64({}) incorrect: got {}", signed_value, result);
            }
        }
    }
});
