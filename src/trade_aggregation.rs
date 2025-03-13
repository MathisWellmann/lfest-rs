use trade_aggregation::TakerTrade;

use crate::prelude::*;

impl<I, const D: u8, BaseOrQuote> TakerTrade for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    #[inline(always)]
    fn timestamp(&self) -> i64 {
        *self.timestamp_exchange_ns.as_ref()
    }

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

impl<I, const D: u8, BaseOrQuote> Into<trade_aggregation::Trade> for Trade<I, D, BaseOrQuote>
where
    I: Mon<D>,
    BaseOrQuote: Currency<I, D>,
{
    #[inline]
    fn into(self) -> trade_aggregation::Trade {
        trade_aggregation::Trade {
            timestamp: *self.timestamp_exchange_ns.as_ref(),
            price: self.price.into(),
            size: <Trade<I, D, BaseOrQuote> as TakerTrade>::size(&self),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn taker_trade() {
        let trade = Trade {
            price: QuoteCurrency::<i64, 5>::new(100, 0),
            quantity: BaseCurrency::new(5, 0),
            side: Side::Buy,
            timestamp_exchange_ns: 1.into(),
        };
        assert_eq!(trade.size(), 5.0);
        assert_eq!(
            <Trade<i64, 5, BaseCurrency<i64, 5>> as TakerTrade>::size(&trade),
            5.0
        );
        assert_eq!(trade.price(), 100.0);
        assert_eq!(
            <Trade<i64, 5, BaseCurrency<i64, 5>> as TakerTrade>::price(&trade),
            100.0
        );
        assert_eq!(trade.timestamp(), 1_i64);

        let t: trade_aggregation::Trade = trade.into();
        assert_eq!(
            t,
            trade_aggregation::Trade {
                price: 100.0,
                size: 5.0,
                timestamp: 1,
            }
        );
    }
}
