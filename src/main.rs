mod config;
mod market_data;
mod signals;
mod risk;
mod exchange;
mod price_feed;
mod ui;
mod polymarket_data;
mod paper_trading;

use std::sync::Arc;
use std::io;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Disable console logging - prevents text appearing over TUI
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::sink)
        .try_init();
    
    run_tui().await
}

async fn run_tui() -> anyhow::Result<()> {
    use crossterm::{
        event::{self, Event, KeyCode},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{
        backend::CrosstermBackend,
        Terminal,
    };
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Load config
    let config = config::Config {
        api_key: "demo".to_string(),
        api_secret: "demo".to_string(),
        passphrase: "demo".to_string(),
        private_key: "0x".to_string(),
        chain_id: 137,
        market_slug: "bitcoin-up-down-15min".to_string(),
        trade_size_usdc: 10.0,
        max_daily_loss: 100.0,
        max_position_size: 100.0,
        prediction_threshold: 0.55,
        use_momentum: true,
        use_orderflow: true,
        stop_loss_pct: 0.05,
        take_profit_pct: 0.10,
        check_interval_secs: 2,
        binance_ws_url: "wss://stream.binance.com:9443/ws/btcusdt@aggTrade".to_string(),
    };
    
    // Initialize components
    let market_data: Arc<market_data::MarketData> = Arc::new(market_data::MarketData::new());
    let risk_manager: Arc<risk::RiskManager> = Arc::new(risk::RiskManager::new(
        rust_decimal::Decimal::from(100),
        rust_decimal::Decimal::from(1000),
    ));
    
    let paper_trading: Arc<paper_trading::PaperTrading> = Arc::new(paper_trading::PaperTrading::new(
        risk_manager.clone(),
        rust_decimal::Decimal::from(1000),
        rust_decimal::Decimal::from(10),
        config.market_slug.clone(),
    ));
    
    let polymarket_data: Arc<polymarket_data::PolymarketData> = Arc::new(polymarket_data::PolymarketData::new());
    
    let signal_generator = signals::SignalGenerator::new(
        config.use_momentum,
        config.use_orderflow,
        config.prediction_threshold,
    );
    
    // Start price feed (Coinbase)
    let md_clone = market_data.clone();
    let _feed_handle = tokio::spawn(async move {
        let feed = price_feed::PriceFeed::new(md_clone);
        if let Err(e) = feed.run().await {
            eprintln!("Price feed error: {}", e);
        }
    });
    
    // Start Polymarket data fetcher
    let pm_clone = polymarket_data.clone();
    let _pm_handle = tokio::spawn(async move {
        if let Err(e) = pm_clone.run().await {
            eprintln!("Polymarket data error: {}", e);
        }
    });
    
    // Create app
    let mut app = ui::App::new(config, market_data, signal_generator, paper_trading, polymarket_data);
    
    // Run TUI loop
    let mut last_update = tokio::time::Instant::now();
    
    loop {
        // Draw UI
        terminal.draw(|f| ui::draw(f, &app))?;
        
        // Handle events
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('q') {
                    app.running = false;
                }
                app.on_key(key);
            }
        }
        
        // Update app state
        if last_update.elapsed().as_secs() >= 1 {
            app.update().await;
            last_update = tokio::time::Instant::now();
        }
        
        if !app.running {
            break;
        }
    }
    
    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    
    Ok(())
}
