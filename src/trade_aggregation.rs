//! Aggregate lfest trades into candles with the `trade_aggregation` crate.
//!
//! A [`Trade`] is a [`TakerTrade`] with a nanosecond timestamp, and the unit of its size
//! follows the currency of its quantity, so `By::Base` and `By::Quote` sum base volumes
//! and notionals for linear and inverse contracts alike.

use trade_aggregation::{
    SizeUnit,
    TakerTrade,
    TimestampResolution,
    Timestamped,
};

use crate::prelude::*;

impl<I, const D: u8, BaseOrQuote> Timestamped for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const TIMESTAMP_RESOLUTION: TimestampResolution = TimestampResolution::Nanosecond;

    #[inline(always)]
    fn timestamp(&self) -> i64 {
        self.timestamp_exchange_ns.get()
    }
}

impl<I, const D: u8, BaseOrQuote> TakerTrade for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    const SIZE_UNIT: SizeUnit = match BaseOrQuote::KIND {
        CurrencyKind::Base => SizeUnit::Base,
        CurrencyKind::Quote => SizeUnit::Quote,
    };

    #[inline(always)]
    fn price(&self) -> f64 {
        self.price.into()
    }

    #[inline(always)]
    fn size(&self) -> f64 {
        match self.side {
            Side::Buy => self.quantity.into(),
            Side::Sell => self.quantity.neg().into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn a_linear_trade_has_base_sizes_and_nanosecond_timestamps() {
        let trade = Trade {
            price: QuoteCurrency::<i64, 5>::new(100, 0),
            quantity: BaseCurrency::new(5, 0),
            side: Side::Sell,
            timestamp_exchange_ns: 1.into(),
        };
        type T = Trade<i64, 5, BaseCurrency<i64, 5>>;
        assert_eq!(T::TIMESTAMP_RESOLUTION, TimestampResolution::Nanosecond);
        assert_eq!(T::SIZE_UNIT, SizeUnit::Base);
        assert_eq!(Timestamped::timestamp(&trade), 1);
        assert_eq!(TakerTrade::price(&trade), 100.0);
        assert_eq!(TakerTrade::size(&trade), -5.0);
        assert_eq!(trade.base_size(), -5.0);
        assert_eq!(trade.quote_size(), -500.0);
    }

    #[test]
    fn an_inverse_trade_has_quote_sizes() {
        let trade = Trade {
            price: QuoteCurrency::<i64, 5>::new(100, 0),
            quantity: QuoteCurrency::new(500, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 2.into(),
        };
        type T = Trade<i64, 5, QuoteCurrency<i64, 5>>;
        assert_eq!(T::SIZE_UNIT, SizeUnit::Quote);
        assert_eq!(TakerTrade::size(&trade), 500.0);
        assert_eq!(trade.base_size(), 5.0);
        assert_eq!(trade.quote_size(), 500.0);
    }
}
