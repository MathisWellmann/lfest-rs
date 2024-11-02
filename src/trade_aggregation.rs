use crate::prelude::*;

impl<I, const D: u8, BaseOrQuote> trade_aggregation::TakerTrade for Trade<I, D, BaseOrQuote>
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
