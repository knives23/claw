use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Position {
    pub market_id: String,
    pub side: PositionSide,
    pub size: Decimal,
    pub entry_price: Decimal,
    pub entry_time: DateTime<Utc>,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum PositionSide {
    Yes,
    No,
}

pub struct RiskManager {
    daily_pnl: RwLock<Decimal>,
    #[allow(dead_code)]
    daily_pnl_start: RwLock<DateTime<Utc>>,
    #[allow(dead_code)]
    positions: RwLock<HashMap<String, Position>>,
    max_daily_loss: Decimal,
    #[allow(dead_code)]
    max_position_size: Decimal,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum RiskCheck {
    Allow,
    Block(String),
    Reduce(String),
}

impl RiskManager {
    pub fn new(max_daily_loss: Decimal, max_position_size: Decimal) -> Self {
        Self {
            daily_pnl: RwLock::new(Decimal::ZERO),
            daily_pnl_start: RwLock::new(Utc::now()),
            positions: RwLock::new(HashMap::new()),
            max_daily_loss,
            max_position_size,
        }
    }
    
    #[allow(dead_code)]
    pub async fn check_trade(&self, _market_id: &str, size: Decimal, _side: PositionSide) -> RiskCheck {
        self.reset_daily_if_needed().await;
        
        let daily_pnl = *self.daily_pnl.read().await;
        if daily_pnl < -self.max_daily_loss {
            return RiskCheck::Block(format!("Daily loss limit reached: {} USDC", daily_pnl));
        }
        
        if size > self.max_position_size {
            return RiskCheck::Reduce(format!(
                "Trade size {} exceeds limit {}", size, self.max_position_size
            ));
        }
        
        RiskCheck::Allow
    }
    
    #[allow(dead_code)]
    pub async fn positions(&self) -> Vec<Position> {
        self.positions.read().await.values().cloned().collect()
    }
    
    pub async fn total_pnl(&self) -> Decimal {
        self.reset_daily_if_needed().await;
        *self.daily_pnl.read().await
    }
    
    #[allow(dead_code)]
    async fn reset_daily_if_needed(&self) {
        let start = *self.daily_pnl_start.read().await;
        let now = Utc::now();
        
        if now.signed_duration_since(start) > Duration::days(1) {
            *self.daily_pnl.write().await = Decimal::ZERO;
            *self.daily_pnl_start.write().await = now;
        }
    }
}
