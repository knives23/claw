use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::VecDeque;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct PricePoint {
    #[allow(dead_code)]
    pub timestamp: DateTime<Utc>,
    pub price: Decimal,
    #[allow(dead_code)]
    pub source: PriceSource,
    #[allow(dead_code)]
    pub volume: Option<Decimal>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum PriceSource {
    Binance,
    PolymarketMid,
    PolymarketLast,
}

pub struct MarketData {
    price_history: RwLock<VecDeque<PricePoint>>,
    current_spot: RwLock<Option<PricePoint>>,
    polymarket_yes_price: RwLock<Option<Decimal>>,
    polymarket_no_price: RwLock<Option<Decimal>>,
}

impl MarketData {
    pub fn new() -> Self {
        Self {
            price_history: RwLock::new(VecDeque::with_capacity(1000)),
            current_spot: RwLock::new(None),
            polymarket_yes_price: RwLock::new(None),
            polymarket_no_price: RwLock::new(None),
        }
    }
    
    pub async fn update_spot(&self, price: Decimal, volume: Option<Decimal>) {
        let point = PricePoint {
            timestamp: Utc::now(),
            price,
            source: PriceSource::Binance,
            volume,
        };
        
        let mut history = self.price_history.write().await;
        if history.len() >= 1000 {
            history.pop_front();
        }
        history.push_back(point.clone());
        drop(history);
        
        *self.current_spot.write().await = Some(point);
    }
    
    #[allow(dead_code)]
    pub async fn update_polymarket_prices(&self, yes_price: Decimal, no_price: Decimal) {
        *self.polymarket_yes_price.write().await = Some(yes_price);
        *self.polymarket_no_price.write().await = Some(no_price);
    }
    
    pub async fn current_spot(&self) -> Option<PricePoint> {
        self.current_spot.read().await.clone()
    }
    
    pub async fn implied_probability(&self) -> Option<f64> {
        let yes = *self.polymarket_yes_price.read().await;
        let no = *self.polymarket_no_price.read().await;
        let yes = yes?;
        let no = no?;
        let sum = yes + no;
        if sum > Decimal::ZERO {
            Some((yes / sum).to_f64().unwrap_or(0.5))
        } else {
            None
        }
    }
    
    pub async fn momentum(&self, periods: usize) -> Option<f64> {
        let history = self.price_history.read().await;
        if history.len() < periods {
            return None;
        }
        
        let recent: Vec<_> = history.iter().rev().take(periods).collect();
        let current = recent.first()?.price;
        let past = recent.last()?.price;
        
        if past > Decimal::ZERO {
            Some(((current - past) / past).to_f64().unwrap_or(0.0))
        } else {
            None
        }
    }
    
    pub async fn volatility(&self, periods: usize) -> Option<f64> {
        let history = self.price_history.read().await;
        if history.len() < periods + 1 {
            return None;
        }
        
        let prices: Vec<_> = history.iter().rev().take(periods + 1).map(|p| p.price).collect();
        let returns: Vec<f64> = prices.windows(2)
            .map(|w| {
                if w[1] > Decimal::ZERO {
                    ((w[0] - w[1]) / w[1]).to_f64().unwrap_or(0.0)
                } else {
                    0.0
                }
            })
            .collect();
        
        if returns.is_empty() {
            return None;
        }
        
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        Some(variance.sqrt())
    }
}

impl Default for MarketData {
    fn default() -> Self {
        Self::new()
    }
}
