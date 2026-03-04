# 5-Minute Pre-Order and Mid-Market Bot

A Rust trading bot for **Polymarket** that trades **5-minute Up/Down** binary markets. It places **limit BUY orders for both Up and Down** at a low price before the next 5-minute period starts (pre-order), and optionally places mid-market orders in the current period. If both sides fill below $1 total, profit is locked in at resolution.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** 5-minute Up/Down markets for **BTC, ETH, SOL, XRP**
- **Slug pattern:** `{asset}-updown-5m-{period_start_unix}`

---

## Why This Strategy Can Profit

- **Binary outcome:** One of Up or Down pays $1 at expiry. If you buy **both** sides cheap (e.g. 45c each), your total cost is 90c for a **guaranteed** $1 payoff.
- **When the edge appears:** Early in a new 5m period, liquidity is thin and order books can show both sides below 50c. Placing limit orders on both sides captures that mispricing.
- **Selling the loser:** Once one side is clearly winning (e.g. sell price >= 95c) and little time is left, selling the losing side at market cuts exposure. You keep the winning side to expiry ($1).
- **Signal filter:** Placing only when the current market is "stable" (e.g. 35-65c) avoids entering when the market is already one-sided.

---

## Trading Strategy

### Pre-order for next period

In the last `place_order_before_mins` minutes of the **current** market, the bot places limit orders on the **next** period's market (Up and Down at `price_limit`, e.g. 0.45).

Only placed when the current market's signal is **Good** (both sides in a stable range).

### Mid-market (current period)

Optional mid-market orders in the **current** period when signal and time-remaining conditions are met.

### After both fill

If one side's sell price >= `sell_opposite_above` and time remaining <= `sell_opposite_time_remaining_secs`, sell the **losing** side; hold the winning side to expiry ($1).

### Danger handling

If only one side fills and the matched side's price falls to "danger" levels, the bot sells that side or places opposite orders for risk management.

---

## Configuration

### `strategy.strategy1`

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Turn strategy on/off | `true` |
| `price_limit` | Limit price for both Up and Down | `0.45` |
| `shares` | Size per order | `5` |
| `place_order_before_mins` | Minutes before next period to place pre-orders | `1` |
| `sell_opposite_above` | Sell losing side when winner's price >= this | `0.95` |
| `sell_opposite_time_remaining_secs` | Only sell opposite when time left <= this (seconds) | `60` |

### `strategy.signal`

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Use signal to allow/skip placing orders | `true` |
| `stable_min` / `stable_max` | Good signal range | `0.35` / `0.65` |
| `clear_threshold` | Bad if one side >= this with time left | `0.9` |
| `danger_price` | Danger: matched token <= this -> sell early | `0.3` |
| `mid_market_enabled` | Allow mid-market orders in current period | `true` |
