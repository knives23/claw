use crate::config::Config;
use crate::market_data::MarketData;
use crate::signals::{Signal, SignalGenerator};
use crate::paper_trading::{PaperTrading, PaperTradingStats, PaperPosition};
use crate::polymarket_data::{PolymarketData, MarketMetrics};
use chrono::Utc;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    symbols,
    widgets::{
        Axis, Block, Borders, Chart, Dataset, Paragraph, Tabs,
    },
    Frame,
};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::sync::Arc;
use std::collections::VecDeque;

pub struct App {
    pub config: Config,
    pub market_data: Arc<MarketData>,
    pub signal_generator: SignalGenerator,
    pub paper_trading: Arc<PaperTrading>,
    pub polymarket_data: Arc<PolymarketData>,
    pub state: AppState,
    pub current_tab: usize,
    pub price_history: Vec<(f64, f64)>,
    pub signal_history: Vec<(f64, f64)>,
    pub pnl_history: Vec<(f64, f64)>,
    pub log_buffer: VecDeque<String>,
    pub last_signal: Option<Signal>,
    pub current_price: Option<Decimal>,
    pub stats: PaperTradingStats,
    pub open_positions: Vec<PaperPosition>,
    pub pm_metrics: MarketMetrics,
    pub data_points_received: usize,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppState {
    Running,
    Paused,
}

impl App {
    pub fn new(
        config: Config,
        market_data: Arc<MarketData>,
        signal_generator: SignalGenerator,
        paper_trading: Arc<PaperTrading>,
        polymarket_data: Arc<PolymarketData>,
    ) -> Self {
        let mut app = Self {
            config,
            market_data,
            signal_generator,
            paper_trading,
            polymarket_data,
            state: AppState::Running,
            current_tab: 0,
            price_history: Vec::with_capacity(100),
            signal_history: Vec::with_capacity(100),
            pnl_history: Vec::with_capacity(100),
            log_buffer: VecDeque::with_capacity(100),
            last_signal: None,
            current_price: None,
            stats: PaperTradingStats {
                total_trades: 0,
                winning_trades: 0,
                losing_trades: 0,
                win_rate: 0.0,
                total_pnl: Decimal::ZERO,
                avg_trade_pnl: Decimal::ZERO,
                max_drawdown: Decimal::ZERO,
                current_equity: Decimal::from(1000),
                initial_equity: Decimal::from(1000),
                open_positions: 0,
            },
            open_positions: Vec::new(),
            pm_metrics: MarketMetrics::default(),
            data_points_received: 0,
            running: true,
        };
        app.add_log("Bot started - waiting for data...".to_string());
        app
    }
    
    pub async fn update(&mut self) {
        if self.state != AppState::Running {
            return;
        }
        
        // Update Polymarket metrics
        self.pm_metrics = self.polymarket_data.get_metrics().await;
        
        // Get current price
        if let Some(spot) = self.market_data.current_spot().await {
            self.data_points_received += 1;
            self.current_price = Some(spot.price);
            let now = Utc::now().timestamp() as f64;
            
            // Update price history
            self.price_history.push((self.data_points_received as f64, spot.price.to_f64().unwrap_or(0.0)));
            if self.price_history.len() > 100 {
                self.price_history.remove(0);
            }
            
            // Generate signal
            let signal = self.signal_generator.generate(&self.market_data).await;
            let prev_signal = self.last_signal;
            
            let signal_val = match signal {
                Signal::Up { confidence } => confidence,
                Signal::Down { confidence } => -confidence,
                Signal::Hold => 0.0,
            };
            
            // Always record signal for chart
            self.signal_history.push((self.data_points_received as f64, signal_val));
            if self.signal_history.len() > 100 {
                self.signal_history.remove(0);
            }
            
            // Log signal changes
            if let Some(prev) = prev_signal {
                if std::mem::discriminant(&prev) != std::mem::discriminant(&signal) {
                    self.last_signal = Some(signal);
                    if !matches!(signal, Signal::Hold) {
                        self.paper_trading.execute_signal(signal, spot.price).await;
                        self.add_log(format!("📊 SIGNAL: {:?} @ ${:.2}", signal, spot.price));
                    } else {
                        self.add_log(format!("⏸️  HOLD @ ${:.2}", spot.price));
                    }
                }
            } else {
                self.last_signal = Some(signal);
            }
            
            // Update paper trading positions
            self.paper_trading.execute_signal(Signal::Hold, spot.price).await;
            
            // Update stats
            self.stats = self.paper_trading.get_stats().await;
            self.open_positions = self.paper_trading.get_open_positions().await;
            
            // Update PnL history
            let pnl_val = self.stats.total_pnl.to_f64().unwrap_or(0.0);
            self.pnl_history.push((self.data_points_received as f64, pnl_val));
            if self.pnl_history.len() > 100 {
                self.pnl_history.remove(0);
            }
            
            // Log every 10th price update
            if self.data_points_received % 10 == 0 {
                self.add_log(format!("💰 BTC: ${:.2} | Signal: {:.2}", spot.price, signal_val));
            }
        }
    }
    
    pub fn add_log(&mut self, msg: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        self.log_buffer.push_back(format!("[{}] {}", timestamp, msg));
        if self.log_buffer.len() > 100 {
            self.log_buffer.pop_front();
        }
    }
    
    pub fn on_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => self.running = false,
            KeyCode::Char(' ') => {
                self.state = match self.state {
                    AppState::Running => AppState::Paused,
                    AppState::Paused => AppState::Running,
                };
            }
            KeyCode::Tab => {
                self.current_tab = (self.current_tab + 1) % 5;
            }
            _ => {}
        }
    }
}

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.area());

    let titles = vec!["Charts", "Polymarket", "Positions", "Trades", "Logs"];
    let tabs = Tabs::new(titles)
        .select(app.current_tab)
        .block(Block::default().title("Polymarket BTC Paper Trader").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    match app.current_tab {
        0 => draw_charts_tab(f, app, chunks[1]),
        1 => draw_polymarket_tab(f, app, chunks[1]),
        2 => draw_positions_tab(f, app, chunks[1]),
        3 => draw_trades_tab(f, app, chunks[1]),
        4 => draw_logs_tab(f, app, chunks[1]),
        _ => draw_charts_tab(f, app, chunks[1]),
    }

    let current_price_str = app.current_price.map(|p| format!("${:.2}", p)).unwrap_or_else(|| "Loading...".to_string());
    let status = format!(
        "Status: {} | BTC: {} | Points: {} | PM YES: {:.2} | Time: {}",
        if app.state == AppState::Running { "▶" } else { "⏸" },
        current_price_str,
        app.data_points_received,
        app.pm_metrics.yes_price,
        app.pm_metrics.time_remaining
    );
    let controls = Paragraph::new(format!("{} | [Q]uit | [Space] Pause | [Tab] Switch", status))
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(controls, chunks[2]);
}

fn draw_polymarket_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(14), Constraint::Min(0)])
        .split(area);

    let pm = &app.pm_metrics;
    
    let market_text = format!(
        "📊 {}\n\n\
        💰 YES Price:  {:.3} ({:.1}%)\n\
        💰 NO Price:   {:.3} ({:.1}%)\n\
        📏 Spread:     {:.3}\n\
        📊 24h Volume: ${:.2}\n\
        💧 Liquidity:  ${:.2}\n\
        ⏰ Time Left:  {}\n\
        🔄 Updated:    {}\n\
        Status:       {}",
        pm.question,
        pm.yes_price,
        (pm.yes_price * Decimal::from(100)).to_f64().unwrap_or(0.0),
        pm.no_price,
        (pm.no_price * Decimal::from(100)).to_f64().unwrap_or(0.0),
        pm.spread,
        pm.volume_24h,
        pm.liquidity,
        pm.time_remaining,
        pm.last_update.format("%H:%M:%S"),
        if app.data_points_received > 0 { "✅ Connected" } else { "⏳ Loading..." }
    );
    
    let market_widget = Paragraph::new(market_text)
        .block(Block::default().title("Polymarket BTC 15-Min Market").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(market_widget, chunks[0]);

    let arb_text = if let Some(btc_price) = app.current_price {
        let pm_implied = pm.yes_price.to_f64().unwrap_or(0.5);
        let our_signal = app.last_signal.map(|s| match s {
            Signal::Up { confidence } => confidence,
            Signal::Down { confidence } => -confidence,
            Signal::Hold => 0.0,
        }).unwrap_or(0.0);
        
        format!(
            "📈 Comparison:\n\n\
            Our Signal:      {}\n\
            PM YES Price:    {:.1}% (implied up probability)\n\
            Difference:      {:.1}%\n\n\
            💡 Strategy:\n\
            If our signal shows UP but PM YES < 50% = Buy YES (undervalued)\n\
            If our signal shows DOWN but PM YES > 50% = Buy NO (overvalued)",
            if our_signal > 0.3 { format!("🟢 UP {:.0}%", our_signal * 100.0) }
            else if our_signal < -0.3 { format!("🔴 DOWN {:.0}%", our_signal.abs() * 100.0) }
            else { "⚪ NEUTRAL".to_string() },
            pm_implied * 100.0,
            (our_signal - (pm_implied - 0.5) * 2.0).abs() * 100.0
        )
    } else {
        "Waiting for BTC price data...\n\nThe bot is fetching data from Coinbase API.\nThis may take a few seconds.".to_string()
    };
    
    let arb_widget = Paragraph::new(arb_text)
        .block(Block::default().title("Signal vs Market").borders(Borders::ALL))
        .style(Style::default().fg(Color::Cyan));
    f.render_widget(arb_widget, chunks[1]);
}

fn draw_charts_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(area);

    // Price chart
    let price_data: Vec<(f64, f64)> = app.price_history.clone();
    
    if !price_data.is_empty() {
        let price_dataset = Dataset::default()
            .name("BTC")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Yellow))
            .data(&price_data);

        let min_price = price_data.iter().map(|(_, p)| *p).fold(f64::INFINITY, f64::min);
        let max_price = price_data.iter().map(|(_, p)| *p).fold(0.0f64, f64::max);
        let (y_min, y_max) = if min_price == f64::INFINITY {
            (90000.0, 100000.0)
        } else {
            (min_price * 0.999, max_price * 1.001)
        };
        
        let price_chart = Chart::new(vec![price_dataset])
            .block(Block::default().title(format!("BTC Price (${:.2})", app.current_price.unwrap_or(Decimal::ZERO))).borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0_f64.max(app.data_points_received as f64 - 100.0), app.data_points_received as f64 + 1.0]))
            .y_axis(Axis::default().bounds([y_min, y_max]));
        f.render_widget(price_chart, chunks[0]);
    } else {
        let loading = Paragraph::new("Loading price data...\n\nFetching from Coinbase API")
            .block(Block::default().title("BTC Price").borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(loading, chunks[0]);
    }

    // Signal chart
    let signal_data: Vec<(f64, f64)> = app.signal_history.clone();
    
    if !signal_data.is_empty() {
        let signal_dataset = Dataset::default()
            .name("Signal")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Cyan))
            .data(&signal_data);

        let signal_chart = Chart::new(vec![signal_dataset])
            .block(Block::default().title("Signal Strength (-1=Down, +1=Up)").borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0_f64.max(app.data_points_received as f64 - 100.0), app.data_points_received as f64 + 1.0]))
            .y_axis(Axis::default().bounds([-1.0, 1.0]));
        f.render_widget(signal_chart, chunks[1]);
    } else {
        let loading = Paragraph::new("Waiting for signals...")
            .block(Block::default().title("Signals").borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(loading, chunks[1]);
    }

    // PnL chart
    let pnl_data: Vec<(f64, f64)> = app.pnl_history.clone();
    
    if !pnl_data.is_empty() {
        let pnl_dataset = Dataset::default()
            .name("PnL")
            .marker(symbols::Marker::Braille)
            .style(Style::default().fg(Color::Green))
            .data(&pnl_data);

        let min_pnl = pnl_data.iter().map(|(_, p)| *p).fold(0.0f64, f64::min);
        let max_pnl = pnl_data.iter().map(|(_, p)| *p).fold(0.0f64, f64::max);
        
        let pnl_chart = Chart::new(vec![pnl_dataset])
            .block(Block::default().title(format!("P&L (${:.2})", app.stats.total_pnl)).borders(Borders::ALL))
            .x_axis(Axis::default().bounds([0.0_f64.max(app.data_points_received as f64 - 100.0), app.data_points_received as f64 + 1.0]))
            .y_axis(Axis::default().bounds([min_pnl - 0.5, max_pnl + 0.5]));
        f.render_widget(pnl_chart, chunks[2]);
    } else {
        let loading = Paragraph::new("No trades yet")
            .block(Block::default().title("P&L").borders(Borders::ALL))
            .alignment(Alignment::Center);
        f.render_widget(loading, chunks[2]);
    }
}

fn draw_positions_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(area);

    let stats_text = format!(
        "💵 Initial: ${:.2}  |  Current: ${:.2}\n\
        📈 Total P&L: ${:.2} ({:.1}%)\n\
        🎯 Win Rate: {:.1}% ({}/{} trades)\n\
        📊 Avg Trade: ${:.2}\n\
        📝 Open Positions: {} | Data Points: {}",
        app.stats.initial_equity,
        app.stats.current_equity,
        app.stats.total_pnl,
        (app.stats.total_pnl / app.stats.initial_equity * Decimal::from(100)).to_f64().unwrap_or(0.0),
        app.stats.win_rate * 100.0,
        app.stats.winning_trades,
        app.stats.total_trades,
        app.stats.avg_trade_pnl,
        app.open_positions.len(),
        app.data_points_received
    );
    
    let stats = Paragraph::new(stats_text)
        .block(Block::default().title("Trading Statistics").borders(Borders::ALL));
    f.render_widget(stats, chunks[0]);

    let mut pos_text = String::new();
    if app.open_positions.is_empty() {
        pos_text.push_str("No open positions\n\nWaiting for signals...");
    } else {
        for pos in &app.open_positions {
            let pnl_str = format!("${:.2}", pos.unrealized_pnl);
            let emoji = if pos.unrealized_pnl >= Decimal::ZERO { "🟢" } else { "🔴" };
            let side_str = match pos.side {
                crate::risk::PositionSide::Yes => "YES",
                crate::risk::PositionSide::No => "NO ",
            };
            pos_text.push_str(&format!(
                "{} {} | Size: ${} | Entry: ${:.2} | PnL: {} {}\n",
                emoji, side_str, pos.size, pos.entry_price, emoji, pnl_str
            ));
        }
    }
    
    let positions_widget = Paragraph::new(pos_text)
        .block(Block::default().title("Open Positions").borders(Borders::ALL));
    f.render_widget(positions_widget, chunks[1]);
}

fn draw_trades_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut trade_text = String::new();
    
    for log in app.log_buffer.iter().rev().take(30) {
        if log.contains("SIGNAL") || log.contains("position") {
            trade_text.push_str(log);
            trade_text.push('\n');
        }
    }
    
    if trade_text.is_empty() {
        trade_text = "No trades yet.\n\nWaiting for trading signals...\n\nSignals are generated based on BTC momentum and market conditions.".to_string();
    }
    
    let trades_widget = Paragraph::new(trade_text)
        .block(Block::default().title("Recent Trades & Signals").borders(Borders::ALL));
    f.render_widget(trades_widget, area);
}

fn draw_logs_tab(f: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let logs: String = app.log_buffer.iter().rev().take(30).cloned().collect::<Vec<_>>().join("\n");
    let logs_widget = Paragraph::new(logs)
        .block(Block::default().title("Event Log").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(logs_widget, area);
}
