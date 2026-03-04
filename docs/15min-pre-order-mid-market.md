# 15-Minute Pre-Order and Mid-Market Bot

A Rust trading bot for **Polymarket** that trades **15-minute Up/Down** binary markets. It places **limit BUY orders for both Up and Down** at a low price before the next period starts (pre-order), and optionally places mid-market orders in the current period. If both sides fill below $1 total, profit is locked in at resolution.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** 15-minute Up/Down markets for **BTC, ETH, SOL, XRP**
- **Time:** 15m periods aligned to **Eastern Time (ET)** (:00, :15, :30, :45)
- **Modes:** Simulation or live trading; optional redeem-only mode

---

## Trading Strategy

### Pre-order for next period

When **time until next 15m period** <= `place_order_before_mins` (e.g. 2 minutes), the bot looks up the **next** period's market and places two limit BUY orders:
- Up token: `price_limit` (e.g. $0.45), `shares`
- Down token: same price and size

Placed only if the **current** market has a **Good** signal. If signal is **Bad**, pre-orders are **skipped**.

### Mid-market (current period)

When there is **enough time left** in the current period and **signal is Good**, the bot places limit orders on the **current** period's market:
- Prices are derived from current Up/Down SELL prices so the pair sums to ~0.98 (e.g. Up @ 0.40, Down @ 0.58).

### After orders are placed

- **Both filled:** Optionally **sell the losing side** when one side's SELL price >= `sell_opposite_above` (e.g. 0.9) and time remaining <= `sell_opposite_time_remaining` (e.g. 2 minutes). Hold the winning side to expiry ($1).
- **Sell winner below:** After selling the loser, if the winner's SELL price drops below `sell_winner_below`, sell the winner to realize PnL early.
- **Only one side filled (risk):**
  - **Timeout:** After `sell_unmatched_after_mins` (e.g. 8), sell the matched side and cancel the other order.
  - **Danger (price):** If matched token's SELL price <= `danger_price` (e.g. 0.3), sell immediately.
  - **Danger (time):** If only one side matched for `danger_time_passed` minutes, sell and cancel.

### Signal logic

- **Good:** Both Up and Down SELL prices in `[stable_min, stable_max]` (e.g. [0.35, 0.65]) -> place orders.
- **Bad:** One side >= `clear_threshold` (e.g. 0.9) and time remaining > `clear_remaining_mins` (e.g. 5) -> skip.

---

## Configuration

### `strategy` section

| Field | Description | Example |
|-------|-------------|---------|
| `strategy1_enabled` | Enable pre-order / mid-market strategy | `true` |
| `price_limit` | Limit price for both sides | `0.45` |
| `shares` | Shares per side | `5` |
| `place_order_before_mins` | Place pre-orders this many minutes before next period | `2` |
| `sell_unmatched_after_mins` | Sell single-filled side after this many minutes | `8` |
| `sell_opposite_above` | Sell loser when winner price >= this | `0.9` |
| `sell_opposite_time_remaining` | ...and time remaining <= this (minutes) | `2` |
| `sell_winner_below` | Sell winner if its price < this (0 = disabled) | `0.7` |

### `strategy.signal`

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Use signal to allow/skip placing orders | `true` |
| `stable_min` / `stable_max` | Good signal range | `0.35` / `0.65` |
| `clear_threshold` | Bad if one side >= this with time left | `0.9` |
| `clear_remaining_mins` | Bad when time remaining > this | `5` |
| `danger_price` | One-side risk: sell when matched token <= this | `0.3` |
| `danger_time_passed` | One-side risk: sell after this many minutes | `8` |
| `mid_market_enabled` | Allow mid-market orders in current period | `true` |

---

## Summary Table

| Component | Description |
|-----------|-------------|
| **Pre-order** | Limit BUY Up + Down before next period. Profit when both fill and sum < $1. |
| **Mid-market** | Same idea, placed in current period using dynamic pricing. |
| **Signal** | Good = both in stable range. Bad = one side dominant. Only place when Good. |
| **Sell opposite** | When both filled: sell loser when winner price high and time short. |
| **One-side risk** | Only one filled: timeout sell, danger price sell, or danger time sell. |
