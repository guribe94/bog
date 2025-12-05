//! Orderbook depth constants re-exported from Huginn
//!
//! All depth-related values MUST use these constants, never hardcoded literals.
//!
//! # Production Safety
//!
//! This module ensures compile-time safety for orderbook depth configuration:
//! - All array sizes derived from Huginn's `ORDERBOOK_DEPTH`
//! - Padding size automatically adjusted for 512-byte struct alignment
//! - Compile-time assertions catch unsupported configurations
//!
//! # Example
//!
//! ```rust
//! use bog_core::data::constants::{ORDERBOOK_DEPTH, PADDING_SIZE};
//!
//! // CORRECT: Use the constant
//! let bid_prices: [u64; ORDERBOOK_DEPTH] = [0; ORDERBOOK_DEPTH];
//!
//! // WRONG: Never hardcode!
//! // let bid_prices: [u64; 10] = [0; 10];  // ❌ DON'T DO THIS
//! ```

pub use huginn::shm::{ORDERBOOK_DEPTH, PADDING_SIZE, SNAPSHOT_SIZE};

/// Compile-time assertion: Ensure we're aware of the depth configuration
///
/// This will cause a compile error with a descriptive message if an unsupported
/// depth value is detected.
const _: () = {
    // This match will show the actual depth value in compiler errors
    match ORDERBOOK_DEPTH {
        1 | 2 | 5 | 10 => {}
        _ => panic!("Unsupported ORDERBOOK_DEPTH. Expected 1, 2, 5, or 10."),
    }
};

/// Compile-time assertion: Verify MarketSnapshot is cache-aligned
///
/// The struct size is now optimized based on orderbook depth:
/// - depth-1:  128 bytes (2 cache lines)
/// - depth-2:  192 bytes (3 cache lines)
/// - depth-5:  256 bytes (4 cache lines)
/// - depth-10: 448 bytes (7 cache lines)
const _: () = {
    const ACTUAL_SIZE: usize = core::mem::size_of::<huginn::shm::MarketSnapshot>();

    // Verify cache-line alignment (must be multiple of 64 bytes)
    if ACTUAL_SIZE % 64 != 0 {
        panic!("MarketSnapshot must be cache-line aligned (multiple of 64 bytes)!");
    }

    // Verify reasonable size range (128-512 bytes)
    if ACTUAL_SIZE < 128 || ACTUAL_SIZE > 512 {
        panic!("MarketSnapshot size out of reasonable range (128-512 bytes)!");
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_depth_constants_from_huginn() {
        // Document what depth we're compiled with
        println!("ORDERBOOK_DEPTH: {}", ORDERBOOK_DEPTH);
        println!("PADDING_SIZE: {}", PADDING_SIZE);

        // Verify supported depth value
        assert!(
            matches!(ORDERBOOK_DEPTH, 1 | 2 | 5 | 10),
            "ORDERBOOK_DEPTH must be 1, 2, 5, or 10"
        );
    }

    #[test]
    fn verify_market_snapshot_size() {
        use huginn::shm::SNAPSHOT_SIZE;

        let actual_size = std::mem::size_of::<huginn::shm::MarketSnapshot>();

        // Verify struct matches expected optimized size
        assert_eq!(
            actual_size, SNAPSHOT_SIZE,
            "MarketSnapshot size must match SNAPSHOT_SIZE"
        );

        // Verify cache-line alignment
        assert_eq!(
            actual_size % 64,
            0,
            "MarketSnapshot must be cache-line aligned"
        );

        println!(
            "MarketSnapshot: {} bytes ({} cache lines)",
            actual_size,
            actual_size / 64
        );
    }

    #[test]
    fn verify_padding_calculation() {
        use huginn::shm::SNAPSHOT_SIZE;

        let snapshot_size = std::mem::size_of::<huginn::shm::MarketSnapshot>();

        // Calculate expected padding based on depth
        let seqlock_fields = 16; // generation_start + generation_end (2 x u64)
        let hot_data = 72; // 9 u64 fields
        let depth_arrays_size = ORDERBOOK_DEPTH * 32; // 4 arrays of u64
        let flags_size = 2; // snapshot_flags + dex_type

        let data_size = seqlock_fields + hot_data + depth_arrays_size + flags_size;
        let expected_padding = SNAPSHOT_SIZE - data_size;

        println!("Depth: {}", ORDERBOOK_DEPTH);
        println!("SeqLock fields: {} bytes", seqlock_fields);
        println!("Data size: {} bytes", data_size);
        println!("Expected padding: {} bytes", expected_padding);
        println!("Actual padding: {} bytes", PADDING_SIZE);
        println!("Total size: {} bytes", snapshot_size);
        println!(
            "Waste: {:.1}%",
            (PADDING_SIZE as f64 / snapshot_size as f64) * 100.0
        );

        assert_eq!(
            PADDING_SIZE, expected_padding,
            "Padding calculation mismatch"
        );
    }
}
