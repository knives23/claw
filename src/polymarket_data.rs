//! Polymarket market data fetcher
use anyhow::Result;
use chrono::{DateTime, Timelike, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct MarketMetrics {
    pub market_id: String,
    pub question: String,
    pub yes_price: Decimal,
    pub no_price: Decimal,
    pub spread: Decimal,
    pub volume_24h: Decimal,
    pub liquidity: Decimal,
    pub ends_at: Option<DateTime<Utc>>,
    pub time_remaining: String,
    pub last_update: DateTime<Utc>,
}

impl Default for MarketMetrics {
    fn default() -> Self {
        Self {
            market_id: String::new(),
            question: "BTC 15-Min Market (Loading...)".to_string(),
            yes_price: Decimal::from(5) / Decimal::from(10),
            no_price: Decimal::from(5) / Decimal::from(10),
            spread: Decimal::ZERO,
            volume_24h: Decimal::ZERO,
            liquidity: Decimal::ZERO,
            ends_at: None,
            time_remaining: "Loading...".to_string(),
            last_update: Utc::now(),
        }
    }
}

pub struct PolymarketData {
    metrics: tokio::sync::RwLock<MarketMetrics>,
    client: reqwest::Client,
}

impl PolymarketData {
    pub fn new() -> Self {
        Self {
            metrics: tokio::sync::RwLock::new(MarketMetrics::default()),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
    
    pub async fn run(&self) -> Result<()> {
        let mut ticker = interval(Duration::from_secs(5));
        
        info!("Starting Polymarket data fetcher");
        
        // Initial fetch
        let _ = self.fetch_market_data().await;
        
        loop {
            ticker.tick().await;
            
            if let Err(e) = self.fetch_market_data().await {
                warn!("Failed to fetch Polymarket data: {}", e);
            }
        }
    }
    
    async fn fetch_market_data(&self) -> Result<()> {
        // Generate slug based on pattern: btc-updown-15m-<epoch>
        let now = Utc::now().timestamp() as u64;
        let epoch = (now / 900) * 900;
        let slug = format!("btc-updown-15m-{}", epoch);
        
        // Fetch market directly by slug
        let url = format!("https://gamma-api.polymarket.com/markets?slug={}&active=true", slug);
        
        info!("Fetching Polymarket market with slug: {}", slug);
        
        let resp = self.client.get(&url).send().await?;
        let markets: Vec<PolymarketMarket> = resp.json().await?;
        
        if let Some(market) = markets.first() {
            self.update_metrics(market).await;
            return Ok(());
        }
        
        // Try to find market by searching
        warn!("Market not found by slug, trying search...");
        self.fetch_by_search().await
    }
    
    async fn fetch_by_search(&self) -> Result<()> {
        let url = "https://gamma-api.polymarket.com/markets?active=true&tag=BTC&limit=20";
        
        let resp = self.client.get(url).send().await?;
        let markets: Vec<PolymarketMarket> = resp.json().await?;
        
        // Find BTC 15min market
        for market in &markets {
            let desc_lower = market.question.to_lowercase();
            if desc_lower.contains("15") && (desc_lower.contains("up") || desc_lower.contains("down")) {
                self.update_metrics(market).await;
                return Ok(());
            }
        }
        
        warn!("No BTC 15min market found");
        Ok(())
    }
    
    async fn update_metrics(&self, market: &PolymarketMarket) {
        // Parse outcomePrices from JSON string array: "[\"0.795\", \"0.205\"]"
        let (yes_price, no_price) = match parse_outcome_prices(&market.outcome_prices) {
            Some((yes, no)) => (yes, no),
            None => {
                warn!("Failed to parse outcome prices: {}", market.outcome_prices);
                (Decimal::from(5) / Decimal::from(10), Decimal::from(5) / Decimal::from(10))
            }
        };
        
        let ends_at = market.endDateIso.as_ref()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
            .map(|d| d.with_timezone(&Utc));
        
        let time_remaining = get_time_to_next_interval();
        
        let liquidity = market.liquidity.parse::<f64>().unwrap_or(0.0);
        let volume_24h = market.volume24hr.unwrap_or(0.0);
        
        let metrics = MarketMetrics {
            market_id: market.id.clone(),
            question: market.question.clone(),
            yes_price,
            no_price,
            spread: (yes_price + no_price - Decimal::ONE).abs(),
            volume_24h: Decimal::try_from(volume_24h).unwrap_or_default(),
            liquidity: Decimal::try_from(liquidity).unwrap_or_default(),
            ends_at,
            time_remaining,
            last_update: Utc::now(),
        };
        
        let mut guard = self.metrics.write().await;
        *guard = metrics;
        drop(guard);
        
        info!("Polymarket: {} - YES={:.3} NO={:.3}", market.question, yes_price, no_price);
    }
    
    pub async fn get_metrics(&self) -> MarketMetrics {
        self.metrics.read().await.clone()
    }
}

fn get_time_to_next_interval() -> String {
    let now = Utc::now();
    let current_minute = now.minute() as i64;
    let current_second = now.second() as i64;
    
    // Find next 15-min boundary
    let next_boundary = ((current_minute / 15) + 1) * 15;
    let minutes_remaining = next_boundary - current_minute;
    let seconds_remaining = 60 - current_second;
    
    if seconds_remaining == 60 {
        format!("{}m 00s", minutes_remaining)
    } else {
        format!("{}m {:02}s", minutes_remaining - 1, seconds_remaining)
    }
}

fn parse_outcome_prices(prices_str: &str) -> Option<(Decimal, Decimal)> {
    // Parse JSON string array: "[\"0.795\", \"0.205\"]"
    let prices: Vec<String> = serde_json::from_str(prices_str).ok()?;
    if prices.len() >= 2 {
        let yes = prices[0].parse::<f64>().ok()?;
        let no = prices[1].parse::<f64>().ok()?;
        Some((Decimal::try_from(yes).ok()?, Decimal::try_from(no).ok()?))
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct PolymarketMarket {
    id: String,
    question: String,
    #[serde(rename = "outcomePrices")]
    outcome_prices: String, // JSON string: "[\"0.795\", \"0.205\"]"
    volume24hr: Option<f64>,
    liquidity: String,
    endDateIso: Option<String>,
}
