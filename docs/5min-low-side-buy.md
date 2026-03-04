# 5-Minute Low-Side Buy Bot

A Rust trading bot for **Polymarket** that trades **5-minute Up/Down** binary markets. It places **limit buys at very low prices** (e.g. 1c, 2c, 3c) on **both** Up and Down to catch reversals. Risk is tiny per share; reward is up to 99c if that side wins. Take-profit tiers lock in gains as the market reprices.

---

## Overview

- **Platform:** [Polymarket](https://polymarket.com) (CLOB API + Gamma API)
- **Markets:** 5-minute Up/Down markets for **BTC, ETH, SOL, XRP**
- **Slug pattern:** `{asset}-updown-5m-{period_start_unix}`

---

## Why This Strategy Can Profit

- **Asymmetric payoff:** Buying at 1c means you risk 1c to make up to **99c** if that side wins ($1 - price). You only need a ~1% win rate to break even.
- **Reversals and fat tails:** In 5-minute markets, the underlying (e.g. BTC) can move sharply. The "losing" side early in the period can become the winner by expiry. Placing limit buys at 1c/2c/3c on **both** sides means you don't have to predict direction — you're buying cheap optionality on both outcomes.
- **Take-profit tiers:** When a cheap position moves to 10c or 15c, selling part or all locks in a 5-15x multiple of your entry. You don't have to hold to expiry to profit.
- **Cancel unfilled:** Cancelling unfilled orders in the last N seconds avoids stale orders into settlement.

---

## Trading Strategy

### Placement

After `after_secs` seconds into the market, the bot places limit buys at **each** price in `entry_prices` (e.g. `[0.01, 0.02, 0.03]`) for **both** Up and Down. Placement is **once per market**.

**Order count example:** For `entry_prices: [0.01, 0.02]` -> 2 prices x 2 sides = **4** orders per market (each at `shares_per_entry`).

### Cancel unfilled

In the last `cancel_unfilled_last_secs` seconds of the market, any **unfilled** low-side limit order is cancelled. Set to `0` to disable.

### Take-profit tiers

When filled, the bot applies take-profit tiers (sell a % of the position when sell price reaches tier price).

**Example:**

```json
"take_profit_tiers": [
  { "price": 0.10, "sell_pct": 50 },
  { "price": 0.15, "sell_pct": 100 }
]
```

- When sell price >= 10c: sell **50%** of original size.
- When sell price >= 15c: sell the **remaining 100%** (the rest).

---

## Configuration (per asset)

| Field | Description | Example |
|-------|-------------|---------|
| `enabled` | Turn on/off for this asset | `true` |
| `entry_prices` | List of limit prices per side | `[0.01, 0.02, 0.03]` |
| `shares_per_entry` | Size per order at each entry price | `50` |
| `after_secs` | Wait this many seconds after market start | `10` |
| `cancel_unfilled_last_secs` | Cancel unfilled orders in last N seconds; `0` = don't cancel | `30` |
| `take_profit_tiers` | `price` (sell when >= this), `sell_pct` (% of original to sell) | see above |
