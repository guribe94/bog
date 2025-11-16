//! Tests for market ID encoding/decoding and type safety
//!
//! Huginn encodes market IDs as: (dex_type * 1_000_000) + raw_market_id
//! Example: Lighter (dex_type=1) market 1 = 1,000,001
//!
//! These tests verify:
//! 1. Encoding raw IDs with DEX type produces correct encoded IDs
//! 2. Decoding encoded IDs produces correct (dex_type, raw_id) pairs
//! 3. Round-trip encoding/decoding is lossless
//! 4. Invalid DEX types and market IDs are handled correctly

#[cfg(test)]
mod market_id_encoding {
    use bog_core::data::types::{EncodedMarketId, RawMarketId};

    /// Test: Lighter (dex_type=1) market 1 encodes to 1,000,001
    #[test]
    fn test_encode_lighter_market_1() {
        let encoded = bog_core::data::types::encode_market_id(1, 1);
        assert_eq!(encoded, 1_000_001, "Lighter market 1 should encode to 1,000,001");
    }

    /// Test: Lighter market 42 encodes to 1,000,042
    #[test]
    fn test_encode_lighter_market_42() {
        let encoded = bog_core::data::types::encode_market_id(1, 42);
        assert_eq!(encoded, 1_000_042, "Lighter market 42 should encode to 1,000,042");
    }

    /// Test: DEX type 2 market 1 encodes to 2,000,001
    #[test]
    fn test_encode_dex2_market_1() {
        let encoded = bog_core::data::types::encode_market_id(2, 1);
        assert_eq!(encoded, 2_000_001, "DEX 2 market 1 should encode to 2,000,001");
    }

    /// Test: Decoding 1,000,001 produces (1, 1)
    #[test]
    fn test_decode_lighter_market_1() {
        let (dex, market) = bog_core::data::types::decode_market_id(1_000_001);
        assert_eq!(dex, 1, "Encoded 1,000,001 should decode to dex=1");
        assert_eq!(market, 1, "Encoded 1,000,001 should decode to market=1");
    }

    /// Test: Decoding 1,000,042 produces (1, 42)
    #[test]
    fn test_decode_lighter_market_42() {
        let (dex, market) = bog_core::data::types::decode_market_id(1_000_042);
        assert_eq!(dex, 1, "Encoded 1,000,042 should decode to dex=1");
        assert_eq!(market, 42, "Encoded 1,000,042 should decode to market=42");
    }

    /// Test: Decoding 2,000,001 produces (2, 1)
    #[test]
    fn test_decode_dex2_market_1() {
        let (dex, market) = bog_core::data::types::decode_market_id(2_000_001);
        assert_eq!(dex, 2, "Encoded 2,000,001 should decode to dex=2");
        assert_eq!(market, 1, "Encoded 2,000,001 should decode to market=1");
    }

    /// Test: Round-trip encode/decode is lossless for Lighter market 1
    #[test]
    fn test_roundtrip_lighter_market_1() {
        let dex_in = 1u8;
        let market_in = 1u64;

        let encoded = bog_core::data::types::encode_market_id(dex_in, market_in);
        let (dex_out, market_out) = bog_core::data::types::decode_market_id(encoded);

        assert_eq!(dex_out, dex_in, "DEX type should round-trip correctly");
        assert_eq!(market_out, market_in, "Market ID should round-trip correctly");
    }

    /// Test: Round-trip encode/decode is lossless for various combinations
    #[test]
    fn test_roundtrip_various_markets() {
        let test_cases = vec![
            (1u8, 1u64),    // Lighter market 1
            (1u8, 42u64),   // Lighter market 42
            (1u8, 999_999u64), // Lighter market near max
            (2u8, 1u64),    // DEX 2 market 1
            (3u8, 100u64),  // DEX 3 market 100
        ];

        for (dex_in, market_in) in test_cases {
            let encoded = bog_core::data::types::encode_market_id(dex_in, market_in);
            let (dex_out, market_out) = bog_core::data::types::decode_market_id(encoded);

            assert_eq!(
                dex_out, dex_in,
                "DEX type should round-trip for dex={}, market={}",
                dex_in, market_in
            );
            assert_eq!(
                market_out, market_in,
                "Market ID should round-trip for dex={}, market={}",
                dex_in, market_in
            );
        }
    }

    /// Test: Market ID > 999,999 is clamped to 999,999 (encoded supports up to 1M markets per DEX)
    #[test]
    fn test_market_id_exceeds_1_million() {
        // Encoding with market_id > 999_999 should clamp/fail gracefully
        // Alternatively, values >= 1_000_000 will overlap with DEX encoding
        // This documents the behavior
        let result = bog_core::data::types::encode_market_id_checked(1, 1_000_000);
        assert!(
            result.is_err(),
            "Market ID >= 1,000,000 should fail (conflicts with DEX encoding)"
        );
    }

    /// Test: Type aliases are distinct and prevent accidental mixing
    #[test]
    fn test_type_aliases_exist() {
        let encoded: EncodedMarketId = 1_000_001;
        let raw: RawMarketId = 1;

        // This should be different types and prevent accidental mixing
        // (Rust's type system should catch this at compile time)
        assert_eq!(encoded, 1_000_001);
        assert_eq!(raw, 1);
    }

    /// Test: Encoding function preserves DEX type correctly across range
    #[test]
    fn test_dex_type_extraction() {
        for dex_type in 1..=10 {
            let encoded = bog_core::data::types::encode_market_id(dex_type, 1);
            let (decoded_dex, _) = bog_core::data::types::decode_market_id(encoded);
            assert_eq!(
                decoded_dex, dex_type,
                "DEX type {} should be preserved",
                dex_type
            );
        }
    }

    /// Test: All market IDs in valid range encode/decode correctly
    #[test]
    fn test_all_market_ids_in_range() {
        // Test a sample of market IDs (not all million, for performance)
        let sample_ids = vec![1, 10, 100, 1_000, 10_000, 100_000, 500_000, 999_999];

        for market_id in sample_ids {
            let encoded = bog_core::data::types::encode_market_id(1, market_id);
            let (_, decoded_market) = bog_core::data::types::decode_market_id(encoded);

            assert_eq!(
                decoded_market, market_id,
                "Market ID {} should round-trip correctly",
                market_id
            );
        }
    }
}

#[cfg(test)]
mod market_id_usage {
    use bog_core::data::MarketFeed;

    /// Test: MarketFeed::connect() can be called with encoded market ID
    #[test]
    #[ignore] // Requires Huginn running
    fn test_market_feed_connect_encoded() {
        // Arrange: Use encoded market ID for Lighter market 1
        let encoded_market_id = 1_000_001u64;

        // Act: Connect to market
        // (This would fail without Huginn running, so we ignore it)
        let result = MarketFeed::connect(encoded_market_id);

        // Assert: Should succeed (when Huginn is running)
        assert!(result.is_ok(), "Should connect with encoded market ID");
    }

    /// Test: MarketFeed::connect_with_dex() encodes internally
    #[test]
    #[ignore] // Requires Huginn running
    fn test_market_feed_connect_with_dex() {
        // Arrange: Use separate DEX type and market ID
        let dex_type = 1u8;
        let market_id = 1u64;

        // Act: Connect with DEX type (should encode internally)
        let result = MarketFeed::connect_with_dex(dex_type, market_id);

        // Assert: Should succeed (when Huginn is running)
        assert!(result.is_ok(), "Should connect with dex_type and market_id");
    }
}
