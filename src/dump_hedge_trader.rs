use crate::api::PolymarketApi;
use crate::monitor::MarketSnapshot;
use anyhow::Result;
use log::warn;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::{VecDeque, HashMap};

pub struct DumpHedgeTrader {
    api: Arc<PolymarketApi>,
    simulation_mode: bool,
    shares: f64,
    sum_target: f64,
    move_threshold: f64,
    window_minutes: u64,
    /// Seconds to look back for "old" price when detecting a dump. Default 3.
    dump_lookback_seconds: u64,
    /// Trigger stop loss when remaining time until market close is less than this many minutes.
    stop_loss_last_remaining_minutes: u64,
    stop_loss_percentage: f64,
    /// "buy_opposite" or "sell_position"
    stop_loss_method: String,
    market_states: Arc<Mutex<HashMap<String, MarketState>>>,
    trades: Arc<Mutex<HashMap<String, CycleTrade>>>,
    total_profit: Arc<Mutex<f64>>,
    period_profit: Arc<Mutex<f64>>,
}

#[derive(Debug, Clone)]
enum TradingPhase {
    /// Waiting for dump
    WatchingForDump {
        round_start_time: u64,
        window_end_time: u64,
    },
    /// Leg 1 executed, waiting for hedge opportunity
    WaitingForHedge {
        leg1_side: String,
        leg1_token_id: String,
        leg1_entry_price: f64,
        leg1_shares: f64,
        leg1_timestamp: u64,
    },
    /// Both legs executed, cycle complete
    CycleComplete {
        leg1_side: String,
        leg1_entry_price: f64,
        leg1_shares: f64,
        leg2_side: String,
        leg2_entry_price: f64,
        leg2_shares: f64,
        total_cost: f64,
    },
}

#[derive(Debug, Clone)]
struct MarketState {
    condition_id: String,
    period_timestamp: u64,
    up_token_id: Option<String>,
    down_token_id: Option<String>,
    up_price_history: VecDeque<(u64, f64)>,
    down_price_history: VecDeque<(u64, f64)>,
    phase: TradingPhase,
    closure_checked: bool,
}

#[derive(Debug, Clone)]
struct CycleTrade {
    condition_id: String,
    period_timestamp: u64,
    up_token_id: Option<String>,
    down_token_id: Option<String>,
    up_shares: f64,
    down_shares: f64,
    up_avg_price: f64,
    down_avg_price: f64,
    expected_profit: f64,
}

impl DumpHedgeTrader {
    pub fn new(
        api: Arc<PolymarketApi>,
        simulation_mode: bool,
        shares: f64,
        sum_target: f64,
        move_threshold: f64,
        window_minutes: u64,
        dump_lookback_seconds: u64,
        stop_loss_last_remaining_minutes: u64,
        stop_loss_percentage: f64,
        stop_loss_method: String,
    ) -> Self {
        Self {
            api,
            simulation_mode,
            shares,
            sum_target,
            move_threshold,
            window_minutes,
            dump_lookback_seconds,
            stop_loss_last_remaining_minutes: stop_loss_last_remaining_minutes,
            stop_loss_percentage,
            stop_loss_method: stop_loss_method.to_lowercase(),
            market_states: Arc::new(Mutex::new(HashMap::new())),
            trades: Arc::new(Mutex::new(HashMap::new())),
            total_profit: Arc::new(Mutex::new(0.0)),
            period_profit: Arc::new(Mutex::new(0.0)),
        }
    }

    /// Process market snapshot
    pub async fn process_snapshot(&self, snapshot: &MarketSnapshot) -> Result<()> {
        let market_name = &snapshot.market_name;
        let market_data = &snapshot.btc_market_15m;
        let period_timestamp = snapshot.btc_15m_period_timestamp;
        let _time_remaining = snapshot.btc_15m_time_remaining;
        let condition_id = &market_data.condition_id;
        
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut states = self.market_states.lock().await;
        let should_reset = match states.get(condition_id) {
            Some(s) => s.period_timestamp != period_timestamp,
            None => true,
        };

        if should_reset {
            let round_start_time = period_timestamp;
            let window_end_time = round_start_time + (self.window_minutes * 60);
            
            let phase = if current_time <= window_end_time {
                crate::log_println!(
                    "{}: New round started (period: {}) | Watch window: {} minutes (active)",
                    market_name,
                    period_timestamp,
                    self.window_minutes
                );
                TradingPhase::WatchingForDump {
                    round_start_time,
                    window_end_time,
                }
            } else {
                crate::log_println!(
                    "{}: New round detected (period: {}) | Watch window already passed ({} minutes elapsed)",
                    market_name,
                    period_timestamp,
                    (current_time - period_timestamp) / 60
                );
                TradingPhase::CycleComplete {
                    leg1_side: String::new(),
                    leg1_entry_price: 0.0,
                    leg1_shares: 0.0,
                    leg2_side: String::new(),
                    leg2_entry_price: 0.0,
                    leg2_shares: 0.0,
                    total_cost: 0.0,
                }
            };
            
            states.insert(condition_id.clone(), MarketState {
                condition_id: market_data.condition_id.clone(),
                period_timestamp,
                up_token_id: market_data.up_token.as_ref().map(|t| t.token_id.clone()),
                down_token_id: market_data.down_token.as_ref().map(|t| t.token_id.clone()),
                up_price_history: VecDeque::new(),
                down_price_history: VecDeque::new(),
                phase,
                closure_checked: false,
            });
        }

        let market_state = states.get_mut(condition_id).unwrap();
        
        // Update token IDs
        if let Some(up_token) = &market_data.up_token {
            market_state.up_token_id = Some(up_token.token_id.clone());
        }
        if let Some(down_token) = &market_data.down_token {
            market_state.down_token_id = Some(down_token.token_id.clone());
        }

        let up_ask = market_data.up_token.as_ref()
            .and_then(|t| t.ask_price().to_string().parse::<f64>().ok())
            .unwrap_or(0.0);
        let down_ask = market_data.down_token.as_ref()
            .and_then(|t| t.ask_price().to_string().parse::<f64>().ok())
            .unwrap_or(0.0);

        // Get BID prices for stop loss calculations
        let up_bid = market_data.up_token.as_ref()
            .and_then(|t| t.bid)
            .map(|bid| bid.to_string().parse::<f64>().ok())
            .flatten()
            .unwrap_or(0.0);
        let down_bid = market_data.down_token.as_ref()
            .and_then(|t| t.bid)
            .map(|bid| bid.to_string().parse::<f64>().ok())
            .flatten()
            .unwrap_or(0.0);

        if up_ask <= 0.0 || down_ask <= 0.0 {
            return Ok(());
        }

        market_state.up_price_history.push_back((current_time, up_ask));
        market_state.down_price_history.push_back((current_time, down_ask));
        
        if market_state.up_price_history.len() > 10 {
            market_state.up_price_history.pop_front();
        }
        if market_state.down_price_history.len() > 10 {
            market_state.down_price_history.pop_front();
        }

        match &market_state.phase.clone() {
            TradingPhase::WatchingForDump { window_end_time, .. } => {
                if current_time > *window_end_time {
                    return Ok(());
                }

                if let Some(dump_detected) = self.check_dump(&market_state.up_price_history, current_time) {
                    if dump_detected {
                        crate::log_println!(
                            "{}: UP dump detected! Buying {} shares @ ${:.4}",
                            market_name,
                            self.shares,
                            up_ask
                        );
                        
                        if let Some(token_id) = &market_state.up_token_id {
                            self.execute_buy(
                                market_name,
                                "Up",
                                token_id,
                                self.shares,
                                up_ask,
                            ).await?;
                            
                            // Record trade
                            self.record_trade(
                                &market_state.condition_id,
                                period_timestamp,
                                "Up",
                                token_id,
                                self.shares,
                                up_ask,
                            ).await?;
                            
                            market_state.phase = TradingPhase::WaitingForHedge {
                                leg1_side: "Up".to_string(),
                                leg1_token_id: token_id.clone(),
                                leg1_entry_price: up_ask,
                                leg1_shares: self.shares,
                                leg1_timestamp: current_time,
                            };
                        }
                        return Ok(());
                    }
                }

                if let Some(dump_detected) = self.check_dump(&market_state.down_price_history, current_time) {
                    if dump_detected {
                        crate::log_println!(
                            "{}: DOWN dump detected! Buying {} shares @ ${:.4}",
                            market_name,
                            self.shares,
                            down_ask
                        );
                        
                        if let Some(token_id) = &market_state.down_token_id {
                            self.execute_buy(
                                market_name,
                                "Down",
                                token_id,
                                self.shares,
                                down_ask,
                            ).await?;
                            
                            self.record_trade(
                                &market_state.condition_id,
                                period_timestamp,
                                "Down",
                                token_id,
                                self.shares,
                                down_ask,
                            ).await?;
                            
                            market_state.phase = TradingPhase::WaitingForHedge {
                                leg1_side: "Down".to_string(),
                                leg1_token_id: token_id.clone(),
                                leg1_entry_price: down_ask,
                                leg1_shares: self.shares,
                                leg1_timestamp: current_time,
                            };
                        }
                        return Ok(());
                    }
                }
            }

            TradingPhase::WaitingForHedge { 
                leg1_side, 
                leg1_entry_price, 
                leg1_token_id,
                leg1_shares,
                leg1_timestamp,
            } => {
                let time_remaining_minutes = snapshot.btc_15m_time_remaining / 60;
                let opposite_ask = if leg1_side == "Up" {
                    down_ask
                } else {
                    up_ask
                };
                let opposite_side = if leg1_side == "Up" { "Down" } else { "Up" };
                let opposite_token_id = if leg1_side == "Up" {
                    market_state.down_token_id.as_ref()
                } else {
                    market_state.up_token_id.as_ref()
                };

                let total_price = *leg1_entry_price + opposite_ask;
                
                // Check hedge condition FIRST. Only do stop loss when hedge condition is NOT met.
                // (Previously stop loss was checked first, so we could buy "stop loss" even when
                // total_price <= sum_target and the normal hedge should have run.)
                if total_price <= self.sum_target {
                    if let Some(token_id) = opposite_token_id {
                        crate::log_println!(
                            "{}: Hedge condition met! Leg1: ${:.4} + Opposite: ${:.4} = ${:.4} <= ${:.4}",
                            market_name,
                            leg1_entry_price,
                            opposite_ask,
                            total_price,
                            self.sum_target
                        );
                        crate::log_println!(
                            "{}: Buying {} {} shares @ ${:.4} (Leg 2)",
                            market_name,
                            self.shares,
                            opposite_side,
                            opposite_ask
                        );

                        self.execute_buy(
                            market_name,
                            opposite_side,
                            token_id,
                            self.shares,
                            opposite_ask,
                        ).await?;
                        self.record_trade(
                            &market_state.condition_id,
                            period_timestamp,
                            opposite_side,
                            token_id,
                            self.shares,
                            opposite_ask,
                        ).await?;

                        let profit_percent = ((1.0 - total_price) / total_price) * 100.0;
                        let total_cost = (*leg1_entry_price * *leg1_shares) + (opposite_ask * self.shares);
                        let expected_profit = (self.shares * 1.0) - total_cost;
                        
                        crate::log_println!(
                            "{}: Cycle complete! Locked in ~{:.2}% profit (${:.4} + ${:.4} = ${:.4}) | Expected profit: ${:.2}",
                            market_name,
                            profit_percent,
                            leg1_entry_price,
                            opposite_ask,
                            total_price,
                            expected_profit
                        );
                        
                        let mut period_profit = self.period_profit.lock().await;
                        *period_profit += expected_profit;
                        drop(period_profit);
                        
                        let mut total_profit = self.total_profit.lock().await;
                        *total_profit += expected_profit;
                        drop(total_profit);
                        
                        let market_key = format!("{}:{}", market_state.condition_id, period_timestamp);
                        let mut trades = self.trades.lock().await;
                        if let Some(trade) = trades.get_mut(&market_key) {
                            trade.expected_profit = expected_profit;
                        }
                        drop(trades);

                        market_state.phase = TradingPhase::CycleComplete {
                            leg1_side: leg1_side.clone(),
                            leg1_entry_price: *leg1_entry_price,
                            leg1_shares: *leg1_shares,
                            leg2_side: opposite_side.to_string(),
                            leg2_entry_price: opposite_ask,
                            leg2_shares: self.shares,
                            total_cost,
                        };
                    }
                    return Ok(());
                }
                // Hedge condition not met. Trigger stop loss when remaining time until market close is less than threshold.
                if time_remaining_minutes <= self.stop_loss_last_remaining_minutes {
                    let leg1_bid = if leg1_side == "Up" { up_bid } else { down_bid };
                    if self.stop_loss_method == "sell_position" {
                        crate::log_println!(
                            "{}: STOP LOSS TRIGGERED (Hedge not met, remaining time {}m <= {}m) | Selling position ({})",
                            market_name,
                            time_remaining_minutes,
                            self.stop_loss_last_remaining_minutes,
                            leg1_side
                        );
                        self.execute_stop_loss_sell(
                            market_name,
                            market_state,
                            leg1_side,
                            &leg1_token_id,
                            *leg1_entry_price,
                            *leg1_shares,
                            leg1_bid,
                            period_timestamp,
                        ).await?;
                        return Ok(());
                    }
                    // buy_opposite (default)
                    if let Some(token_id) = opposite_token_id.cloned() {
                        crate::log_println!(
                            "{}: STOP LOSS TRIGGERED (Hedge not met, remaining time {}m <= {}m) | Buying opposite side to hedge",
                            market_name,
                            time_remaining_minutes,
                            self.stop_loss_last_remaining_minutes
                        );
                        self.execute_stop_loss_hedge(
                            market_name,
                            market_state,
                            leg1_side,
                            *leg1_entry_price,
                            *leg1_shares,
                            opposite_side,
                            &token_id,
                            opposite_ask,
                            period_timestamp,
                        ).await?;
                        return Ok(());
                    }
                }
                if current_time % 10 == 0 {
                    crate::log_println!(
                        "{}: Waiting for hedge... Leg1: ${:.4} + Current {}: ${:.4} = ${:.4} (need <= ${:.4}) | Remaining: {}m (stop loss if <= {}m)",
                        market_name,
                        leg1_entry_price,
                        if leg1_side == "Up" { "Down" } else { "Up" },
                        opposite_ask,
                        total_price,
                        self.sum_target,
                        time_remaining_minutes,
                        self.stop_loss_last_remaining_minutes
                    );
                }
            }

            TradingPhase::CycleComplete { .. } => {
                // Cycle complete, do nothing
            }
        }

        Ok(())
    }

    fn check_dump(&self, price_history: &VecDeque<(u64, f64)>, current_time: u64) -> Option<bool> {
        if price_history.len() < 2 {
            return None;
        }

        let lookback_cutoff = current_time.saturating_sub(self.dump_lookback_seconds);
        let mut old_price_opt = None;
        let mut old_timestamp_opt = None;
        let mut new_price_opt = None;
        let mut new_timestamp_opt = None;

        for (timestamp, price) in price_history.iter() {
            if *timestamp <= lookback_cutoff {
                if old_timestamp_opt.is_none() || *timestamp > old_timestamp_opt.unwrap() {
                    old_price_opt = Some(*price);
                    old_timestamp_opt = Some(*timestamp);
                }
            }
            
            // Find newest price
            if new_timestamp_opt.is_none() || *timestamp > new_timestamp_opt.unwrap() {
                new_price_opt = Some(*price);
                new_timestamp_opt = Some(*timestamp);
            }
        }

        if old_price_opt.is_none() && !price_history.is_empty() {
            if let Some((timestamp, price)) = price_history.front() {
                old_price_opt = Some(*price);
                old_timestamp_opt = Some(*timestamp);
            }
        }

        if new_price_opt.is_none() && !price_history.is_empty() {
            if let Some((timestamp, price)) = price_history.back() {
                new_price_opt = Some(*price);
                new_timestamp_opt = Some(*timestamp);
            }
        }

        match (old_price_opt, new_price_opt, old_timestamp_opt, new_timestamp_opt) {
            (Some(old_price), Some(new_price), Some(old_ts), Some(new_ts)) if old_price > 0.0 => {
                let time_diff = new_ts.saturating_sub(old_ts);
                if time_diff < 1 || time_diff > 5 {
                    return None;
                }
                
                let price_drop = old_price - new_price;
                let drop_percent = price_drop / old_price;
                Some(drop_percent >= self.move_threshold && price_drop > 0.0)
            }
            _ => None,
        }
    }

    async fn execute_buy(
        &self,
        market_name: &str,
        side: &str,
        token_id: &str,
        shares: f64,
        price: f64,
    ) -> Result<()> {
        crate::log_println!(
            "{} BUY {} {} shares @ ${:.4}",
            market_name,
            side,
            shares,
            price
        );

        if self.simulation_mode {
            crate::log_println!("SIMULATION: Order executed");
        } else {
            let shares_rounded = (shares * 10000.0).round() / 10000.0;
            match self.api.place_market_order(token_id, shares_rounded, "BUY", None).await {
                Ok(_) => crate::log_println!("REAL: Order placed"),
                Err(e) => {
                    warn!("Failed to place order: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// Returns the actual size sold (for P&L). Returns 0.0 if skipped (e.g. position too small).
    async fn execute_sell(
        &self,
        market_name: &str,
        side: &str,
        token_id: &str,
        shares: f64,
        price: f64,
    ) -> Result<f64> {
        // Cap sell size to actual position balance (avoids "not enough balance" when received < ordered, e.g. 4.96 vs 5)
        let sell_size = if !self.simulation_mode {
            match self.api.get_trading_address() {
                Ok(addr) => {
                    match self.api.get_position_size(&addr, token_id).await {
                        Ok(Some(bal)) => {
                            let cap = bal.min(shares);
                            let rounded = (cap * 100.0).floor() / 100.0;
                            if rounded < 0.01 {
                                warn!("{}: Position size {:.4} too small to sell", market_name, bal);
                                return Ok(0.0);
                            }
                            if (rounded - shares).abs() > 0.001 {
                                crate::log_println!(
                                    "{}: Capping SELL to position balance: {:.2} (requested {:.2})",
                                    market_name, rounded, shares
                                );
                            }
                            rounded
                        }
                        _ => (shares * 10000.0).round() / 10000.0,
                    }
                }
                Err(_) => (shares * 10000.0).round() / 10000.0,
            }
        } else {
            shares
        };

        crate::log_println!(
            "{} SELL {} {:.2} shares @ ${:.4}",
            market_name,
            side,
            sell_size,
            price
        );

        if self.simulation_mode {
            crate::log_println!("SIMULATION: Order executed");
        } else {
            match self.api.place_market_order(token_id, sell_size, "SELL", None).await {
                Ok(_) => crate::log_println!("REAL: Order placed"),
                Err(e) => {
                    warn!("Failed to place sell order: {}", e);
                    return Err(e);
                }
            }
        }

        Ok(sell_size)
    }

    /// Execute stop loss by selling the held leg 1 position (no hedge)
    async fn execute_stop_loss_sell(
        &self,
        market_name: &str,
        market_state: &mut MarketState,
        leg1_side: &str,
        leg1_token_id: &str,
        leg1_entry_price: f64,
        leg1_shares: f64,
        leg1_bid: f64,
        period_timestamp: u64,
    ) -> Result<()> {
        crate::log_println!(
            "{}: STOP LOSS SELL - Selling {} {} shares @ ${:.4} (BID) (bought @ ${:.4})",
            market_name,
            leg1_shares,
            leg1_side,
            leg1_bid,
            leg1_entry_price
        );

        let actual_sold = self.execute_sell(
            market_name,
            leg1_side,
            leg1_token_id,
            leg1_shares,
            leg1_bid,
        ).await?;

        let cost = leg1_entry_price * actual_sold;
        let proceeds = leg1_bid * actual_sold;
        let profit = proceeds - cost;

        crate::log_println!(
            "{}: Stop loss sell complete! Cost: ${:.2} | Proceeds: ${:.2} | P&L: ${:.2}",
            market_name,
            cost,
            proceeds,
            profit
        );

        let mut period_profit = self.period_profit.lock().await;
        *period_profit += profit;
        drop(period_profit);

        let mut total_profit = self.total_profit.lock().await;
        *total_profit += profit;
        drop(total_profit);

        let market_key = format!("{}:{}", market_state.condition_id, period_timestamp);
        let mut trades = self.trades.lock().await;
        trades.remove(&market_key);
        drop(trades);

        market_state.phase = TradingPhase::CycleComplete {
            leg1_side: leg1_side.to_string(),
            leg1_entry_price,
            leg1_shares,
            leg2_side: "Sold".to_string(),
            leg2_entry_price: leg1_bid,
            leg2_shares: 0.0,
            total_cost: cost - proceeds,
        };

        Ok(())
    }

    /// Execute stop loss by buying opposite side (hedge)
    async fn execute_stop_loss_hedge(
        &self,
        market_name: &str,
        market_state: &mut MarketState,
        leg1_side: &str,
        leg1_entry_price: f64,
        leg1_shares: f64,
        opposite_side: &str,
        opposite_token_id: &str,
        opposite_ask: f64,
        period_timestamp: u64,
    ) -> Result<()> {
        crate::log_println!(
            "{}: STOP LOSS HEDGE - Buying {} {} shares @ ${:.4} (opposite of Leg 1: {} @ ${:.4})",
            market_name,
            leg1_shares,
            opposite_side,
            opposite_ask,
            leg1_side,
            leg1_entry_price
        );

        self.execute_buy(
            market_name,
            opposite_side,
            opposite_token_id,
            leg1_shares,
            opposite_ask,
        ).await?;
        self.record_trade(
            &market_state.condition_id,
            period_timestamp,
            opposite_side,
            opposite_token_id,
            leg1_shares,
            opposite_ask,
        ).await?;

        let total_cost = (leg1_entry_price * leg1_shares) + (opposite_ask * leg1_shares);
        let total_price_per_share = leg1_entry_price + opposite_ask;
        let expected_profit = (leg1_shares * 1.0) - total_cost; // One side will win $1.00
        
        // Calculate profit percentage
        let profit_percent = if total_price_per_share > 0.0 {
            ((1.0 - total_price_per_share) / total_price_per_share) * 100.0
        } else {
            0.0
        };
        
        crate::log_println!(
            "{}: Stop loss hedge complete! Total cost: ${:.2} (${:.4} + ${:.4} = ${:.4}) | Expected profit: ${:.2} ({:.2}%)",
            market_name,
            total_cost,
            leg1_entry_price,
            opposite_ask,
            total_price_per_share,
            expected_profit,
            profit_percent
        );
        
        let mut period_profit = self.period_profit.lock().await;
        *period_profit += expected_profit;
        drop(period_profit);
        
        let mut total_profit = self.total_profit.lock().await;
        *total_profit += expected_profit;
        drop(total_profit);
        
        let market_key = format!("{}:{}", market_state.condition_id, period_timestamp);
        let mut trades = self.trades.lock().await;
        if let Some(trade) = trades.get_mut(&market_key) {
            trade.expected_profit = expected_profit;
        }
        drop(trades);

        market_state.phase = TradingPhase::CycleComplete {
            leg1_side: leg1_side.to_string(),
            leg1_entry_price,
            leg1_shares,
            leg2_side: opposite_side.to_string(),
            leg2_entry_price: opposite_ask,
            leg2_shares: leg1_shares,
            total_cost,
        };
        
        Ok(())
    }

    async fn record_trade(
        &self,
        condition_id: &str,
        period_timestamp: u64,
        side: &str,
        token_id: &str,
        shares: f64,
        price: f64,
    ) -> Result<()> {
        let market_key = format!("{}:{}", condition_id, period_timestamp);
        let mut trades = self.trades.lock().await;
        
        let trade = trades.entry(market_key.clone())
            .or_insert_with(|| CycleTrade {
                condition_id: condition_id.to_string(),
                period_timestamp,
                up_token_id: None,
                down_token_id: None,
                up_shares: 0.0,
                down_shares: 0.0,
                up_avg_price: 0.0,
                down_avg_price: 0.0,
                expected_profit: 0.0,
            });
        
        match side {
            "Up" => {
                let old_total = trade.up_shares * trade.up_avg_price;
                trade.up_shares += shares;
                trade.up_avg_price = if trade.up_shares > 0.0 {
                    (old_total + shares * price) / trade.up_shares
                } else {
                    price
                };
                trade.up_token_id = Some(token_id.to_string());
            }
            "Down" => {
                let old_total = trade.down_shares * trade.down_avg_price;
                trade.down_shares += shares;
                trade.down_avg_price = if trade.down_shares > 0.0 {
                    (old_total + shares * price) / trade.down_shares
                } else {
                    price
                };
                trade.down_token_id = Some(token_id.to_string());
            }
            _ => {}
        }
        
        Ok(())
    }

    pub async fn check_market_closure(&self) -> Result<()> {
        let trades: Vec<(String, CycleTrade)> = {
            let trades = self.trades.lock().await;
            trades.iter()
                .map(|(key, trade)| (key.clone(), (*trade).clone()))
                .collect()
        };
        
        if trades.is_empty() {
            return Ok(()); // No trades to check
        }
        
        let current_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        for (market_key, trade) in trades {
            let market_end_timestamp = trade.period_timestamp + 900;
            
            if current_timestamp < market_end_timestamp {
                continue;
            }
            
            let states = self.market_states.lock().await;
            if let Some(market_state) = states.get(&trade.condition_id) {
                if market_state.closure_checked {
                    drop(states);
                    continue; // Already processed
                }
            }
            drop(states);
            
            let time_since_close = current_timestamp - market_end_timestamp;
            let minutes_since_close = time_since_close / 60;
            let seconds_since_close = time_since_close % 60;
            
            crate::log_println!("Market {} closed {}m {}s ago | Checking resolution...", 
                  &trade.condition_id[..8], minutes_since_close, seconds_since_close);
            
            // Check if market is closed and resolved
            let market = match self.api.get_market(&trade.condition_id).await {
                Ok(m) => m,
                Err(e) => {
                    warn!("Failed to fetch market {}: {}", &trade.condition_id[..8], e);
                    continue;
                }
            };
            
            if !market.closed {
                if time_since_close % 60 == 0 || time_since_close < 60 {
                    crate::log_println!("Market {} not yet closed (API says active) | Will retry in next check...", 
                          &trade.condition_id[..8]);
                }
                continue;
            }
            
            crate::log_println!("Market {} is closed and resolved", &trade.condition_id[..8]);
            
            let up_is_winner = trade.up_token_id.as_ref()
                .map(|id| market.tokens.iter().any(|t| t.token_id == *id && t.winner))
                .unwrap_or(false);
            let down_is_winner = trade.down_token_id.as_ref()
                .map(|id| market.tokens.iter().any(|t| t.token_id == *id && t.winner))
                .unwrap_or(false);
            
            let mut actual_profit = 0.0;
            
            // Redeem once per condition (Safe path redeems both index sets [1,2] in one call; non-Safe still per-outcome)
            let has_winning_shares = (up_is_winner && trade.up_shares > 0.001) || (down_is_winner && trade.down_shares > 0.001);
            if !self.simulation_mode && has_winning_shares {
                let units = (if up_is_winner { trade.up_shares } else { 0.0 }) + (if down_is_winner { trade.down_shares } else { 0.0 });
                let (token_id, outcome) = if up_is_winner && trade.up_shares > 0.001 {
                    (trade.up_token_id.as_deref().unwrap_or(""), "Up")
                } else {
                    (trade.down_token_id.as_deref().unwrap_or(""), "Down")
                };
                if let Err(e) = self.redeem_token_by_id(token_id, outcome, units, outcome, &trade.condition_id).await {
                    warn!("Failed to redeem tokens for condition: {}", e);
                }
            }
            
            if trade.up_shares > 0.001 {
                if up_is_winner {
                    
                    let remaining_value = trade.up_shares * 1.0;
                    let remaining_cost = trade.up_avg_price * trade.up_shares;
                    let profit = remaining_value - remaining_cost;
                    actual_profit += profit;
                    
                    crate::log_println!("Market Closed - Up Winner: {:.2} shares @ ${:.4} avg | Profit: ${:.2}", 
                          trade.up_shares, trade.up_avg_price, profit);
                } else {
                    let loss = trade.up_avg_price * trade.up_shares;
                    actual_profit -= loss;
                    
                    crate::log_println!("Market Closed - Up Lost: {:.2} shares @ ${:.4} avg | Loss: ${:.2}", 
                          trade.up_shares, trade.up_avg_price, loss);
                }
            }
            
            if trade.down_shares > 0.001 {
                if down_is_winner {
                    
                    let remaining_value = trade.down_shares * 1.0;
                    let remaining_cost = trade.down_avg_price * trade.down_shares;
                    let profit = remaining_value - remaining_cost;
                    actual_profit += profit;
                    
                    crate::log_println!("Market Closed - Down Winner: {:.2} shares @ ${:.4} avg | Profit: ${:.2}", 
                          trade.down_shares, trade.down_avg_price, profit);
                } else {
                    let loss = trade.down_avg_price * trade.down_shares;
                    actual_profit -= loss;
                    
                    crate::log_println!("Market Closed - Down Lost: {:.2} shares @ ${:.4} avg | Loss: ${:.2}", 
                          trade.down_shares, trade.down_avg_price, loss);
                }
            }
            
            if trade.expected_profit != 0.0 {
                let mut total = self.total_profit.lock().await;
                *total = *total - trade.expected_profit + actual_profit;
                drop(total);
                
                let mut period = self.period_profit.lock().await;
                *period = *period - trade.expected_profit + actual_profit;
                drop(period);
            } else {
                let mut total = self.total_profit.lock().await;
                *total += actual_profit;
                drop(total);
                
                let mut period = self.period_profit.lock().await;
                *period += actual_profit;
                drop(period);
            }
            
            let total_profit = *self.total_profit.lock().await;
            let period_profit = *self.period_profit.lock().await;
            crate::log_println!("Period Profit: ${:.2} | Total Profit: ${:.2}", period_profit, total_profit);
            
            let mut states = self.market_states.lock().await;
            if let Some(market_state) = states.get_mut(&trade.condition_id) {
                market_state.closure_checked = true;
            }
            drop(states);
            
            // Remove trade
            let mut trades = self.trades.lock().await;
            trades.remove(&market_key);
            crate::log_println!("Trade removed from tracking");
        }
        
        Ok(())
    }
    
    async fn redeem_token_by_id(&self, token_id: &str, token_name: &str, units: f64, outcome: &str, condition_id: &str) -> Result<()> {
        crate::log_println!("Redeeming {:.2} units of {} (outcome: {})", units, token_name, outcome);
        
        match self.api.redeem_tokens(
            condition_id,
            token_id,
            outcome,
        ).await {
            Ok(_) => {
                crate::log_println!("Successfully redeemed {:.2} units", units);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to redeem tokens: {}", e);
                Err(e)
            }
        }
    }

    pub async fn reset_period(&self) {
        let mut states = self.market_states.lock().await;
        states.clear();
        crate::log_println!("Dump-Hedge Trader: Period reset");
    }
    
    /// Get current total profit
    pub async fn get_total_profit(&self) -> f64 {
        *self.total_profit.lock().await
    }
    
    pub async fn get_period_profit(&self) -> f64 {
        *self.period_profit.lock().await
    }
}
