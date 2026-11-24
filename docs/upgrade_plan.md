# `lfest-rs` Upgrade Plan: Realistic Multi-Asset Perpetual Futures Exchange

## Context & Motivation

`lfest` v0.141.0 is a fast, deterministic, fixed-point (`const_decimal`) single-symbol perpetual futures simulator. When simulating realistic multi-asset strategies (such as cross-sectional long/short perpetual portfolios across 29+ symbols), the single-symbol isolated-margin engine forces workarounds:

- Running $N$ independent `Exchange` instances with artificial isolated per-symbol budgets instead of a shared cross-margin wallet.
- Emulating stop-loss orders externally on candle closes instead of native venue-side triggers evaluated intrabar.
- Computing static entry-price liquidation thresholds that ignore live mark prices, unrealized PnL, cumulative funding erosion, and tiered maintenance margin brackets.
- Accepting post-only (`GoodTilCrossing`) limit orders only, without `ReduceOnly`, `IOC`, `FOK`, `GTC`, marketable limits, or `CancelAll`.
- Ignoring order-size slippage on market orders and skipping volume depletion in `SmartCandle` (documented `TODO`).

This plan upgrades `lfest` in small, independent, CI-clean revisions while preserving:

1. **Backwards compatibility** for the existing single-symbol `Exchange` API and downstream consumers (`hydra`, `agent-symbiont`).
2. **Zero-allocation hot paths** (`#![feature(vec_push_within_capacity)]`, bounded pre-allocated buffers, dense `MarketId(u16)` indexing, lazy zero-cost stop store when no stops are active).
3. **Exact fixed-point arithmetic** (`const_decimal::Decimal<I, D>`, `BaseCurrency` / `QuoteCurrency`, linear and inverse contracts).
4. **Heavy assertions and property/edge-case test coverage**.

---

## Algorithmic Reference: `OrderBook-rs` Adaptation

`joaquinbejar/OrderBook-rs` was inspected as a reference for production order-management mechanics. Because `OrderBook-rs` is a concurrent multi-trader matching engine using `u128` raw ticks, `DashMap`, and lock-free `SkipMap`, whereas `lfest` is a single-threaded, zero-allocation market-update-driven futures simulator using `const_decimal`, we adapt the *algorithms and invariants* without copying concurrent containers:

| `OrderBook-rs` pattern | `lfest` adaptation |
|---|---|
| `stop_orders.rs` — lazy `PendingStops` (zero overhead when empty), dual ordering (trigger price + watermark), monotonic print-segment evaluation (`prev -> first -> last`), cascade-safe election queue, `StopWouldTrigger` admission check | Single-threaded pre-allocated `PendingStops` per market keyed by `(trigger_price, OrderId)` / linear scan for small active counts (matching `SortedOrders`), supporting `StopMarket`, `StopLimit`, `TakeProfitMarket`, `TakeProfitLimit`, and `TrailingStop` driven by `Last`, `Mark`, or `Bba` (`TriggerPriceType`). Conservative intrabar sweep over candle `[low, high]`. |
| `stop_protection.rs` — `StopProtection` collar (`stop ± collar`) bounding slippage on elected stop-market orders | `StopProtection<I, D>` price collar on `ContractSpecification` / per-stop override; elected stop-market orders execute as collared orders, cancelling if market gaps outside the protection band. |
| `repricing.rs` — crossing / post-only / marketable-limit / peg-clamp semantics | Complete `RePricing::Marketable` (crossing limit executes immediately at top-of-book with taker fee, optional remainder rests under `GTC`, or cancels under `IOC`/`FOK`) alongside `GoodTilCrossing` (post-only). |
| `market_impact.rs` — structured `MarketImpact` (average fill price, worst price, slippage bps, unfilled quantity) | Pluggable `ImpactModel` / `SlippageModel` trait returning a `FillQuote<I, D, BaseOrQuote>` (effective fill price, slippage, filled quantity) with built-in `ZeroImpact`, `FixedBpsSlippage`, and `LinearSqrtImpact` (spread + linear/sqrt participation) models. |
| `mass_cancel.rs` — atomic `CancelAll` / cancel-by-side / cancel-by-market with deterministic ordering and structured `MassCancelResult` | `cancel_all_limit_orders`, `cancel_limit_orders_by_side` on `Exchange` and per-market / all-market mass cancel on `PortfolioExchange`, returning cancelled orders deterministically. |
| `fees.rs` — signed maker fee (negative = rebate), non-negative taker fee | Keep `Fee<I, D, Maker>` signed (negative = rebate, already reserved as `maker_fee.max(0)` in `Account`), validate `Fee<I, D, Taker> >= 0`, add liquidation clearance fee rate. |
| `risk.rs` — pre-trade notional / position / order-size checks and tiered risk validation | `MinNotional` filter on `ContractSpecification`, tiered `MaintenanceMarginSchedule` (`MaintenanceMarginBracket`), and `CrossMarginRiskEngine` checking shared equity + live mark-to-market unrealized PnL across all legs. |

---

## Phased Revision Roadmap

Each milestone below lands in its own `jj` revision passing `cargo check`, `cargo test --all-features`, `cargo clippy` (`RUSTFLAGS="-D warnings"`), `cargo bench --no-run`, `cargo fmt --check`, `cargo doc`, and Nix checks.

### Revision 1 — Upgrade Plan Document
- Record this architecture and revision plan in `docs/upgrade_plan.md`.

### Revision 2 — Mark Price Wiring, `MinNotional` Filter, Maker Rebates & Liquidation Fee
- **Wire `MarkMethod` into `MarketState` and valuation**:
  - Add optional `fair_price: Option<QuoteCurrency<I, D>>` to `MarketState` plus `set_fair_price` / `mark_price(method: MarkMethod, side_hint)` and a dedicated `MarkPriceUpdate` market update type (with `CAN_FILL_LIMIT_ORDERS = false`).
  - Add `Position::unrealized_pnl_at_mark(mark_price)` and `Position::notional_at_price(price)` while preserving existing `unrealized_pnl(bid, ask)` and `notional()`.
  - Use `mark_price` in `PriceFilter::validate_limit_price` and valuation helpers.
- **`MinNotional` order filter**:
  - Add `NotionalFilter<I, D, MarginCurrency>` (optional `min_notional`, optional `max_notional`) on `ContractSpecification` (default `None` for backwards compatibility), enforced on `submit_market_order` and `submit_limit_order` (bypassed for strictly position-reducing orders so a trader can always flatten dust positions).
- **Fee realism**:
  - Explicitly verify and test signed maker rebates (`Fee<I, D, Maker>` negative -> credits wallet equity, `total_fees_paid` reflects net fees, zero fee reserve in `Account`).
  - Add `liquidation_fee: Decimal<I, D>` (clearance fee, default `0` for backwards compatibility, configurable via setter/builder) applied on `force_liquidate` in addition to taker fee.

### Revision 3 — Cross-Margin `RiskEngine` & Tiered Maintenance Margin Brackets
- **Tiered Maintenance Margin Schedule**:
  - Introduce `MaintenanceMarginTier<I, D, Margin>` (`notional_cap`, `maintenance_margin_rate`, `initial_margin_rate`, `maintenance_amount`) and `MaintenanceMarginSchedule<I, D, Margin>` (Binance/Bybit piecewise-continuous bracket formula: `MMR(notional) = notional * mm_rate - maintenance_amount`).
  - Single-tier flat schedule wraps the existing `ContractSpecification::maintenance_margin()` seamlessly.
- **Live Mark-to-Market Margin Accounting**:
  - Add `MarginMode::{Isolated, Cross}` and `CrossMarginRiskEngine` implementing equity + mark-to-market unrealized PnL checks:
    - `total_equity = wallet_balance + sum(unrealized_pnl_i)`
    - `total_maintenance_margin = sum(tier_mm(mark_notional_i))`
    - `total_initial_margin = sum(pos_im_i + order_margin_i + reserved_maker_fees_i)`
    - Liquidate when `total_equity < total_maintenance_margin`; reject risk-increasing orders when `total_equity < total_initial_margin_after`.
  - Expose `liquidation_price` calculation helper for both isolated and cross-margin positions.
  - Keep existing `Exchange::new` defaulting to `IsolatedMarginRiskEngine` so all existing tests pass unchanged, while adding `Exchange::new_with_margin_mode` / opt-in mark-to-market liquidation.

### Revision 4 — Multi-Symbol `PortfolioExchange`, `MarketId(u16)`, Delisting & Mass-Cancel
- **`MarketId(u16)` newtype**: dense `Copy + Eq + Hash + Ord` market identifier indexing directly into contiguous `Vec<MarketSlot<...>>` (no `HashMap` lookup on the hot path), with ticker lookup table `ticker -> MarketId` for cold-path ergonomics.
- **`PortfolioExchange<I, const D: u8, BaseOrQuote, UserOrderIdT>`**:
  - Shared `Balances<I, D, BaseOrQuote::PairedCurrency>` across all markets (cross-margin).
  - Per-market `MarketState`, `ContractSpecification`, `Position`, `ActiveLimitOrders`, `OrderRateLimiter`, and `MarketStatus::{Active, ReduceOnly, Delisted}`.
  - Shared monotonic `OrderId` sequence, pre-allocated per-call event buffers (`PortfolioOrderEvent` tagged with `MarketId`).
  - Portfolio-wide `update_market(market_id, &update)`, `apply_funding(market_id, rate, mark_price)`, `submit_market_order(market_id, order)`, `submit_limit_order(market_id, order)`, `amend_limit_order(market_id, id, new_order)`, `cancel_limit_order(market_id, by)`.
  - **Margin call & liquidation waterfall** across the portfolio: cancel largest collateral-contributing resting orders across markets first, then liquidate positions if `total_equity < total_maintenance_margin` (with full or incremental liquidation policy).
  - **Lifecycle management**: `set_market_status(market_id, MarketStatus::ReduceOnly)` and `delist_market(market_id, settlement_price)` (cancels all resting orders on that market, force-settles any open position at `settlement_price` with zero fee, returns `DelistSettlement`).
- **Mass cancel (`CancelAll` / by side / by market)**:
  - Add `cancel_all_limit_orders` and `cancel_limit_orders_by_side(side)` on both `Exchange` and `PortfolioExchange` (plus `cancel_all_across_markets`), returning a structured `MassCancelResult`.

### Revision 5 — Order Flags & Time-in-Force (`ReduceOnly`, `GTC`/`GTX`/`IOC`/`FOK`, `RePricing::Marketable`)
- **`TimeInForce` enum**:
  - `GoodTilCancelled` (`GTC`) — rests on book after optional immediate marketable fill.
  - `GoodTilCrossing` (`GTX` / Post-Only) — rejects if marketable.
  - `ImmediateOrCancel` (`IOC`) — fills marketable portion immediately, cancels any unexecuted remainder without resting.
  - `FillOrKill` (`FOK`) — executes entirely immediately or cancels untouched.
- **`RePricing::Marketable`**:
  - Completes the existing `TODO` in `re_pricing.rs`: crossing limit orders fill immediately against top-of-book at best bid/ask (capped by `limit_price`) paying taker fee, settling via a unified `LimitOrderSubmission` result (keeping `submit_limit_order` compatible and adding `submit_limit_order_with_execution` or richer submission outcome where marketable fills can occur).
- **`ReduceOnly` flag on `MarketOrder` and `LimitOrder`**:
  - Rejects if no opposite-side position exists.
  - Clamps or rejects (configurable / standard venue rule: reject if neutral or same-side; clip excess qty on market orders or reject if exceeding open position, and auto-cancel/clip resting `ReduceOnly` limit orders when the open position shrinks or closes).
  - Zero extra margin reservation for strictly reducing orders.

### Revision 6 — Native Conditional & Stop Orders
- **Conditional order types**:
  - `StopMarket`, `StopLimit`, `TakeProfitMarket`, `TakeProfitLimit`, `TrailingStop` (with fixed offset or percentage callback rate).
  - `TriggerPriceType::{Last, Mark, BestBidAsk}` — choose which price triggers the order.
  - `StopProtection<I, D>` collar (adapted from `OrderBook-rs`) capping slippage on triggered stop-market orders.
  - `ReduceOnly` support on conditional orders (ideal for protective stop-losses and take-profits).
- **Lazy `PendingStops` store**:
  - Zero work on `update_state` when no stops are active (`if self.pending_stops.is_empty() { return; }`).
  - Pre-allocated capacity bounded by `Config` (`max_num_open_stops` or `max_num_open_orders`).
  - Admission check rejecting stops that would trigger immediately (`StopWouldTrigger`), matching `OrderBook-rs`.
  - Monotonic update evaluation:
    - `Trade` / `Bba` / `MarkPriceUpdate`: trail watermarks first, then elect crossed stops in deterministic trigger-price / admission-order sequence.
    - `Candle` / `SmartCandle`: conservative intrabar ordering (check adverse extreme for stop-losses before favorable extreme for take-profits; fill price = `worst(stop_price, gap_open_or_extreme)` bounded by `StopProtection` collar).
  - Cascade-safe execution in both `Exchange` and `PortfolioExchange`, emitting structured `StopOrderEvent`s (`Triggered`, `Filled`, `PlacedLimit`, `Cancelled`).

### Revision 7 — Fill Realism, Slippage / Market Impact & Cashflow Ledger
- **Fix `SmartCandle` volume depletion (`TODO` in `src/market_update/smart_candle.rs`)**:
  - Decrement cumulative available taker volume in `aggregate_sell_volume` / `aggregate_buy_volume` as multiple resting limit orders fill within the same `SmartCandle`, returning `exhausted = true` once depleted.
- **Volume-aware / participation-capped `Candle` fills**:
  - Add optional `volume` and `participation_rate` support (e.g. `VolumeCandle` or optional volume field/helper) so candle fills cannot exceed `participation_rate * bar_volume`.
- **Pluggable `SlippageModel` / `ImpactModel` trait**:
  - `ZeroImpact` (default, exact existing behavior).
  - `FixedBpsSlippage` (constant bps adverse slippage on market orders).
  - `LinearSqrtImpact` (half-spread + linear + square-root participation impact given reference bar/ADV volume and volatility, producing a `MarketImpact` summary with avg fill price, worst price, and slippage bps).
- **Optional Cashflow / Accounting Ledger**:
  - Opt-in `Ledger<I, D, Margin>` recording every `Fill`, `Fee`, `Funding`, `Liquidation`, `DelistSettlement`, and `BadDebt` entry with timestamp and running equity invariant check (`starting_balance + sum(cashflows) - bad_debt_offset == equity`).

---

## Verification Strategy

For every revision:
1. Unit tests covering happy paths, invalid inputs, boundary conditions, linear (`QuoteCurrency` margin) and inverse (`BaseCurrency` margin) contracts, and struct-size invariants.
2. Property tests (`proptest`) on margin accounting, cross-margin invariants, stop-order monotonicity, and `SmartCandle` volume conservation.
3. Full CI suite:
   - `RUSTFLAGS="-D warnings" nix develop .#ci --command cargo check`
   - `RUSTFLAGS="-D warnings" nix develop .#ci --command cargo test --all-features`
   - `RUSTFLAGS="-D warnings" nix develop .#ci --command cargo clippy --all-targets --all-features`
   - `RUSTFLAGS="-D warnings" nix develop .#ci --command cargo bench --no-run`
   - `nix develop .#ci --command cargo fmt --check`
   - `RUSTFLAGS="-D warnings" nix develop .#ci --command cargo doc --no-deps`
   - `nix flake check` (plus `deadnix`, `statix check`, `alejandra --check .`, `yamlfmt`)
