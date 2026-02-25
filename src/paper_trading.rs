//! Paper trading module - simulates trades without real money
use crate::risk::{PositionSide, RiskManager};
use crate::signals::Signal;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Debug, Clone)]
pub struct PaperPosition {
    pub id: String,
    pub market_id: String,
    pub side: PositionSide,
    pub size: Decimal,
    pub entry_price: Decimal,
    pub entry_time: DateTime<Utc>,
    pub exit_price: Option<Decimal>,
    pub exit_time: Option<DateTime<Utc>>,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub status: PositionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PositionStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub timestamp: DateTime<Utc>,
    pub market_id: String,
    pub side: PositionSide,
    pub size: Decimal,
    pub price: Decimal,
    pub pnl: Decimal,
    pub signal_confidence: f64,
}

#[derive(Debug, Clone)]
pub struct PaperTradingStats {
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub total_pnl: Decimal,
    pub avg_trade_pnl: Decimal,
    pub max_drawdown: Decimal,
    pub current_equity: Decimal,
    pub initial_equity: Decimal,
    pub open_positions: usize,
}

pub struct PaperTrading {
    _risk_manager: Arc<RiskManager>,
    positions: RwLock<HashMap<String, PaperPosition>>,
    trade_history: RwLock<Vec<TradeRecord>>,
    equity: RwLock<Decimal>,
    initial_equity: Decimal,
    trade_size: Decimal,
    market_id: String,
}

impl PaperTrading {
    pub fn new(
        _risk_manager: Arc<RiskManager>,
        initial_equity: Decimal,
        trade_size: Decimal,
        market_id: String,
    ) -> Self {
        Self {
            _risk_manager,
            positions: RwLock::new(HashMap::new()),
            trade_history: RwLock::new(Vec::new()),
            equity: RwLock::new(initial_equity),
            initial_equity,
            trade_size,
            market_id,
        }
    }
    
    pub async fn execute_signal(&self, signal: Signal, current_price: Decimal) {
        match signal {
            Signal::Up { confidence } => {
                self.open_position(PositionSide::Yes, current_price, confidence).await;
            }
            Signal::Down { confidence } => {
                self.open_position(PositionSide::No, current_price, confidence).await;
            }
            Signal::Hold => {
                self.check_exits(current_price).await;
            }
        }
    }
    
    async fn open_position(&self, side: PositionSide, price: Decimal, confidence: f64) {
        let positions = self.positions.read().await;
        
        // Check if we already have a position in this direction
        let has_position = positions.values().any(|p| p.side == side && p.status == PositionStatus::Open);
        
        if has_position {
            return;
        }
        
        drop(positions);
        
        // Close opposite positions first
        self.close_opposite_positions(side, price).await;
        
        // Open new position
        let position = PaperPosition {
            id: format!("pos_{}", Utc::now().timestamp()),
            market_id: self.market_id.clone(),
            side,
            size: self.trade_size,
            entry_price: price,
            entry_time: Utc::now(),
            exit_price: None,
            exit_time: None,
            unrealized_pnl: Decimal::ZERO,
            realized_pnl: Decimal::ZERO,
            status: PositionStatus::Open,
        };
        
        info!("Paper position opened: {:?} @ ${} (conf: {:.0}%)", side, price, confidence * 100.0);
        
        let mut positions = self.positions.write().await;
        positions.insert(position.id.clone(), position);
    }
    
    async fn close_opposite_positions(&self, side: PositionSide, current_price: Decimal) {
        let opposite = match side {
            PositionSide::Yes => PositionSide::No,
            PositionSide::No => PositionSide::Yes,
        };
        
        let mut positions = self.positions.write().await;
        let mut to_close: Vec<(String, Decimal, PositionSide)> = Vec::new();
        
        for (id, pos) in positions.iter() {
            if pos.side == opposite && pos.status == PositionStatus::Open {
                let pnl = if pos.side == PositionSide::Yes {
                    (current_price - pos.entry_price) * pos.size / pos.entry_price
                } else {
                    (pos.entry_price - current_price) * pos.size / pos.entry_price
                };
                to_close.push((id.clone(), pnl, pos.side));
            }
        }
        
        for (id, pnl, side) in to_close {
            if let Some(pos) = positions.get_mut(&id) {
                pos.status = PositionStatus::Closed;
                pos.exit_price = Some(current_price);
                pos.exit_time = Some(Utc::now());
                pos.realized_pnl = pnl;
                
                let mut equity = self.equity.write().await;
                *equity += pnl;
                
                info!("Paper position closed: PnL = ${:.2}", pnl);
            }
            
            // Record trade
            let mut history = self.trade_history.write().await;
            history.push(TradeRecord {
                timestamp: Utc::now(),
                market_id: self.market_id.clone(),
                side,
                size: self.trade_size,
                price: current_price,
                pnl,
                signal_confidence: 0.0,
            });
        }
    }
    
    async fn check_exits(&self, current_price: Decimal) {
        let mut positions = self.positions.write().await;
        let mut to_close: Vec<(String, Decimal)> = Vec::new();
        
        for (id, pos) in positions.iter_mut() {
            if pos.status != PositionStatus::Open {
                continue;
            }
            
            // Update unrealized PnL
            pos.unrealized_pnl = if pos.side == PositionSide::Yes {
                (current_price - pos.entry_price) * pos.size / pos.entry_price
            } else {
                (pos.entry_price - current_price) * pos.size / pos.entry_price
            };
            
            // Check stop loss / take profit
            let pnl_pct = pos.unrealized_pnl / pos.size;
            
            if pnl_pct < Decimal::from(-2) || pnl_pct > Decimal::from(4) {
                to_close.push((id.clone(), pos.unrealized_pnl));
            }
        }
        
        for (id, pnl) in to_close {
            if let Some(pos) = positions.get_mut(&id) {
                let side = pos.side;
                pos.status = PositionStatus::Closed;
                pos.exit_price = Some(current_price);
                pos.exit_time = Some(Utc::now());
                pos.realized_pnl = pnl;
                
                let mut equity = self.equity.write().await;
                *equity += pnl;
                
                info!("Paper position exited (SL/TP): PnL = ${:.2}", pnl);
                
                // Record trade
                let mut history = self.trade_history.write().await;
                history.push(TradeRecord {
                    timestamp: Utc::now(),
                    market_id: self.market_id.clone(),
                    side,
                    size: self.trade_size,
                    price: current_price,
                    pnl,
                    signal_confidence: 0.0,
                });
            }
        }
    }
    
    pub async fn get_stats(&self) -> PaperTradingStats {
        let positions = self.positions.read().await;
        let history = self.trade_history.read().await;
        let equity = *self.equity.read().await;
        
        let open_positions = positions.values().filter(|p| p.status == PositionStatus::Open).count();
        let total_trades = history.len();
        let winning_trades = history.iter().filter(|t| t.pnl > Decimal::ZERO).count();
        let losing_trades = total_trades - winning_trades;
        
        let total_pnl: Decimal = history.iter().map(|t| t.pnl).sum();
        let avg_trade_pnl = if total_trades > 0 {
            total_pnl / Decimal::from(total_trades as i64)
        } else {
            Decimal::ZERO
        };
        
        PaperTradingStats {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate: if total_trades > 0 { winning_trades as f64 / total_trades as f64 } else { 0.0 },
            total_pnl,
            avg_trade_pnl,
            max_drawdown: Decimal::ZERO,
            current_equity: equity,
            initial_equity: self.initial_equity,
            open_positions,
        }
    }
    
    pub async fn get_open_positions(&self) -> Vec<PaperPosition> {
        let positions = self.positions.read().await;
        positions.values().filter(|p| p.status == PositionStatus::Open).cloned().collect()
    }
    
    pub async fn get_recent_trades(&self, n: usize) -> Vec<TradeRecord> {
        let history = self.trade_history.read().await;
        history.iter().rev().take(n).cloned().collect()
    }
}
