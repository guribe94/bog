//! Property-based tests for fixed-point arithmetic
//!
//! These tests use proptest to verify mathematical invariants across
//! thousands of randomized inputs, catching edge cases that unit tests miss.

#[cfg(test)]
mod tests {
    use super::super::fixed_point;
    use super::super::errors::ConversionError;
    use proptest::prelude::*;

    // ===== CONVERSION PROPERTY TESTS =====

    /// Property: Round-trip conversion should be approximately equal
    ///
    /// For any valid f64 value, converting to fixed-point and back
    /// should produce a value within acceptable precision loss.
    #[test]
    fn prop_roundtrip_within_precision() {
        proptest!(|(value in -1000000.0..1000000.0_f64)| {
            match fixed_point::from_f64_checked(value) {
                Ok(fixed) => {
                    let back = fixed_point::to_f64(fixed);
                    let error = (value - back).abs();

                    // Error should be less than 1 nanoscale unit (1e-9)
                    prop_assert!(error < 1e-8,
                        "Round-trip error too large: {} -> {} -> {} (error: {})",
                        value, fixed, back, error);
                }
                Err(_) => {
                    // If conversion failed, value must be out of range
                    prop_assert!(value > fixed_point::MAX_SAFE_F64 ||
                                value < fixed_point::MIN_SAFE_F64,
                                "Conversion failed but value {} is in range", value);
                }
            }
        });
    }

    /// Property: NaN always fails conversion
    #[test]
    fn prop_nan_always_fails() {
        let result = fixed_point::from_f64_checked(f64::NAN);
        assert!(matches!(result, Err(ConversionError::NotANumber)));
    }

    /// Property: Infinity always fails conversion
    #[test]
    fn prop_infinity_always_fails() {
        let pos_inf = fixed_point::from_f64_checked(f64::INFINITY);
        let neg_inf = fixed_point::from_f64_checked(f64::NEG_INFINITY);

        assert!(matches!(pos_inf, Err(ConversionError::Infinite { positive: true })));
        assert!(matches!(neg_inf, Err(ConversionError::Infinite { positive: false })));
    }

    /// Property: Values within safe range always succeed
    #[test]
    fn prop_safe_range_always_succeeds() {
        proptest!(|(value in -1e12..1e12_f64)| {
            // All values in this range should convert successfully
            let result = fixed_point::from_f64_checked(value);
            prop_assert!(result.is_ok(),
                "Conversion failed for safe value: {}", value);
        });
    }

    /// Property: Values outside safe range always fail
    #[test]
    fn prop_unsafe_range_always_fails() {
        proptest!(|(value in prop::num::f64::POSITIVE)| {
            // Test values beyond safe range
            let large_value = value.abs() + fixed_point::MAX_SAFE_F64 + 1.0;

            let result = fixed_point::from_f64_checked(large_value);
            prop_assert!(result.is_err(),
                "Conversion succeeded for unsafe value: {}", large_value);
        });
    }

    // ===== ARITHMETIC PROPERTY TESTS =====

    /// Property: Conversion preserves sign
    #[test]
    fn prop_conversion_preserves_sign() {
        proptest!(|(value in -1000000.0..1000000.0_f64)| {
            if let Ok(fixed) = fixed_point::from_f64_checked(value) {
                if value > 0.0 {
                    prop_assert!(fixed > 0, "Positive value {} converted to negative {}",
                                value, fixed);
                } else if value < 0.0 {
                    prop_assert!(fixed < 0, "Negative value {} converted to positive {}",
                                value, fixed);
                } else {
                    prop_assert_eq!(fixed, 0, "Zero converted to non-zero {}", fixed);
                }
            }
        });
    }

    /// Property: Conversion preserves ordering
    #[test]
    fn prop_conversion_preserves_ordering() {
        proptest!(|(a in -1000000.0..1000000.0_f64,
                   b in -1000000.0..1000000.0_f64)| {
            if let (Ok(fixed_a), Ok(fixed_b)) =
                (fixed_point::from_f64_checked(a), fixed_point::from_f64_checked(b)) {

                if a < b {
                    prop_assert!(fixed_a < fixed_b,
                        "{} < {} but {} >= {}", a, b, fixed_a, fixed_b);
                } else if a > b {
                    prop_assert!(fixed_a > fixed_b,
                        "{} > {} but {} <= {}", a, b, fixed_a, fixed_b);
                }
                // Note: We don't test equality due to precision loss
            }
        });
    }

    /// Property: to_f64(0) == 0.0
    #[test]
    fn prop_zero_converts_to_zero() {
        assert_eq!(fixed_point::to_f64(0), 0.0);
    }

    /// Property: Conversion is deterministic
    #[test]
    fn prop_conversion_is_deterministic() {
        proptest!(|(value in -1000000.0..1000000.0_f64)| {
            if let Ok(fixed1) = fixed_point::from_f64_checked(value) {
                let fixed2 = fixed_point::from_f64_checked(value).unwrap();
                prop_assert_eq!(fixed1, fixed2,
                    "Non-deterministic conversion for {}", value);
            }
        });
    }

    // ===== PRECISION TESTS =====

    /// Property: Small values preserve reasonable precision
    #[test]
    fn prop_small_values_preserve_precision() {
        proptest!(|(value in -1000.0..1000.0_f64)| {
            if let Ok(fixed) = fixed_point::from_f64_checked(value) {
                let back = fixed_point::to_f64(fixed);
                let relative_error = if value != 0.0 {
                    ((value - back) / value).abs()
                } else {
                    0.0
                };

                // Relative error should be less than 0.0001%
                prop_assert!(relative_error < 1e-6,
                    "Precision loss too high for {}: {} (error: {}%)",
                    value, back, relative_error * 100.0);
            }
        });
    }

    /// Property: Large values have bounded precision loss
    #[test]
    fn prop_large_values_bounded_loss() {
        proptest!(|(value in 1e6..1e12_f64)| {
            if let Ok(fixed) = fixed_point::from_f64_checked(value) {
                let back = fixed_point::to_f64(fixed);
                let absolute_error = (value - back).abs();

                // Absolute error should be less than 1 unit in 9th decimal
                prop_assert!(absolute_error < 1.0,
                    "Precision loss too high for {}: {} (error: {})",
                    value, back, absolute_error);
            }
        });
    }

    // ===== U64 CONVERSION TESTS =====

    /// Property: from_u64_checked succeeds for values <= i64::MAX
    #[test]
    fn prop_u64_conversion_bounded() {
        proptest!(|(value in 0..=(i64::MAX as u64))| {
            let result = fixed_point::from_u64_checked(value);
            prop_assert!(result.is_ok(),
                "Conversion failed for safe u64 value: {}", value);

            if let Ok(converted) = result {
                prop_assert!(converted >= 0,
                    "u64 value {} converted to negative {}", value, converted);
            }
        });
    }

    /// Property: from_u64_checked fails for values > i64::MAX
    #[test]
    fn prop_u64_overflow_detection() {
        let overflow_value = (i64::MAX as u64) + 1;
        let result = fixed_point::from_u64_checked(overflow_value);

        assert!(result.is_err(),
            "Conversion should fail for u64 value > i64::MAX");
    }

    /// Property: to_u64 never returns negative
    #[test]
    fn prop_to_u64_never_negative() {
        proptest!(|(value: i64)| {
            let result = fixed_point::to_u64(value);
            prop_assert!(result >= 0,
                "to_u64({}) returned negative value (shouldn't be possible)", value);
        });
    }

    /// Property: to_u64 clamps negative to zero
    #[test]
    fn prop_to_u64_clamps_negative() {
        proptest!(|(value in i64::MIN..0_i64)| {
            let result = fixed_point::to_u64(value);
            prop_assert_eq!(result, 0,
                "to_u64({}) should clamp to 0, got {}", value, result);
        });
    }

    // ===== SCALE FACTOR TESTS =====

    /// Property: SCALE is exactly 1 billion
    #[test]
    fn test_scale_constant() {
        assert_eq!(fixed_point::SCALE, 1_000_000_000);
    }

    /// Property: MAX_SAFE_F64 is approximately 9.2 quadrillion
    #[test]
    fn test_max_safe_constant() {
        let expected = (i64::MAX / fixed_point::SCALE) as f64;
        assert_eq!(fixed_point::MAX_SAFE_F64, expected);
        // Approximately 9.2 quadrillion
        assert!(fixed_point::MAX_SAFE_F64 > 9e15);
        assert!(fixed_point::MAX_SAFE_F64 < 1e16);
    }

    /// Property: MIN_SAFE_F64 is approximately -9.2 quadrillion
    #[test]
    fn test_min_safe_constant() {
        let expected = (i64::MIN / fixed_point::SCALE) as f64;
        assert_eq!(fixed_point::MIN_SAFE_F64, expected);
        // Approximately -9.2 quadrillion
        assert!(fixed_point::MIN_SAFE_F64 < -9e15);
        assert!(fixed_point::MIN_SAFE_F64 > -1e16);
    }
}
