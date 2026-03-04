mod api;
mod config;
mod models;
mod monitor;
mod dump_hedge_trader;

use anyhow::{Context, Result};
use clap::Parser;
use config::{Args, Config};
use log::warn;
use std::sync::Arc;
use std::io::{self, Write};
use std::fs::{File, OpenOptions};
use std::sync::{Mutex, OnceLock};

use api::PolymarketApi;
use dump_hedge_trader::DumpHedgeTrader;
use monitor::MarketMonitor;

struct DualWriter {
    stderr: io::Stderr,
    file: Mutex<File>,
}

impl Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = self.stderr.write_all(buf);
        let _ = self.stderr.flush();
        let mut file = self.file.lock().unwrap();
        file.write_all(buf)?;
        file.flush()?;
        
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()?;
        let mut file = self.file.lock().unwrap();
        file.flush()?;
        Ok(())
    }
}

unsafe impl Send for DualWriter {}
unsafe impl Sync for DualWriter {}

static HISTORY_FILE: OnceLock<Mutex<File>> = OnceLock::new();

fn init_history_file(file: File) {
    HISTORY_FILE.set(Mutex::new(file)).expect("History file already initialized");
}

pub fn log_to_history(message: &str) {
    eprint!("{}", message);
    let _ = io::stderr().flush();
    if let Some(file_mutex) = HISTORY_FILE.get() {
        if let Ok(mut file) = file_mutex.lock() {
            let _ = write!(file, "{}", message);
            let _ = file.flush();
        }
    }
}

#[macro_export]
macro_rules! log_println {
    ($($arg:tt)*) => {
        {
            let message = format!($($arg)*);
            $crate::log_to_history(&format!("{}\n", message));
        }
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    // Open log file
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("history.toml")
        .context("Failed to open history.toml for logging")?;
    
    init_history_file(log_file.try_clone().context("Failed to clone history file")?);
    
    let dual_writer = DualWriter {
        stderr: io::stderr(),
        file: Mutex::new(log_file),
    };
    
    // Initialize logger
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .target(env_logger::Target::Pipe(Box::new(dual_writer)))
        .init();

    let args = Args::parse();
    let config = Config::load(&args.config)?;

    let api = Arc::new(PolymarketApi::new(
        config.polymarket.gamma_api_url.clone(),
        config.polymarket.clob_api_url.clone(),
        config.polymarket.api_key.clone(),
        config.polymarket.api_secret.clone(),
        config.polymarket.api_passphrase.clone(),
        config.polymarket.private_key.clone(),
        config.polymarket.proxy_wallet_address.clone(),
        config.polymarket.signature_type,
    ));

    // Redeem-only mode: --redeem [--condition-id 0x...]
    if args.redeem {
        run_redeem_only(api.as_ref(), &config, args.condition_id.as_deref()).await?;
        return Ok(());
    }

    eprintln!("Starting Polymarket Hedge Trading Bot");
    let is_simulation = args.is_simulation();
    eprintln!("Mode: {}", if is_simulation { "SIMULATION" } else { "PRODUCTION" });

    if !is_simulation {
        match api.authenticate().await {
            Ok(_) => {
                eprintln!("Authentication successful");
            }
            Err(e) => {
                warn!("Failed to authenticate: {}", e);
                warn!("Order placement may fail. Verify credentials in config.json");
            }
        }
    }

    let markets = &config.trading.markets;
    if markets.is_empty() {
        anyhow::bail!("No markets configured. Please add markets to config.json (e.g., [\"btc\", \"eth\", \"sol\", \"xrp\"])");
    }
    
    let data_source = config.trading.data_source.clone();
    let shares = config.trading.dump_hedge_shares.unwrap_or(10.0);
    let sum_target = config.trading.dump_hedge_sum_target.unwrap_or(0.95);
    let move_threshold = config.trading.dump_hedge_move_threshold.unwrap_or(0.15);
    let window_minutes = config.trading.dump_hedge_window_minutes.unwrap_or(2);
    let dump_lookback_seconds = config.trading.dump_hedge_dump_lookback_seconds.unwrap_or(3);
    let stop_loss_last_remaining = config.trading.dump_hedge_stop_loss_last_remaining_minutes.unwrap_or(5);
    let stop_loss_percentage = config.trading.dump_hedge_stop_loss_percentage.unwrap_or(0.20);
    let stop_loss_method = config.trading.stop_loss_management_method.trim().to_lowercase();
    let stop_loss_method = if stop_loss_method == "sell_position" {
        "sell_position"
    } else {
        "buy_opposite"
    };

    eprintln!("Strategy: DUMP-AND-HEDGE");
    eprintln!("   - Markets: {}", markets.join(", ").to_uppercase());
    eprintln!("   - Data source: {}", data_source.to_uppercase());
    eprintln!("   - Shares per leg: {}", shares);
    eprintln!("   - Sum target: {}", sum_target);
    eprintln!("   - Move threshold: {}%", move_threshold * 100.0);
    eprintln!("   - Dump lookback: {} seconds", dump_lookback_seconds);
    eprintln!("   - Watch window: {} minutes", window_minutes);
    eprintln!("   - Stop Loss: trigger when remaining time < {}min | Method: {}", stop_loss_last_remaining, stop_loss_method);
    eprintln!("   - Mode: {}", if is_simulation { "SIMULATION" } else { "PRODUCTION" });
    eprintln!("");
    
    let dump_hedge_trader = DumpHedgeTrader::new(
        api.clone(),
        is_simulation,
        shares,
        sum_target,
        move_threshold,
        window_minutes,
        dump_lookback_seconds,
        stop_loss_last_remaining,
        stop_loss_percentage,
        stop_loss_method.to_string(),
    );
    let trader_arc = Arc::new(dump_hedge_trader);
    let trader_clone = trader_arc.clone();
    
    // Start background task to check market closure
    let trader_closure = trader_clone.clone();
    let market_closure_interval = config.trading.market_closure_check_interval_seconds;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(
            market_closure_interval
        ));
        loop {
            interval.tick().await;
            if let Err(e) = trader_closure.check_market_closure().await {
                warn!("Error checking market closure: {}", e);
            }
            
            let total_profit = trader_closure.get_total_profit().await;
            let period_profit = trader_closure.get_period_profit().await;
            if total_profit != 0.0 || period_profit != 0.0 {
                crate::log_println!("Current Profit - Period: ${:.2} | Total: ${:.2}", period_profit, total_profit);
            }
        }
    });
    
    let mut handles = Vec::new();
    
    for asset in markets {
        let asset_upper = asset.to_uppercase();
        let market_name = format!("{} 15m", asset_upper);
        
        eprintln!("Discovering {} market...", market_name);
        let market = match discover_market_for_asset(&api, asset).await {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to discover {} market: {}. Skipping...", market_name, e);
                continue;
            }
        };
        
        let monitor = MarketMonitor::new(
            api.clone(),
            market_name.clone(),
            market,
            config.trading.check_interval_ms,
            data_source.clone(),
            config.polymarket.clob_api_url.clone(),
        );
        let monitor_arc = Arc::new(monitor);
        
        let monitor_for_period_check = monitor_arc.clone();
        let api_for_period_check = api.clone();
        let trader_for_period_reset = trader_clone.clone();
        let asset_clone = asset.to_string();
        let market_name_clone = market_name.clone();
        
        let handle = tokio::spawn(async move {
            let mut last_processed_period: Option<u64> = None;
            loop {
                let current_market_timestamp = monitor_for_period_check.get_current_market_timestamp().await;
                let next_period_timestamp = current_market_timestamp + 900;
                
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                let sleep_duration = if next_period_timestamp > current_time {
                    next_period_timestamp - current_time
                } else {
                    0
                };
                
                tokio::time::sleep(tokio::time::Duration::from_secs(sleep_duration)).await;
                
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let current_period = (current_time / 900) * 900;
                
                if let Some(last_period) = last_processed_period {
                    if current_period == last_period {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                }
                
                eprintln!("New period detected for {}! (Period: {}) Discovering new market...", market_name_clone, current_period);
                last_processed_period = Some(current_period);
                
                match discover_market_for_asset(&api_for_period_check, &asset_clone).await {
                    Ok(new_market) => {
                        if let Err(e) = monitor_for_period_check.update_market(new_market).await {
                            warn!("Failed to update {} market: {}", market_name_clone, e);
                        } else {
                            let trader_reset = trader_for_period_reset.clone();
                            trader_reset.reset_period().await;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to discover new {} market: {}", market_name_clone, e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    }
                }
            }
        });
        handles.push(handle);
        
        let monitor_start = monitor_arc.clone();
        let trader_start = trader_clone.clone();
        tokio::spawn(async move {
            monitor_start.start_monitoring(move |snapshot| {
                let trader = trader_start.clone();
                
                async move {
                    if let Err(e) = trader.process_snapshot(&snapshot).await {
                        warn!("Error processing snapshot: {}", e);
                    }
                }
            }).await;
        });
    }
    
    if handles.is_empty() {
        anyhow::bail!("No valid markets found. Please check your market configuration.");
    }
    
    eprintln!("Started monitoring {} market(s)", handles.len());
    
    futures::future::join_all(handles).await;

    Ok(())
}

/// Redeem-only mode: redeem a specific condition or all redeemable positions for the proxy wallet.
async fn run_redeem_only(
    api: &PolymarketApi,
    config: &Config,
    condition_id: Option<&str>,
) -> Result<()> {
    let proxy = config.polymarket.proxy_wallet_address.as_deref()
        .ok_or_else(|| anyhow::anyhow!("--redeem requires proxy_wallet_address in config.json"))?;
    let _pk = config.polymarket.private_key.as_deref()
        .ok_or_else(|| anyhow::anyhow!("--redeem requires private_key in config.json"))?;

    eprintln!("Redeem-only mode (proxy: {})", proxy);
    let cids: Vec<String> = if let Some(cid) = condition_id {
        let cid = if cid.starts_with("0x") { cid.to_string() } else { format!("0x{}", cid) };
        eprintln!("Redeeming condition: {}", cid);
        vec![cid]
    } else {
        eprintln!("Fetching redeemable positions...");
        let list = api.get_redeemable_positions(proxy).await?;
        if list.is_empty() {
            eprintln!("No redeemable positions found.");
            return Ok(());
        }
        eprintln!("Found {} condition(s) to redeem.", list.len());
        list
    };

    let mut ok_count = 0u32;
    let mut fail_count = 0u32;
    for cid in &cids {
        eprintln!("\n--- Redeeming condition {} ---", &cid[..cid.len().min(18)]);
        match api.redeem_tokens(cid, "", "Up").await {
            Ok(_) => {
                eprintln!("Success: {}", cid);
                ok_count += 1;
            }
            Err(e) => {
                eprintln!("Failed to redeem {}: {} (skipping)", cid, e);
                fail_count += 1;
            }
        }
    }
    eprintln!("\nRedeem complete. Succeeded: {}, Failed: {}", ok_count, fail_count);
    Ok(())
}

async fn discover_market_for_asset(
    api: &PolymarketApi,
    asset: &str,
) -> Result<crate::models::Market> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let mut seen_ids = std::collections::HashSet::new();
    let asset_lower = asset.to_lowercase();
    let slug_prefix = match asset_lower.as_str() {
        "btc" => "btc",
        "eth" => "eth",
        "sol" => "sol",
        "xrp" => "xrp",
        _ => anyhow::bail!("Unsupported asset: {}. Supported: BTC, ETH, SOL, XRP", asset),
    };
    discover_market(api, asset, slug_prefix, 15, current_time, &mut seen_ids).await
        .context(format!("Failed to discover {} 15m market", asset))
}

async fn discover_btc_15m_market(
    api: &PolymarketApi,
) -> Result<crate::models::Market> {
    discover_market_for_asset(api, "BTC").await
}

async fn discover_market(
    api: &PolymarketApi,
    market_name: &str,
    slug_prefix: &str,
    market_duration_minutes: u64,
    current_time: u64,
    seen_ids: &mut std::collections::HashSet<String>,
) -> Result<crate::models::Market> {
    if market_duration_minutes != 15 {
        anyhow::bail!("Only 15-minute markets are supported");
    }
    
    let period_duration_secs = 900;
    let rounded_time = (current_time / period_duration_secs) * period_duration_secs;
    let timeframe_str = "15m";
    let slug = format!("{}-updown-{}-{}", slug_prefix, timeframe_str, rounded_time);
    
    if let Ok(market) = api.get_market_by_slug(&slug).await {
        if !seen_ids.contains(&market.condition_id) && market.active && !market.closed {
            eprintln!("Found {} {} market by slug: {} | Condition ID: {}", market_name, timeframe_str, market.slug, market.condition_id);
            return Ok(market);
        }
    }
    
    for offset in 1..=3 {
        let try_time = rounded_time - (offset * period_duration_secs);
        let try_slug = format!("{}-updown-{}-{}", slug_prefix, timeframe_str, try_time);
        eprintln!("Trying previous {} {} market by slug: {}", market_name, timeframe_str, try_slug);
        if let Ok(market) = api.get_market_by_slug(&try_slug).await {
            if !seen_ids.contains(&market.condition_id) && market.active && !market.closed {
                eprintln!("Found {} {} market by slug: {} | Condition ID: {}", market_name, timeframe_str, market.slug, market.condition_id);
                return Ok(market);
            }
        }
    }
    
    anyhow::bail!("Could not find active {} {} up/down market. Please set condition_id in config.json", market_name, timeframe_str)
}

