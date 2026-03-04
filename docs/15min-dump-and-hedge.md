# 15-Minute Dump-and-Hedge Bot

A Rust trading bot for **Polymarket** that trades **15-minute Up/Down** binary markets (e.g. "Will BTC go up or down in this 15m window?"). It uses a **dump-and-hedge** strategy: buy the side that just "dumped" in price, then hedge by buying the opposite side when the combined cost is below a target, locking in profit when the market resolves (one side pays $1).

> **Strategy credit:** Based on [The Smart Ape's](https://x.com/the_smart_ape) two-leg catching-and-hedging strategy ([original tweet](https://x.com/the_smart_ape/status/2005576087875527082) · [detailed write-up](https://www.lookonchain.com/articles/1209)). The core idea — detect a sharp dump, buy the dumped side (Leg 1), then hedge with the opposite side when combined cost < $1 (Leg 2) — achieved ~86% ROI in backtesting. This implementation adds configurable stop-loss, multi-asset support, and automatic token redemption.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** 15-minute Up/Down markets for assets such as BTC, ETH, SOL, XRP
- **Strategy:** Dump-and-hedge
- **Modes:** Simulation (no real orders) or Production (live trading); optional redeem-only mode

The bot discovers the current 15m market for each configured asset by slug (e.g. `btc-updown-15m-<period_timestamp>`), monitors bid/ask prices, and executes the strategy per period.

---

## Strategy

### Phases (per 15m period)

1. **Watching for dump**
   For the first **N minutes** of the period (configurable via `dump_hedge_window_minutes`), the bot watches the **Up** and **Down** ask prices.
   - **Dump detection:** If one side's ask drops by at least `dump_hedge_move_threshold` (e.g. 15%) compared to a price from `dump_hedge_dump_lookback_seconds` ago (e.g. 3 seconds), that counts as a "dump."
   - **Leg 1:** On dump, it **buys** that side (Up or Down) at the current ask for `dump_hedge_shares` shares.

2. **Waiting for hedge**
   After leg 1, the bot waits for the **opposite** side's ask to be low enough that:
   - `leg1_entry_price + opposite_ask <= dump_hedge_sum_target` (e.g. <= 0.95).
   When that holds, it buys the opposite side (Leg 2). You then hold one share of each outcome; at resolution one pays $1, so **expected profit = (1 - total_cost) x shares**.

3. **Cycle complete**
   No further action that period. Profit is realized when the market resolves and you redeem the winning side.

4. **Stop loss**
   If the hedge condition is **not** met and the **time remaining** until the period ends is **<=** `dump_hedge_stop_loss_last_remaining_minutes` (e.g. 5 minutes), the bot triggers a stop loss:
   - **`sell_position`:** Sells the Leg 1 position (market sell).
   - **`buy_opposite`:** Buys the opposite side anyway to hedge (default), accepting a worse combined price.

### Summary

- **Entry:** Buy the side that just dumped (large short-term drop in ask).
- **Hedge:** Buy the other side when sum of leg1 price + opposite ask <= `dump_hedge_sum_target`.
- **Exit:** Either cycle complete (both legs filled) or stop loss (sell position or buy opposite).

---

## Configuration

### `trading` section

| Field | Description | Default |
|-------|-------------|---------|
| `check_interval_ms` | How often to poll market data (ms) | `1000` |
| `market_closure_check_interval_seconds` | How often to check market closure and print P&L | `20` |
| `data_source` | Price source: `"api"` or `"websocket"` | `"api"` |
| `markets` | List of assets to trade | `["btc"]` |
| `dump_hedge_shares` | Size in shares per leg | `5.0` |
| `dump_hedge_sum_target` | Hedge when leg1_price + opposite_ask <= this | `0.95` |
| `dump_hedge_move_threshold` | Dump = price drop >= this (e.g. 0.15 = 15%) | `0.15` |
| `dump_hedge_window_minutes` | Only look for dumps in first N minutes of period | `2` |
| `dump_hedge_dump_lookback_seconds` | Look back this many seconds for "old" price when detecting dump | `3` |
| `dump_hedge_stop_loss_last_remaining_minutes` | Trigger stop loss when time left <= this (minutes) | `5` |
| `dump_hedge_stop_loss_percentage` | Used for stop loss valuation (max loss estimate) | `0.25` |
| `stop_loss_management_method` | `"sell_position"` or `"buy_opposite"` | `"sell_position"` |

---

## Build & Run

```bash
cd /path/to/15min-bot-v1
cargo build --release
```

### Simulation

```bash
./target/release/polymarket-arbitrage-bot --simulation
```

### Production

```bash
./target/release/polymarket-arbitrage-bot --production --config config.json
```

### Redeem

```bash
./target/release/polymarket-arbitrage-bot --redeem --config config.json
```

---

## File Layout

| Path | Purpose |
|------|---------|
| `config.json` | Polymarket and trading settings |
| `src/main.rs` | Entry point, CLI, market discovery, redeem |
| `src/dump_hedge_trader.rs` | Dump-and-hedge strategy and state |
| `src/monitor.rs` | Market data (API/WebSocket) and snapshots |
| `src/api.rs` | Polymarket CLOB/Gamma API client |
| `src/config.rs` | Config and CLI parsing |
| `src/models.rs` | Market/token data structures |
