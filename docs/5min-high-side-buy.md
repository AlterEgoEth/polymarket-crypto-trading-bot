# 5-Minute High-Side Buy Bot

A Rust trading bot for **Polymarket** that trades **5-minute Up/Down** binary markets. When one side is trading **above a high threshold** (e.g. 90c+), the bot buys that side (the likely winner) late in the period. It manages risk by selling or hedging if the price reverses.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** 5-minute Up/Down markets for **BTC, ETH, SOL, XRP**
- **Slug pattern:** `{asset}-updown-5m-{period_start_unix}`

---

## Why This Strategy Can Profit

- **Momentum / information:** When one side trades at 90c+ with meaningful time left, the market is heavily betting that outcome. Often that reflects real information (price near strike, trend, order flow). Buying that side is a bet that the consensus is **right**.
- **Expected value:** At 90c, you need the outcome to happen only ~90% of the time to break even ($1 x 0.9 = 0.90). If the market is right more often than that, buying the favorite is positive EV.
- **Risk control:** If the price **drops** (e.g. below 70c), the bot sells or hedges. This caps losses instead of holding to a possible $0.

---

## Trading Strategy

### When the order is placed

- Only in the **current** 5m window.
- **Time:** At least `after_secs` (e.g. 240s) after the period start, and at least `min_remaining_secs` (e.g. 30s) before period end.
- **Price:** One side's **BUY (bid)** price >= `threshold` (e.g. 0.9). If both sides meet it, the higher-priced side is chosen.
- **Action:** One **limit BUY** (or **market BUY** if `use_market_order: true`) at `limit_price` (e.g. 0.95). At most one high-side order per asset per period.

### After the order is filled

**Two modes:**

1. **Sell under (no hedge):** `buy_opposite_enabled: false`. When the SELL price of the position drops below `sell_under_price` (e.g. 0.70), the bot **sells** at market.

2. **Hedge (opposite):** `buy_opposite_enabled: true`. At order time it also places an **opposite** limit buy at `opposite_limit_price` (e.g. 2c). If the high-side price later drops below `sell_under_price` and the first hedge didn't fill, it places a **second** hedge.

---

## Configuration (per asset)

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Turn on/off for this asset | `true` |
| `after_secs` | Wait this many seconds after market start | `240` |
| `threshold` | Min bid price for a side to trigger | `0.9` |
| `limit_price` | Limit order price | `0.95` |
| `use_market_order` | `true` = market buy, `false` = limit buy | `true` |
| `shares` | Size per order | `100` |
| `min_remaining_secs` | Require at least this many seconds left | `30` |
| `sell_under_price` | Sell or hedge when position's sell price drops below this | `0.70` |
| `buy_opposite_enabled` | If true, hedge with opposite side; if false, just sell | `false` |
| `opposite_limit_price` | Price for opposite (hedge) limit order | `0.02` |
| `opposite_shares` | Size of opposite order | `50` |
| `second_buy` | Second hedge: limit (`true`) or market (`false`) | `false` |
