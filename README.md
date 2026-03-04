# Polymarket Trading Bot Suite

A collection of automated trading bots for **Polymarket** binary Up/Down markets, built in Rust. These bots exploit short-term mispricings in 5-minute, 15-minute, and 1-hour prediction markets across **BTC, ETH, SOL, and XRP**.

> **This repository contains the 15-Minute Dump-and-Hedge Bot.**
> Other bot strategies are available separately — see [Get Other Bots](#get-other-bots) below.

---

## Bot Strategies at a Glance

| # | Bot | Timeframe | Core Idea |
|---|-----|-----------|-----------|
| 1 | [**15min Dump & Hedge**](#1-15-minute-dump-and-hedge-bot-current) | 15 min | Detect a price dump, buy the dumped side, then hedge with the opposite side |
| 2 | [**15min Pre-Order & Mid-Market**](#2-15-minute-pre-order-and-mid-market-bot) | 15 min | Pre-place limit buys on both sides at low prices before the period starts |
| 3 | [**1hour Pre-Limit Order**](#3-1-hour-pre-limit-order-bot) | 1 hour | Pre-place limit buys on both sides, merge when both fill |
| 4 | [**1hour Pre-Limit Order & Mid-Market**](#4-1-hour-pre-limit-order-and-mid-market-bot) | 1 hour | Pre-orders + dynamic mid-market orders in the current hour |
| 5 | [**5min Pre-Order & Mid-Market**](#5-5-minute-pre-order-and-mid-market-bot) | 5 min | Fast pre-orders on both sides for quick 5-minute markets |
| 6 | [**5min High-Side Buy**](#6-5-minute-high-side-buy-bot) | 5 min | Buy the likely winner (90c+) late in the period, ride momentum |
| 7 | [**5min Low-Side Buy**](#7-5-minute-low-side-buy-bot) | 5 min | Buy at 1-3c on both sides for asymmetric reversal payoffs |

---

## Strategy Summaries

### 1. 15-Minute Dump and Hedge Bot (CURRENT)

> **This is the bot included in this repository.**

Monitors 15-minute Up/Down markets and detects sudden price drops ("dumps"). When one side's ask price drops sharply (e.g. 15%+ in 3 seconds), the bot buys that side immediately. It then waits for the opposite side to become cheap enough that the combined cost is below a target (e.g. $0.95). If both legs fill, you hold both outcomes for under $1, guaranteeing profit at resolution ($1 payout). If the hedge doesn't come in time, a stop-loss kicks in.

**Key parameters:** dump threshold, hedge sum target, lookback window, stop-loss timing.

> **Strategy credit:** Based on [The Smart Ape's](https://x.com/the_smart_ape) two-leg catching-and-hedging strategy for Polymarket BTC 15-minute UP/DOWN markets ([original tweet](https://x.com/the_smart_ape/status/2005576087875527082) · [detailed write-up on Lookonchain](https://www.lookonchain.com/articles/1209)). The Smart Ape's approach — detect a sharp dump on one side, buy it, then hedge by buying the opposite side when the combined cost is below $1 — achieved ~86% ROI in backtesting. This bot is a Rust implementation of that core idea with added stop-loss management, multi-asset support, and automatic redemption.

**Real Results:**

![15min Dump & Hedge Results](docs/screenshots/15m-2.png)
![15min Dump & Hedge Results](docs/screenshots/15min-3.png)
![15min Dump & Hedge Results](docs/screenshots/15-ex-1.png)
![15min Dump & Hedge Results](docs/screenshots/15-ex-1-2.png)
![15min Dump & Hedge Results](docs/screenshots/15min.png)

[Read full strategy details ->](docs/15min-dump-and-hedge.md)

---

### 2. 15-Minute Pre-Order and Mid-Market Bot

Places limit BUY orders for **both Up and Down** at a low price (e.g. $0.45 each) before the next 15-minute period starts. If both fill, total cost < $1 and profit is locked in. Also places mid-market orders in the current period using dynamic pricing derived from live sell prices. Signal-based filters skip orders when the market is already one-sided.

**Key parameters:** price limit, signal stable range, sell-opposite timing, danger price.

**Real Results:**

![15min Pre-Order Results](docs/screenshots/15min-pre-order-mid-market-result.png)

[Read full strategy details ->](docs/15min-pre-order-mid-market.md)

---

### 3. 1-Hour Pre-Limit Order Bot

Targets 1-hour Up/Down markets. Places limit BUY orders on both sides at a fixed price before the next hour begins. When both sides fill, positions are **merged** (redeemed back to USDC) to lock in profit immediately without waiting for market resolution. Danger and timeout exits protect against one-sided fills.

**Key parameters:** price limit, merge logic, danger price, timeout.

**Real Results:**

![1hour Pre-Limit Results](docs/screenshots/1hour-pre-limit-order-result.png)

[Read full strategy details ->](docs/1hour-pre-limit-order.md)

---

### 4. 1-Hour Pre-Limit Order and Mid-Market Bot

Extends the 1-hour pre-limit strategy with **dynamic mid-market orders** during the current hour. The cheaper side is bought at its current sell price; the opposite side gets a small discount. Combined with pre-orders for the next hour, this maximizes fill opportunities. Same merge and risk management as the pre-limit bot.

**Key parameters:** price limit, opposite-side discount, mid-market enabled, signal.

**Real Results:**

![1hour Pre-Limit + Mid-Market Results](docs/screenshots/1hour-pre-limit-mid-market-result.png)

[Read full strategy details ->](docs/1hour-pre-limit-order-mid-market.md)

---

### 5. 5-Minute Pre-Order and Mid-Market Bot

Same concept as the 15-minute pre-order bot, adapted for the faster 5-minute markets. Places limit buys on both sides at low prices before the next period. The 5-minute timeframe means thinner liquidity and more frequent opportunities, but also requires faster signal evaluation and tighter risk management.

**Key parameters:** price limit, sell-opposite threshold and timing (in seconds), signal range.

**Real Results:**

![5min Pre-Order Results](docs/screenshots/5min-pre-order-mid-market-result.png)

[Read full strategy details ->](docs/5min-pre-order-mid-market.md)

---

### 6. 5-Minute High-Side Buy Bot

When one side is strongly favored (bid >= 90c+) late in a 5-minute period, the bot buys that side — betting the market consensus is right with limited time to reverse. If the price drops below a floor, the bot sells or hedges with the opposite side. Per-asset configuration allows different thresholds and behaviors.

**Key parameters:** threshold, after seconds, sell-under price, hedge (opposite) enabled.

**Real Results:**

![5min High-Side Buy Results](docs/screenshots/5min-high-side-buy-result.png)

[Read full strategy details ->](docs/5min-high-side-buy.md)

---

### 7. 5-Minute Low-Side Buy Bot

Places limit buys at very low prices (1c, 2c, 3c) on **both** Up and Down. Each fill is a cheap lottery ticket: risk 1-3c to potentially make 97-99c if that side wins. Take-profit tiers (e.g. sell 50% at 10c, rest at 15c) lock in multi-x returns even without holding to expiry. Unfilled orders are automatically cancelled near market close.

**Key parameters:** entry prices, take-profit tiers, cancel-unfilled timing.

**Real Results:**

![5min Low-Side Buy Results](docs/screenshots/5min-low-side-buy-result.png)

[Read full strategy details ->](docs/5min-low-side-buy.md)

---

## Current Bot: 15-Minute Dump and Hedge

This repository contains the **15-Minute Dump-and-Hedge Bot**. Below is everything you need to get it running.

### Prerequisites

- **Rust** (e.g. 1.70+): install from [rustup.rs](https://rustup.rs)
- **Polymarket account** and API credentials (for production)
- **Proxy wallet** and **private key** (for signing and redeeming)

### Configuration

Configuration is in **`config.json`** (path overridable with `--config`).

```json
{
  "polymarket": {
    "gamma_api_url": "https://gamma-api.polymarket.com",
    "clob_api_url": "https://clob.polymarket.com",
    "api_key": "",
    "api_secret": "",
    "api_passphrase": "",
    "private_key": "<your-private-key>",
    "proxy_wallet_address": "<your-proxy-wallet>",
    "signature_type": 2
  },
  "trading": {
    "check_interval_ms": 1000,
    "market_closure_check_interval_seconds": 20,
    "data_source": "api",
    "markets": ["btc"],
    "dump_hedge_shares": 5.0,
    "dump_hedge_sum_target": 0.95,
    "dump_hedge_move_threshold": 0.15,
    "dump_hedge_window_minutes": 2,
    "dump_hedge_dump_lookback_seconds": 3,
    "dump_hedge_stop_loss_last_remaining_minutes": 5,
    "dump_hedge_stop_loss_percentage": 0.25,
    "stop_loss_management_method": "sell_position"
  }
}
```

| Field | Description | Default |
|-------|-------------|---------|
| `dump_hedge_shares` | Size in shares per leg | `5.0` |
| `dump_hedge_sum_target` | Hedge when leg1 price + opposite ask <= this | `0.95` |
| `dump_hedge_move_threshold` | Dump detection: price drop >= this (0.15 = 15%) | `0.15` |
| `dump_hedge_window_minutes` | Only detect dumps in first N minutes | `2` |
| `dump_hedge_dump_lookback_seconds` | Seconds to look back for old price comparison | `3` |
| `dump_hedge_stop_loss_last_remaining_minutes` | Stop loss when time left <= this | `5` |
| `stop_loss_management_method` | `"sell_position"` or `"buy_opposite"` | `"sell_position"` |

### Build & Run

```bash
cargo build --release
```

**Simulation (no real orders):**

```bash
./target/release/polymarket-arbitrage-bot --simulation
```

**Production (live trading):**

```bash
./target/release/polymarket-arbitrage-bot --production --config config.json
```

### Redeem Mode

After a 15m market resolves, redeem winning positions:

```bash
./target/release/polymarket-arbitrage-bot --redeem --config config.json
```

Redeem a specific condition:

```bash
./target/release/polymarket-arbitrage-bot --redeem --condition-id 0x... --config config.json
```

### Running with PM2

```javascript
// ecosystem.config.cjs
module.exports = {
  apps: [{
    name: "polymarket-bot",
    script: "./target/release/polymarket-arbitrage-bot",
    args: "--production --config config.json",
    cwd: __dirname,
    interpreter: "none",
    autorestart: true,
    watch: false,
    max_memory_restart: "500M",
  }],
};
```

```bash
pm2 start ecosystem.config.cjs
pm2 logs polymarket-bot
```

### Logging

- **Stderr:** Main log stream (info level).
- **`history.toml`:** Append-only run log in the working directory.
- **RUST_LOG:** `RUST_LOG=info` (default) or `RUST_LOG=debug` for more detail.

### Supported Markets

Configured via `trading.markets` in `config.json`:

- `btc` — Bitcoin 15m Up/Down
- `eth` — Ethereum 15m Up/Down
- `sol` — Solana 15m Up/Down
- `xrp` — XRP 15m Up/Down

### Security

- **Never commit real keys.** Keep `config.json` out of version control.
- **`private_key`** controls funds; restrict file permissions and use a dedicated trading wallet.

### File Layout

| Path | Purpose |
|------|---------|
| `config.json` | Polymarket and trading settings |
| `src/main.rs` | Entry point, CLI, market discovery, redeem |
| `src/dump_hedge_trader.rs` | Dump-and-hedge strategy and state |
| `src/monitor.rs` | Market data (API/WebSocket) and snapshots |
| `src/api.rs` | Polymarket CLOB/Gamma API client |
| `src/config.rs` | Config and CLI parsing |
| `src/models.rs` | Market/token data structures |
| `docs/` | Detailed strategy documentation for all bots |

---

## Get Other Bots

This repository only includes the **15-Minute Dump-and-Hedge Bot**.

If you are interested in any of the other strategies:

- 15-Minute Pre-Order & Mid-Market Bot
- 1-Hour Pre-Limit Order Bot
- 1-Hour Pre-Limit Order & Mid-Market Bot
- 5-Minute Pre-Order & Mid-Market Bot
- 5-Minute High-Side Buy Bot
- 5-Minute Low-Side Buy Bot

**Please contact me on Telegram: [@gabagool21](https://t.me/gabagool21)**

---

## Disclaimer

These bots are for educational and research purposes. Trading on prediction markets involves risk. Use at your own risk; the authors are not responsible for financial losses. Always test with `simulation_mode: true` and small sizes before live trading.
