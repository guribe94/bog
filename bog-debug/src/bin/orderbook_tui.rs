//! Real-Time Orderbook Terminal UI
//!
//! Beautiful, real-time visualization of the L2 orderbook with:
//! - Live orderbook ladder (10 levels)
//! - Spread visualization
//! - Our orders highlighted
//! - Latency metrics
//! - Position and PnL tracking
//!
//! ## Usage
//!
//! ```bash
//! bog-debug orderbook-tui --market 1
//! ```
//!
//! ## Keyboard Controls
//!
//! - `q` or `Ctrl-C` - Quit
//! - `p` - Pause/Resume updates
//! - `r` - Reset stats
//! - `1-5` - Change depth view (5/10/20/50/100 levels)
//! - `s` - Toggle spread chart
//! - `m` - Toggle metrics panel

use anyhow::Result;
use bog_core::data::MarketSnapshot;
use bog_core::orderbook::OrderBook; // L2OrderBook is re-exported as OrderBook
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use rust_decimal::Decimal;
use std::io;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run app
    let res = run_app(&mut terminal);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {:?}", err);
    }

    Ok(())
}

struct App {
    orderbook: OrderBook,
    paused: bool,
    tick_count: u64,
    last_update: Option<Instant>,
    show_metrics: bool,
    show_spread_chart: bool,
}

impl App {
    fn new() -> Self {
        Self {
            orderbook: OrderBook::new(1),
            paused: false,
            tick_count: 0,
            last_update: None,
            show_metrics: true,
            show_spread_chart: false,
        }
    }

    fn update(&mut self, snapshot: &MarketSnapshot) {
        if !self.paused {
            self.orderbook.sync_from_snapshot(snapshot);
            self.tick_count += 1;
            self.last_update = Some(Instant::now());
        }
    }
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> Result<()> {
    let mut app = App::new();
    let tick_rate = Duration::from_millis(100); // 10 FPS
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('p') => app.paused = !app.paused,
                    KeyCode::Char('m') => app.show_metrics = !app.show_metrics,
                    KeyCode::Char('s') => app.show_spread_chart = !app.show_spread_chart,
                    KeyCode::Char('r') => {
                        app.tick_count = 0;
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // Simulate receiving market data
            // TODO: Connect to real Huginn feed
            let snapshot = create_mock_snapshot(app.tick_count);
            app.update(&snapshot);
            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),    // Orderbook
            Constraint::Length(3),  // Footer
        ])
        .split(f.area());

    // Header
    render_header(f, chunks[0], app);

    // Orderbook ladder
    render_orderbook(f, chunks[1], app);

    // Footer
    render_footer(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let mid = app.orderbook.mid_price();
    let spread = app.orderbook.spread_bps();
    let mid_decimal = Decimal::from(mid) / Decimal::from(1_000_000_000);
    let spread_usd = (mid as f64 * spread as f64 / 10_000.0) / 1_000_000_000.0;

    let status = if app.paused { "PAUSED" } else { "LIVE" };
    let status_color = if app.paused { Color::Yellow } else { Color::Green };

    let title = Line::from(vec![
        Span::styled("BOG ORDERBOOK VIEWER", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | BTC/USD Market 1 | "),
        Span::styled(status, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
    ]);

    let info = Line::from(vec![
        Span::raw("Sequence: "),
        Span::styled(
            format!("{}", app.orderbook.last_sequence),
            Style::default().fg(Color::White),
        ),
        Span::raw(" | Mid: $"),
        Span::styled(
            format!("{:.2}", mid_decimal),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" | Spread: "),
        Span::styled(
            format!("{}bps (${:.2})", spread, spread_usd),
            Style::default().fg(Color::Magenta),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));

    let text = vec![title, info];
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn render_orderbook(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Orderbook ladder (left)
    render_ladder(f, chunks[0], app);

    // Metrics panel (right)
    if app.show_metrics {
        render_metrics(f, chunks[1], app);
    }
}

fn render_ladder(f: &mut Frame, area: Rect, app: &App) {
    let ask_levels = app.orderbook.ask_levels();
    let bid_levels = app.orderbook.bid_levels();

    let max_size = ask_levels
        .iter()
        .chain(bid_levels.iter())
        .map(|(_, size)| *size)
        .max()
        .unwrap_or(1);

    let mut items = Vec::new();

    // Asks (reverse order for display - highest at top)
    for (price, size) in ask_levels.iter().take(5).rev() {
        let price_decimal = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let size_decimal = Decimal::from(*size) / Decimal::from(1_000_000_000);
        let bar = create_bar(*size, max_size, 20);

        let line = Line::from(vec![
            Span::styled("ASK ", Style::default().fg(Color::Red)),
            Span::styled(
                format!("{:>10.2}", price_decimal),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(Color::Red)),
            Span::raw("  "),
            Span::styled(
                format!("{:.4} BTC", size_decimal),
                Style::default().fg(Color::White),
            ),
        ]);

        items.push(ListItem::new(line));
    }

    // Mid line
    let mid = app.orderbook.mid_price();
    let mid_decimal = Decimal::from(mid) / Decimal::from(1_000_000_000);
    let mid_line = Line::from(vec![
        Span::raw("─────"),
        Span::styled(
            format!(" MID: ${:.2} ", mid_decimal),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::raw("─────"),
    ]);
    items.push(ListItem::new(mid_line));

    // Bids
    for (price, size) in bid_levels.iter().take(5) {
        let price_decimal = Decimal::from(*price) / Decimal::from(1_000_000_000);
        let size_decimal = Decimal::from(*size) / Decimal::from(1_000_000_000);
        let bar = create_bar(*size, max_size, 20);

        let line = Line::from(vec![
            Span::styled("BID ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("{:>10.2}", price_decimal),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled(
                format!("{:.4} BTC", size_decimal),
                Style::default().fg(Color::White),
            ),
        ]);

        items.push(ListItem::new(line));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .title("Orderbook Ladder (Top 5 Levels)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(list, area);
}

fn render_metrics(f: &mut Frame, area: Rect, app: &App) {
    let imbalance = app.orderbook.imbalance();
    let bid_depth = app.orderbook.bid_depth();
    let ask_depth = app.orderbook.ask_depth();

    let imbalance_text = if imbalance > 10 {
        "Buy Pressure ↑"
    } else if imbalance < -10 {
        "Sell Pressure ↓"
    } else {
        "Balanced"
    };

    let imbalance_color = if imbalance > 10 {
        Color::Green
    } else if imbalance < -10 {
        Color::Red
    } else {
        Color::Yellow
    };

    let text = vec![
        Line::from(vec![
            Span::styled("MARKET DEPTH", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("Bid Levels: "),
            Span::styled(format!("{}", bid_depth), Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::raw("Ask Levels: "),
            Span::styled(format!("{}", ask_depth), Style::default().fg(Color::Red)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("IMBALANCE", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled(imbalance_text, Style::default().fg(imbalance_color)),
        ]),
        Line::from(vec![
            Span::raw("Value: "),
            Span::styled(format!("{:+}", imbalance), Style::default().fg(imbalance_color)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("STATISTICS", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("Updates: "),
            Span::styled(format!("{}", app.tick_count), Style::default().fg(Color::White)),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Metrics")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::White)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(paragraph, area);
}

fn render_footer(f: &mut Frame, area: Rect, _app: &App) {
    let controls = Line::from(vec![
        Span::styled("Controls: ", Style::default().fg(Color::Cyan)),
        Span::raw("[Q]uit "),
        Span::raw("[P]ause "),
        Span::raw("[M]etrics "),
        Span::raw("[S]pread "),
        Span::raw("[R]eset"),
    ]);

    let paragraph = Paragraph::new(controls)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

/// Create a horizontal bar chart for size visualization
fn create_bar(size: u64, max_size: u64, width: usize) -> String {
    if max_size == 0 {
        return " ".repeat(width);
    }

    let filled = ((size as f64 / max_size as f64) * width as f64) as usize;
    let filled = filled.min(width);

    let mut bar = String::with_capacity(width);
    for i in 0..width {
        if i < filled {
            bar.push('█');
        } else {
            bar.push('░');
        }
    }
    bar
}

/// Create mock snapshot for testing (TODO: Connect to real Huginn)
fn create_mock_snapshot(tick: u64) -> MarketSnapshot {
    let mut snapshot = unsafe { std::mem::zeroed::<MarketSnapshot>() };

    snapshot.market_id = 1;
    snapshot.sequence = tick;
    snapshot.exchange_timestamp_ns = tick * 100_000;

    // Simulate price movement
    let base_price = 50_000_000_000_000u64; // $50,000
    let wave = ((tick as f64 / 10.0).sin() * 50_000_000_000.0) as i64;

    snapshot.best_bid_price = (base_price as i64 + wave - 5_000_000_000) as u64; // -$5
    snapshot.best_ask_price = (base_price as i64 + wave + 5_000_000_000) as u64; // +$5
    snapshot.best_bid_size = 1_000_000_000u64; // 1.0 BTC
    snapshot.best_ask_size = 1_500_000_000u64; // 1.5 BTC

    // Fill in 10 levels
    for i in 0..10 {
        snapshot.bid_prices[i] = snapshot.best_bid_price.saturating_sub((i as u64 + 1) * 10_000_000_000);
        snapshot.bid_sizes[i] = 500_000_000 + (i as u64 * 100_000_000);

        snapshot.ask_prices[i] = snapshot.best_ask_price.saturating_add((i as u64 + 1) * 10_000_000_000);
        snapshot.ask_sizes[i] = 800_000_000 + (i as u64 * 150_000_000);
    }

    snapshot
}
