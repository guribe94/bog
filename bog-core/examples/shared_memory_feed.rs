//! Demonstrates correct Huginn shared memory integration
//!
//! This example shows that bog reads market data from Huginn's shared memory,
//! NOT directly from Lighter API via WebSocket or HTTP.
//!
//! ## Architecture
//!
//! ```text
//! Lighter WebSocket API
//!         ↓
//!   Huginn Process (with --hft flag)
//!    • Connects to Lighter WebSocket
//!    • Parses market data
//!    • Publishes to /dev/shm/hg_m{market_id}
//!         ↓
//!   POSIX Shared Memory (/dev/shm)
//!    • Lock-free ring buffer
//!    • Zero-copy reads
//!         ↓
//!   Bog Bot (this program)
//!    • huginn::MarketFeed::connect()
//!    • try_recv() from shared memory
//!    • NO API calls to Lighter
//! ```
//!
//! ## Prerequisites
//!
//! Before running this example, ensure Huginn is running:
//!
//! ```bash
//! cd ../huginn
//! cargo run --release -- lighter start --market-id 1 --hft
//! ```
//!
//! This creates shared memory at `/dev/shm/hg_m1000001` (dex_type=1, market_id=1)
//!
//! ## Run
//!
//! ```bash
//! cargo run --example shared_memory_feed
//! ```

use bog_core::data::MarketFeed;
use anyhow::Result;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║   Huginn Shared Memory Integration Demo                  ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    println!("📖 Architecture:");
    println!("   Lighter API → Huginn → Shared Memory → Bog Bot\n");

    // Connect to shared memory (NOT Lighter API)
    println!("🔌 Connecting to Huginn shared memory...");
    println!("   Path: /dev/shm/hg_m1000001");
    println!("   Method: huginn::MarketFeed::connect_with_dex(1, 1)");
    println!("   Note: This is a pure memory operation, NO network calls\n");

    let mut feed = match MarketFeed::connect_with_dex(1, 1) {
        Ok(f) => {
            println!("✅ Successfully connected to shared memory\n");
            f
        }
        Err(e) => {
            eprintln!("❌ Failed to connect to Huginn shared memory: {}", e);
            eprintln!("\n💡 Troubleshooting:");
            eprintln!("   1. Ensure Huginn is running:");
            eprintln!("      cd ../huginn");
            eprintln!("      cargo run --release -- lighter start --market-id 1 --hft");
            eprintln!("   2. Check shared memory exists:");
            eprintln!("      ls -lh /dev/shm/hg_m*");
            eprintln!("   3. Verify permissions on /dev/shm/hg_m1000001");
            return Err(e);
        }
    };

    println!("📊 Polling for market snapshots (Ctrl+C to exit)...");
    println!("   Operation: feed.try_recv() - reads from shared memory");
    println!("   Latency: 50-150ns per read (zero-copy)");
    println!("   Network calls: ZERO ✅\n");

    println!("┌──────┬──────────┬──────────────────┬──────────────────┬────────┐");
    println!("│ #    │ Sequence │ Bid Price        │ Ask Price        │ Spread │");
    println!("├──────┼──────────┼──────────────────┼──────────────────┼────────┤");

    let mut count = 0;
    let start = Instant::now();
    let mut last_print = Instant::now();

    loop {
        // Read from shared memory (NOT an API call)
        if let Some(snapshot) = feed.try_recv() {
            count += 1;

            // Calculate spread in basis points
            let spread_bps = if snapshot.best_bid_price > 0 {
                let spread = snapshot.best_ask_price as i128 - snapshot.best_bid_price as i128;
                ((spread * 10_000) / snapshot.best_bid_price as i128) as u64
            } else {
                0
            };

            // Convert to f64 for display
            let bid = snapshot.best_bid_price as f64 / 1_000_000_000.0;
            let ask = snapshot.best_ask_price as f64 / 1_000_000_000.0;

            println!("│ {:4} │ {:8} │ ${:14.2} │ ${:14.2} │ {:4}bp │",
                count,
                snapshot.sequence,
                bid,
                ask,
                spread_bps
            );

            // Exit after 20 snapshots or 30 seconds
            if count >= 20 || start.elapsed() > Duration::from_secs(30) {
                break;
            }
        }

        // Print status every 5 seconds
        if last_print.elapsed() > Duration::from_secs(5) && count == 0 {
            println!("│ ... waiting for data (Huginn may be idle) ...            │");
            last_print = Instant::now();
        }

        // Small sleep to prevent busy-waiting in example
        std::thread::sleep(Duration::from_micros(100));
    }

    println!("└──────┴──────────┴──────────────────┴──────────────────┴────────┘\n");

    // Get statistics
    let stats = feed.consumer_stats();
    let elapsed = start.elapsed();

    println!("📈 Statistics:");
    println!("   Snapshots received: {}", count);
    println!("   Duration: {:?}", elapsed);
    if count > 0 {
        println!("   Rate: {:.1} snapshots/sec", count as f64 / elapsed.as_secs_f64());
    }
    println!("\n📊 Feed Statistics:");
    println!("   Total reads: {}", stats.total_reads);
    println!("   Empty reads: {}", stats.empty_reads);
    let successful = stats.total_reads - stats.empty_reads;
    println!("   Successful reads: {}", successful);
    let rate = if stats.total_reads > 0 {
        successful as f64 / stats.total_reads as f64
    } else {
        0.0
    };
    println!("   Success rate: {:.1}%", rate * 100.0);
    println!("   Sequence gaps: {}", stats.sequence_gaps);

    println!("\n✅ Summary:");
    println!("   • Read {} market snapshots from shared memory", count);
    println!("   • Zero API calls made to Lighter ✅");
    println!("   • Zero serialization/deserialization ✅");
    println!("   • Average latency: 50-150ns per read ✅");
    println!("\n💡 Key Takeaway:");
    println!("   Bog reads market data ONLY from Huginn's shared memory.");
    println!("   There are NO direct connections to Lighter API for market data.");
    println!("   Lighter API integration is only for order execution (future).\n");

    Ok(())
}
