# 1-Hour Pre-Limit Order Bot

A Rust trading bot for **Polymarket** that trades **1-hour Up/Down** binary markets (e.g. "Will BTC go up or down this hour?"). It places **limit BUY orders** on both Up and Down at a fixed price before the next hourly period. When both fill, it **merges** positions to lock in profit.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** **1-hour** Up/Down markets for **BTC, ETH, SOL, XRP**
- **Time:** Hour boundaries in **Eastern Time (ET)** (e.g. 4pm-5pm, 5pm-6pm)
- **Slug format:** `{asset}-up-or-down-{month}-{day}-{hour}{am|pm}-et`

---

## Trading Strategy

### Pre-order for the next hour

- **When:** Time until the **next** 1h period <= `place_order_before_mins` (e.g. 1 minute).
- **Condition:** The **current** (running) market must have a **Good** signal. If **Bad**, the bot **skips** placing orders.
- **Action:** Look up the next hour's market by slug; place two **limit BUY** orders:
  - Up: `price_limit` (e.g. $0.45), `shares`
  - Down: same price and size

### Both sides filled -> merge

When **Up** and **Down** are both matched, the bot **merges** positions (redeem both outcomes back to USDC). This locks in the profit without waiting for market resolution.

**Profit:** `shares x ($1 - cost_per_share_up - cost_per_share_down)`

### Only one side filled -> risk exit

- **Danger exit:** If the matched token's SELL price <= `danger_price` (e.g. 0.15), sell that token and cancel the other order immediately.
- **Timeout exit:** If `sell_unmatched_after_mins` (e.g. 57) have passed since the market start, sell the matched token and cancel the other order.

### Signal logic

- **Good:** Both Up and Down in `[stable_min, stable_max]` (e.g. [0.35, 0.65]) and not "clear."
- **Bad:** One side >= `clear_threshold` with time remaining > `clear_remaining_mins`.

---

## Configuration

### `strategy` section

| Field | Description | Example |
|-------|-------------|---------|
| `price_limit` | Limit price for pre-orders (both sides) | `0.45` |
| `shares` | Shares per side | `70` |
| `place_order_before_mins` | Place pre-orders this many minutes before next hour | `1` |
| `sell_unmatched_after_mins` | Sell single-filled side after this many minutes | `57` |
| `check_interval_ms` | Main loop interval (ms) | `1000` |
| `simulation_mode` | If true, no real orders | `false` |

### `strategy.signal`

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Use signal to allow/skip placing orders | `true` |
| `stable_min` / `stable_max` | Good signal range | `0.35` / `0.65` |
| `clear_threshold` | Bad if one side >= this with time left | `0.99` |
| `clear_remaining_mins` | Bad when time remaining > this | `15` |
| `danger_price` | Danger: matched token <= this -> sell early | `0.15` |

---

## Risk Management

| Situation | Action |
|-----------|--------|
| **Both filled** | Merge positions (lock profit). |
| **Only one filled, matched price <= danger_price** | Sell matched token, cancel other (danger exit). |
| **Only one filled, timeout reached** | Sell matched token, cancel other (timeout exit). |
| **Bad signal** | Do not place pre-orders for next hour. |
