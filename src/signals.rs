use crate::market_data::MarketData;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    Up { confidence: f64 },
    Down { confidence: f64 },
    Hold,
}

pub struct SignalGenerator {
    use_momentum: bool,
    use_orderflow: bool,
    threshold: f64,
}

impl SignalGenerator {
    pub fn new(use_momentum: bool, use_orderflow: bool, threshold: f64) -> Self {
        Self {
            use_momentum,
            use_orderflow,
            threshold,
        }
    }
    
    pub async fn generate(&self, data: &MarketData) -> Signal {
        // Get price history - need at least 2 points
        let spot = match data.current_spot().await {
            Some(s) => s,
            None => return Signal::Hold,
        };
        
        // Calculate momentum with shorter windows for faster signals
        let momentum_short = data.momentum(3).await.unwrap_or(0.0);  // Very short
        let momentum_med = data.momentum(10).await.unwrap_or(0.0);    // Short
        
        // Get implied probability from Polymarket if available
        let implied_prob = data.implied_probability().await.unwrap_or(0.5);
        
        // Calculate signal strength
        let mut up_score = 0.0;
        let mut down_score = 0.0;
        
        if self.use_momentum {
            // Stronger weight on recent momentum
            if momentum_short > 0.0001 {
                up_score += momentum_short * 1000.0;
            } else if momentum_short < -0.0001 {
                down_score += momentum_short.abs() * 1000.0;
            }
            
            if momentum_med > 0.0005 {
                up_score += momentum_med * 500.0;
            } else if momentum_med < -0.0005 {
                down_score += momentum_med.abs() * 500.0;
            }
        }
        
        // Order flow - fade extremes
        if self.use_orderflow {
            if implied_prob > 0.65 {
                down_score += (implied_prob - 0.5) * 2.0;
            } else if implied_prob < 0.35 {
                up_score += (0.5 - implied_prob) * 2.0;
            } else if implied_prob > 0.55 {
                up_score += (implied_prob - 0.5) * 1.5;
            } else if implied_prob < 0.45 {
                down_score += (0.5 - implied_prob) * 1.5;
            }
        }
        
        // Calculate final confidence (0.0 to 1.0)
        let raw_confidence = (up_score - down_score).abs();
        let confidence = raw_confidence.min(1.0);
        
        // Lower threshold for more frequent signals
        if confidence < self.threshold {
            return Signal::Hold;
        }
        
        if up_score > down_score {
            Signal::Up { confidence }
        } else {
            Signal::Down { confidence }
        }
    }
}
