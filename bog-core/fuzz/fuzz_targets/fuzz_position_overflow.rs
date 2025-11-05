//! Fuzz target for Position overflow detection
//!
//! This fuzzer tests Position arithmetic operations with extreme values,
//! verifying that overflow checks work correctly.

#![no_main]

use libfuzzer_sys::fuzz_target;
use bog_core::core::Position;

fuzz_target!(|data: &[u8]| {
    // Need at least 16 bytes for two i64 values
    if data.len() >= 16 {
        // Extract two i64 values from the input
        let initial = i64::from_le_bytes([
            data[0], data[1], data[2], data[3],
            data[4], data[5], data[6], data[7],
        ]);

        let delta = i64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);

        // Create position with initial value
        let position = Position::new();

        // Set initial quantity using saturating (to get to any starting point)
        let _ = position.update_quantity_saturating(initial);

        // Test checked update
        let checked_result = position.update_quantity_checked(delta);

        // Verify consistency
        match checked_result {
            Ok(new_qty) => {
                // If check succeeded, verify the math
                if let Some(expected) = initial.checked_add(delta) {
                    assert_eq!(new_qty, expected,
                        "Checked update succeeded but math is wrong: {} + {} = {} (expected {})",
                        initial, delta, new_qty, expected);
                } else {
                    panic!("Checked update succeeded but should have overflowed: {} + {}",
                           initial, delta);
                }
            }
            Err(_) => {
                // If check failed, verify overflow would occur
                assert!(initial.checked_add(delta).is_none(),
                    "Checked update failed but {} + {} should succeed", initial, delta);
            }
        }

        // Test saturating update (should never panic)
        let saturated = position.update_quantity_saturating(delta);
        let expected_saturated = initial.saturating_add(delta);
        assert_eq!(saturated, expected_saturated,
            "Saturating update incorrect: {} + {} = {} (expected {})",
            initial, delta, saturated, expected_saturated);

        // Test PnL operations with smaller values (more likely to be realistic)
        if data.len() >= 24 {
            let pnl_delta = i64::from_le_bytes([
                data[16], data[17], data[18], data[19],
                data[20], data[21], data[22], data[23],
            ]);

            // Test realized PnL
            let _ = position.update_realized_pnl_checked(pnl_delta);
            let _ = position.update_realized_pnl_saturating(pnl_delta);

            // Test daily PnL
            let _ = position.update_daily_pnl_checked(pnl_delta);
            let _ = position.update_daily_pnl_saturating(pnl_delta);
        }
    }
});
