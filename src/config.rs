use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Run in simulation mode (no real trades)
    /// Default: simulation mode is enabled (true)
    #[arg(short, long, default_value_t = true)]
    pub simulation: bool,

    /// Run in production mode (execute real trades)
    /// This sets simulation to false
    #[arg(long)]
    pub production: bool,

    /// Configuration file path
    #[arg(short, long, default_value = "config.json")]
    pub config: PathBuf,

    /// Redeem-only mode: fetch redeemable positions and redeem, or redeem a specific condition
    #[arg(long)]
    pub redeem: bool,

    /// When using --redeem: redeem only this condition ID (hex). If omitted, redeem all redeemable positions.
    #[arg(long, requires = "redeem")]
    pub condition_id: Option<String>,
}

impl Args {
    pub fn is_simulation(&self) -> bool {
        if self.production {
            false
        } else {
            self.simulation
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub polymarket: PolymarketConfig,
    pub trading: TradingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketConfig {
    pub gamma_api_url: String,
    pub clob_api_url: String,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub api_passphrase: Option<String>,
    pub private_key: Option<String>,
    pub proxy_wallet_address: Option<String>,
    pub signature_type: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingConfig {
    pub check_interval_ms: u64,
    #[serde(default = "default_market_closure_check_interval")]
    pub market_closure_check_interval_seconds: u64,
    #[serde(default = "default_data_source")]
    pub data_source: String,
    #[serde(default = "default_markets")]
    pub markets: Vec<String>,
    pub dump_hedge_shares: Option<f64>,
    pub dump_hedge_sum_target: Option<f64>,
    pub dump_hedge_move_threshold: Option<f64>,
    pub dump_hedge_window_minutes: Option<u64>,
    /// Seconds to look back for "old" price when detecting a dump (e.g. 15% drop within a short window). Default: 3.
    pub dump_hedge_dump_lookback_seconds: Option<u64>,
    /// Trigger stop loss when remaining time until market close is less than this many minutes (e.g. 5 = last 5 min).
    /// Replaces the previous "max wait" semantics: now based on time left, not time elapsed since leg 1.
    #[serde(alias = "dump_hedge_stop_loss_max_wait_minutes")]
    pub dump_hedge_stop_loss_last_remaining_minutes: Option<u64>,
    pub dump_hedge_stop_loss_percentage: Option<f64>,
    /// Stop loss action when hedge condition is not met in time: "buy_opposite" (hedge by buying opposite side) or "sell_position" (sell holding leg 1). Default: "buy_opposite".
    #[serde(default = "default_stop_loss_management_method")]
    pub stop_loss_management_method: String,
}

fn default_market_closure_check_interval() -> u64 {
    20
}

fn default_data_source() -> String {
    "api".to_string()
}

fn default_markets() -> Vec<String> {
    vec!["btc".to_string()]
}

fn default_stop_loss_management_method() -> String {
    "buy_opposite".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            polymarket: PolymarketConfig {
                gamma_api_url: "https://gamma-api.polymarket.com".to_string(),
                clob_api_url: "https://clob.polymarket.com".to_string(),
                api_key: None,
                api_secret: None,
                api_passphrase: None,
                private_key: None,
                proxy_wallet_address: None,
                signature_type: None,
            },
            trading: TradingConfig {
                check_interval_ms: 1000,
                market_closure_check_interval_seconds: 20,
                data_source: "api".to_string(),
                markets: vec!["btc".to_string()],
                dump_hedge_shares: Some(10.0),
                dump_hedge_sum_target: Some(0.95),
                dump_hedge_move_threshold: Some(0.15),
                dump_hedge_window_minutes: Some(2),
                dump_hedge_dump_lookback_seconds: Some(3),
                dump_hedge_stop_loss_last_remaining_minutes: Some(5),
                dump_hedge_stop_loss_percentage: Some(0.20),
                stop_loss_management_method: "buy_opposite".to_string(),
            },
        }
    }
}

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            let config = Config::default();
            let content = serde_json::to_string_pretty(&config)?;
            std::fs::write(path, content)?;
            Ok(config)
        }
    }
}

