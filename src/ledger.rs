//! Opt-in cashflow and accounting ledger for full auditability of wallet equity.
//!
//! When enabled, every realized PnL fill, trading fee, perpetual funding settlement,
//! liquidation, delist settlement, and bad-debt absorption is recorded as a typed
//! [`LedgerEntry`] carrying the full nominal cashflow, allowing callers to verify the
//! exact accounting identity [`Ledger::reconciles_to`]:
//!
//! `starting_balance + net_cashflow() == current_wallet_equity`
//!
//! where `net_cashflow()` sums every entry. Spelled out by kind:
//!
//! `starting_balance + pnl + fees + funding + delist + liquidation + bad_debt_absorbed
//! == current_wallet_equity`
//!
//! A fill records its realized PnL ([`LedgerEntryKind::RealizedPnl`], or
//! [`LedgerEntryKind::Liquidation`] for a forced close) and its fee
//! ([`LedgerEntryKind::TradingFee`]) as separate entries, so neither amount hides the
//! other. Cashflows are never floored: when a loss exceeds the wallet, the venue's credit
//! that floors the wallet back at zero is a separate positive
//! [`LedgerEntryKind::BadDebtAbsorption`] entry, whose sum equals
//! `Balances::bad_debt` accrued since the ledger was enabled.

use getset::CopyGetters;

use crate::types::{
    MarginCurrency,
    MarketId,
    Mon,
    OrderId,
    TimestampNs,
};

/// Classification of a cashflow entry recorded in [`Ledger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerEntryKind {
    /// Realized PnL from reducing or closing a position.
    RealizedPnl,
    /// Trading fee (negative delta when paid, positive delta for maker rebate).
    TradingFee,
    /// Periodic perpetual funding payment or receipt.
    Funding,
    /// Forced liquidation clearance (realized PnL or liquidation fee).
    Liquidation,
    /// Market delisting settlement cashflow.
    DelistSettlement,
    /// Venue-absorbed bad debt credit flooring equity at zero.
    BadDebtAbsorption,
}

/// Single immutable cashflow entry in [`Ledger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, CopyGetters)]
pub struct LedgerEntry<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Exchange timestamp when the cashflow settled.
    #[getset(get_copy = "pub")]
    timestamp_ns: TimestampNs,

    /// Optional market identifier (`Some` in portfolio mode, `None` in single-market mode).
    #[getset(get_copy = "pub")]
    market_id: Option<MarketId>,

    /// Optional associated order identifier.
    #[getset(get_copy = "pub")]
    order_id: Option<OrderId>,

    /// Category of cashflow.
    #[getset(get_copy = "pub")]
    kind: LedgerEntryKind,

    /// Signed equity delta in margin currency (positive = credit, negative = debit).
    #[getset(get_copy = "pub")]
    delta: Margin,

    /// Running wallet equity of the journal immediately after applying this entry.
    /// It can be negative between a bankrupting cashflow and the following
    /// [`LedgerEntryKind::BadDebtAbsorption`] entry.
    #[getset(get_copy = "pub")]
    equity_after: Margin,

    _i: std::marker::PhantomData<I>,
}

/// Append-only accounting journal tracking every wallet equity change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger<I, const D: u8, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    starting_balance: Margin,
    entries: Vec<LedgerEntry<I, D, Margin>>,
}

impl<I, const D: u8, Margin> Ledger<I, D, Margin>
where
    I: Mon<D>,
    Margin: MarginCurrency<I, D>,
{
    /// Create a new empty ledger anchored at `starting_balance` with pre-allocated capacity.
    #[must_use]
    pub fn with_capacity(starting_balance: Margin, capacity: usize) -> Self {
        Self {
            starting_balance,
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Record a new cashflow entry.
    #[allow(
        clippy::disallowed_methods,
        reason = "Opt-in diagnostic ledger may grow across long backtest runs"
    )]
    pub fn record(
        &mut self,
        timestamp_ns: TimestampNs,
        market_id: Option<MarketId>,
        order_id: Option<OrderId>,
        kind: LedgerEntryKind,
        delta: Margin,
        equity_after: Margin,
    ) {
        self.entries.push(LedgerEntry {
            timestamp_ns,
            market_id,
            order_id,
            kind,
            delta,
            equity_after,
            _i: std::marker::PhantomData,
        });
    }

    /// Borrow all recorded entries in chronological order.
    #[inline]
    #[must_use]
    pub fn entries(&self) -> &[LedgerEntry<I, D, Margin>] {
        &self.entries
    }

    /// Number of recorded entries.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no entries have been recorded.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Starting wallet balance anchored at creation.
    #[inline]
    #[must_use]
    pub fn starting_balance(&self) -> Margin {
        self.starting_balance
    }

    /// Record the non-zero `flows` of one settlement in order, starting from the
    /// running `equity_before`, and return the running equity after the last one.
    ///
    /// Zero flows are skipped, because they carry no information.
    pub(crate) fn record_flows(
        &mut self,
        timestamp_ns: TimestampNs,
        market_id: Option<MarketId>,
        order_id: Option<OrderId>,
        flows: &[(LedgerEntryKind, Margin)],
        equity_before: Margin,
    ) -> Margin {
        let len_before = self.entries.len();
        let mut equity = equity_before;
        for &(kind, delta) in flows {
            if delta.is_zero() {
                continue;
            }
            if kind == LedgerEntryKind::BadDebtAbsorption {
                assert!(
                    delta > Margin::zero(),
                    "the venue only ever credits bad debt"
                );
            }
            equity += delta;
            self.record(timestamp_ns, market_id, order_id, kind, delta, equity);
        }
        assert!(self.entries.len() - len_before <= flows.len());
        equity
    }

    /// Sum of the `delta` of every recorded entry of `kind`.
    #[must_use]
    pub fn total_by_kind(&self, kind: LedgerEntryKind) -> Margin {
        let mut total = Margin::zero();
        for e in self.entries.iter().filter(|e| e.kind == kind) {
            total += e.delta;
        }
        total
    }

    /// Sum of every recorded cashflow `delta`.
    #[must_use]
    pub fn net_cashflow(&self) -> Margin {
        let mut total = Margin::zero();
        for e in &self.entries {
            total += e.delta;
        }
        total
    }

    /// Verify that `starting_balance + net_cashflow == expected_equity`.
    #[must_use]
    pub fn reconciles_to(&self, expected_equity: Margin) -> bool {
        self.starting_balance + self.net_cashflow() == expected_equity
    }
}
