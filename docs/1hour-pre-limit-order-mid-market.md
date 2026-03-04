# 1-Hour Pre-Limit Order and Mid-Market Bot

A Rust trading bot for **Polymarket** that trades **1-hour Up/Down** binary markets. It places **limit BUY orders** on both Up and Down at a fixed price (pre-order) **and** dynamically priced mid-market orders in the current period. When both fill, it **merges** positions to lock in profit.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** **1-hour** Up/Down markets for **BTC, ETH, SOL, XRP**
- **Time:** Hour boundaries in **Eastern Time (ET)**
- **Slug format:** `{asset}-up-or-down-{month}-{day}-{hour}{am|pm}-et`

---

## Trading Strategy

### A. Pre-order for the next hour

- **When:** Time until the **next** 1h period <= `place_order_before_mins` (e.g. 1 minute).
- **Condition:** Current market signal must be **Good**. If **Bad**, skip.
- **Action:** Place two **limit BUY** orders (Up + Down at `price_limit`).

### B. Mid-market (current hour)

- **When:** No existing state for the current hour, not in the pre-order window, and signal is **Good**.
- **Action:** Place limit orders with **dynamic pricing**:
  - **Cheaper side:** limit = current SELL price (e.g. Down $0.38).
  - **Opposite side:** limit = current SELL price - `opposite_side_discount` (e.g. $0.02 discount).

### Both sides filled -> merge

When both **Up** and **Down** are matched, the bot merges positions: calls Polymarket/CTF merge (redeem both outcomes back to USDC), locking in the profit immediately.

### Only one side filled -> risk exit

- **Danger exit:** Matched token's SELL price <= `danger_price` (e.g. 0.15) -> sell and cancel.
- **Timeout exit:** After `sell_unmatched_after_mins` (e.g. 57) -> sell and cancel.

### Signal logic

- **Good:** Both Up and Down SELL prices in `[stable_min, stable_max]` (e.g. [0.35, 0.65]).
- **Bad:** One side >= `clear_threshold` (e.g. 0.99) with time remaining > `clear_remaining_mins` (e.g. 15).

---

## Configuration

### `strategy` section

| Field | Description | Example |
|-------|-------------|---------|
| `price_limit` | Limit price for pre-orders (both sides) | `0.45` |
| `shares` | Shares per side | `70` |
| `place_order_before_mins` | Minutes before next hour to place pre-orders | `1` |
| `sell_unmatched_after_mins` | Sell single-filled side after this many minutes | `57` |
| `opposite_side_discount` | Mid-market: discount on the more expensive side | `0.02` |
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
| `mid_market_enabled` | Allow mid-market orders in current hour | `true` |

---

## Risk Management

| Situation | Action |
|-----------|--------|
| **Both filled** | Merge positions (lock profit). |
| **Only one filled, price <= danger_price** | Sell matched, cancel other (danger exit). |
| **Only one filled, timeout reached** | Sell matched, cancel other (timeout exit). |
| **Bad signal** | Do not place orders. |
| **Good signal** | Allow pre-orders (next hour) or mid-market (current hour). |

---

## Summary Flow

1. If within `place_order_before_mins` of next hour and signal Good -> place pre-orders (Up + Down at `price_limit`).
2. If no orders for current hour, `mid_market_enabled`, and signal Good -> place mid-market orders (dynamic pricing).
3. Check fills. Both filled -> merge. One filled -> check danger or timeout -> sell and cancel.
4. Market expired -> clear state.
