use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub private_key: String,
    pub chain_id: u64,
    pub market_slug: String,
    pub trade_size_usdc: f64,
    pub max_daily_loss: f64,
    pub max_position_size: f64,
    pub prediction_threshold: f64,
    pub use_momentum: bool,
    pub use_orderflow: bool,
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
    pub check_interval_secs: u64,
    pub binance_ws_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            api_key: env::var("POLY_API_KEY")?,
            api_secret: env::var("POLY_API_SECRET")?,
            passphrase: env::var("POLY_PASSPHRASE")?,
            private_key: env::var("PRIVATE_KEY")?,
            chain_id: env::var("CHAIN_ID").unwrap_or_else(|_| "137".to_string()).parse()?,
            market_slug: env::var("MARKET_SLUG").unwrap_or_else(|_| "bitcoin-up-down-15min".to_string()),
            trade_size_usdc: env::var("TRADE_SIZE_USDC").unwrap_or_else(|_| "10.0".to_string()).parse()?,
            max_daily_loss: env::var("MAX_DAILY_LOSS").unwrap_or_else(|_| "100.0".to_string()).parse()?,
            max_position_size: env::var("MAX_POSITION_SIZE").unwrap_or_else(|_| "100.0".to_string()).parse()?,
            prediction_threshold: env::var("PREDICTION_THRESHOLD").unwrap_or_else(|_| "0.55".to_string()).parse()?,
            use_momentum: env::var("USE_MOMENTUM").unwrap_or_else(|_| "true".to_string()) == "true",
            use_orderflow: env::var("USE_ORDERFLOW").unwrap_or_else(|_| "true".to_string()) == "true",
            stop_loss_pct: env::var("STOP_LOSS_PCT").unwrap_or_else(|_| "0.05".to_string()).parse()?,
            take_profit_pct: env::var("TAKE_PROFIT_PCT").unwrap_or_else(|_| "0.10".to_string()).parse()?,
            check_interval_secs: env::var("CHECK_INTERVAL_SECS").unwrap_or_else(|_| "10".to_string()).parse()?,
            binance_ws_url: env::var("BINANCE_WS_URL").unwrap_or_else(|_| "wss://stream.binance.com:9443/ws/btcusdt@trade".to_string()),
        })
    }
    
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.trade_size_usdc < 1.0 {
            anyhow::bail!("Minimum trade size is 1 USDC");
        }
        if self.prediction_threshold < 0.5 || self.prediction_threshold > 1.0 {
            anyhow::bail!("Prediction threshold must be between 0.5 and 1.0");
        }
        Ok(())
    }
}
