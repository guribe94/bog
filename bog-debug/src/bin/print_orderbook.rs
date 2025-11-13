//! Orderbook Snapshot Printer
//!
//! Simple CLI tool to print orderbook state to terminal.
//! Useful for debugging, logging, and CI/CD pipelines.
//!
//! ## Usage
//!
//! ```bash
//! # Print top 5 levels
//! bog-debug print-orderbook --market 1
//!
//! # Print top 10 levels
//! bog-debug print-orderbook --market 1 --levels 10
//!
//! # JSON output
//! bog-debug print-orderbook --market 1 --format json
//! ```

use anyhow::Result;
use bog_core::data::MarketSnapshot;
use bog_core::orderbook::OrderBook; // L2OrderBook is re-exported as OrderBook
use clap::Parser;
use rust_decimal::Decimal;
use serde_json::json;

#[derive(Parser)]
#[command(name = "print-orderbook")]
#[command(about = "Print orderbook snapshot", long_about = None)]
struct Args {
    /// Market ID to monitor
    #[arg(short, long, default_value = "1")]
    market: u64,

    /// Number of levels to display
    #[arg(short, long, default_value = "5")]
    levels: usize,

    /// Output format (pretty, compact, json)
    #[arg(short, long, default_value = "pretty")]
    format: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // TODO: Connect to real Huginn feed
    // For now, create a mock snapshot
    let snapshot = create_mock_snapshot();
    let mut orderbook = OrderBook::new(args.market);
    orderbook.sync_from_snapshot(&snapshot);

    match args.format.as_str() {
        "json" => print_json(&orderbook, args.levels)?,
        "compact" => print_compact(&orderbook, args.levels)?,
        _ => print_pretty(&orderbook, args.levels)?,
    }

    Ok(())
}

fn print_pretty(book: &OrderBook, max_levels: usize) -> Result<()> {
    let mid = book.mid_price();
    let spread = book.spread_bps();
    let imbalance = book.imbalance();

    let mid_decimal = Decimal::from(mid) / Decimal::from(1_000_000_000);
    let spread_usd = (mid as f64 * spread as f64 / 10_000.0) / 1_000_000_000.0;

    println!();
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║          BTC/USD ORDERBOOK (Market {})                  ║", book.market_id);
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║  Sequence: {}  │  Depth: {}x{}                     ║",
        book.last_sequence, book.bid_depth(), book.ask_depth());
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();

    // Asks (reverse order - highest first)
    println!("         ASKS        SIZE       BAR");
    println!("    ════════════════════════════════════════");
    let ask_levels = book.ask_levels();
    let max_size = ask_levels.iter().map(|(_, s)| *s).max().unwrap_or(1);

    for (price, size) in ask_levels.iter().take(max_levels).rev() {
        let price_dec = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let size_dec = Decimal::from(*size) / Decimal::from(1_000_000_000);
        let bar = create_ascii_bar(*size, max_size, 15);
        println!("    {:>10.2}   {:>6.3}  {}", price_dec, size_dec, bar);
    }

    println!();
    println!("    ─────────────────────────────────────────");
    println!("     MID: ${:.2}  │  Spread: {}bps (${:.2})", mid_decimal, spread, spread_usd);
    println!("    ─────────────────────────────────────────");
    println!();

    // Bids
    println!("         BIDS        SIZE       BAR");
    println!("    ════════════════════════════════════════");
    let bid_levels = book.bid_levels();

    for (price, size) in bid_levels.iter().take(max_levels) {
        let price_dec = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let size_dec = Decimal::from(*size) / Decimal::from(1_000_000_000);
        let bar = create_ascii_bar(*size, max_size, 15);
        println!("    {:>10.2}   {:>6.3}  {}", price_dec, size_dec, bar);
    }

    println!();
    println!("Imbalance: {:+3} {}", imbalance, imbalance_description(imbalance));
    println!();

    Ok(())
}

fn print_compact(book: &OrderBook, max_levels: usize) -> Result<()> {
    let mid = Decimal::from(book.mid_price()) / Decimal::from(1_000_000_000);
    let spread = book.spread_bps();

    println!("BTC/USD  Mid: ${:.2}  Spread: {}bps  Depth: {}x{}",
        mid, spread, book.bid_depth(), book.ask_depth());

    let bid_levels = book.bid_levels();
    let ask_levels = book.ask_levels();

    for (price, size) in ask_levels.iter().take(max_levels).rev() {
        let p = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let s = Decimal::from(*size) / Decimal::from(1_000_000_000);
        println!("ASK  {:>10.2}  {:>6.3}", p, s);
    }

    println!("MID  {:>10.2}", mid);

    for (price, size) in bid_levels.iter().take(max_levels) {
        let p = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let s = Decimal::from(*size) / Decimal::from(1_000_000_000);
        println!("BID  {:>10.2}  {:>6.3}", p, s);
    }

    Ok(())
}

fn print_json(book: &OrderBook, max_levels: usize) -> Result<()> {
    let bid_levels: Vec<_> = book
        .bid_levels()
        .iter()
        .take(max_levels)
        .map(|(p, s)| {
            json!({
                "price": Decimal::from(*p) / Decimal::from(1_000_000_000),
                "size": Decimal::from(*s) / Decimal::from(1_000_000_000),
            })
        })
        .collect();

    let ask_levels: Vec<_> = book
        .ask_levels()
        .iter()
        .take(max_levels)
        .map(|(p, s)| {
            json!({
                "price": Decimal::from(*p) / Decimal::from(1_000_000_000),
                "size": Decimal::from(*s) / Decimal::from(1_000_000_000),
            })
        })
        .collect();

    let output = json!({
        "market_id": book.market_id,
        "sequence": book.last_sequence,
        "mid_price": Decimal::from(book.mid_price()) / Decimal::from(1_000_000_000),
        "spread_bps": book.spread_bps(),
        "imbalance": book.imbalance(),
        "bids": bid_levels,
        "asks": ask_levels,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn create_ascii_bar(size: u64, max_size: u64, width: usize) -> String {
    if max_size == 0 {
        return " ".repeat(width);
    }

    let filled = ((size as f64 / max_size as f64) * width as f64) as usize;
    let filled = filled.min(width);

    "█".repeat(filled) + &"░".repeat(width - filled)
}

fn imbalance_description(imbalance: i64) -> &'static str {
    match imbalance {
        i if i > 30 => "(Strong buy pressure)",
        i if i > 10 => "(Buy pressure)",
        i if i < -30 => "(Strong sell pressure)",
        i if i < -10 => "(Sell pressure)",
        _ => "(Balanced)",
    }
}

/// Create mock snapshot for testing
fn create_mock_snapshot() -> MarketSnapshot {
    let mut snapshot = unsafe { std::mem::zeroed::<MarketSnapshot>() };

    snapshot.market_id = 1;
    snapshot.sequence = 12345;
    snapshot.exchange_timestamp_ns = 1000000000;

    snapshot.best_bid_price = 50_000_000_000_000u64;
    snapshot.best_ask_price = 50_010_000_000_000u64;
    snapshot.best_bid_size = 1_234_000_000u64;
    snapshot.best_ask_size = 987_000_000u64;

    for i in 0..10 {
        snapshot.bid_prices[i] = 50_000_000_000_000u64 - ((i as u64 + 1) * 10_000_000_000);
        snapshot.bid_sizes[i] = 500_000_000 + (i as u64 * 123_000_000);

        snapshot.ask_prices[i] = 50_010_000_000_000u64 + ((i as u64 + 1) * 10_000_000_000);
        snapshot.ask_sizes[i] = 300_000_000 + (i as u64 * 87_000_000);
    }

    snapshot
}
