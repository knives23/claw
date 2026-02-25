//! Price feed with Coinbase fallback
use crate::market_data::MarketData;
use anyhow::Result;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{info, debug};

pub struct PriceFeed {
    market_data: Arc<MarketData>,
}

impl PriceFeed {
    pub fn new(market_data: Arc<MarketData>) -> Self {
        Self { market_data }
    }
    
    pub async fn run(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let mut ticker = interval(Duration::from_secs(3)); // Faster updates
        
        info!("Starting Coinbase HTTP price feed");
        
        loop {
            ticker.tick().await;
            
            // Try Coinbase API first
            match self.fetch_coinbase(&client).await {
                Ok(price) => {
                    debug!("BTC price: ${}", price);
                    self.market_data.update_spot(price, None).await;
                }
                Err(e) => {
                    // Fallback to alternative API
                    if let Ok(price) = self.fetch_alternative(&client).await {
                        self.market_data.update_spot(price, None).await;
                    } else {
                        info!("Price fetch error: {}", e);
                    }
                }
            }
        }
    }
    
    async fn fetch_coinbase(&self, client: &reqwest::Client) -> Result<Decimal> {
        let resp = client
            .get("https://api.coinbase.com/v2/exchange-rates?currency=BTC")
            .send()
            .await?;
        
        let data: CoinbaseResponse = resp.json().await?;
        let price = data.data.rates.usd.parse::<Decimal>()?;
        Ok(price)
    }
    
    async fn fetch_alternative(&self, client: &reqwest::Client) -> Result<Decimal> {
        // Alternative: use coinapi or other free API
        let resp = client
            .get("https://api.coincap.io/v2/assets/bitcoin")
            .send()
            .await?;
        
        let data: CoincapResponse = resp.json().await?;
        let price = data.data.price_usd.parse::<Decimal>()?;
        Ok(price)
    }
}

#[derive(Debug, Deserialize)]
struct CoinbaseResponse {
    data: CoinbaseData,
}

#[derive(Debug, Deserialize)]
struct CoinbaseData {
    rates: CoinbaseRates,
}

#[derive(Debug, Deserialize)]
struct CoinbaseRates {
    #[serde(rename = "USD")]
    usd: String,
}

#[derive(Debug, Deserialize)]
struct CoincapResponse {
    data: CoincapData,
}

#[derive(Debug, Deserialize)]
struct CoincapData {
    #[serde(rename = "priceUsd")]
    price_usd: String,
}
