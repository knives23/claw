use crate::market_data::MarketData;
use anyhow::Result;
use futures_util::StreamExt;
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::Arc;
use tokio_tungstenite::connect_async;
use tracing::{info, warn, error};

pub struct BinanceFeed {
    ws_url: String,
    market_data: Arc<MarketData>,
}

#[derive(Debug, Deserialize)]
struct BinanceTrade {
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
}

impl BinanceFeed {
    pub fn new(ws_url: String, market_data: Arc<MarketData>) -> Self {
        Self { ws_url, market_data }
    }
    
    pub async fn run(&self) -> Result<()> {
        loop {
            info!("Connecting to Binance WebSocket: {}", self.ws_url);
            
            match self.connect_and_stream().await {
                Ok(_) => warn!("WebSocket closed, reconnecting..."),
                Err(e) => {
                    error!("WebSocket error: {}, reconnecting in 5s...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }
    
    async fn connect_and_stream(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.ws_url).await?;
        info!("Connected to Binance WebSocket");
        
        let (_, mut read) = ws_stream.split();
        
        while let Some(msg) = read.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    self.handle_message(&text).await;
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    warn!("Received close frame");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
        
        Ok(())
    }
    
    async fn handle_message(&self, text: &str) {
        if let Ok(trade) = serde_json::from_str::<BinanceTrade>(text) {
            if let Ok(price) = trade.price.parse::<Decimal>() {
                let volume = trade.quantity.parse::<Decimal>().ok();
                self.market_data.update_spot(price, volume).await;
            }
        }
    }
}
