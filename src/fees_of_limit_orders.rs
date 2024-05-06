use hashbrown::HashMap;

use crate::types::{Currency, Fee, LimitOrder, MarginCurrency, OrderId, Pending};

/// Cumulative fee of all limit orders.
#[allow(clippy::type_complexity)]
pub(crate) fn fees_of_limit_orders<M, UserOrderId>(
    active_limit_orders: &HashMap<
        OrderId,
        LimitOrder<M::PairedCurrency, UserOrderId, Pending<M::PairedCurrency>>,
    >,
    maker_fee: Fee,
) -> M
where
    M: MarginCurrency,
    M::PairedCurrency: Currency,
    UserOrderId: Clone,
{
    active_limit_orders.values().fold(M::new_zero(), |acc, o| {
        acc + (o.quantity().convert(o.limit_price()) * maker_fee)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        base, fee,
        prelude::QuoteCurrency,
        quote,
        types::{ExchangeOrderMeta, Side},
    };

    #[test]
    fn test_fees_of_limit_orders() {
        let mut orders = HashMap::<OrderId, _>::default();
        let meta = ExchangeOrderMeta::default();
        let maker_fee = fee!(0.0002);
        orders.insert(
            0,
            LimitOrder::new(Side::Buy, quote!(100), base!(2))
                .unwrap()
                .into_pending(meta.clone()),
        );
        assert_eq!(
            fees_of_limit_orders::<QuoteCurrency, ()>(&orders, maker_fee),
            quote!(0.04)
        );
        orders.insert(
            1,
            LimitOrder::new(Side::Buy, quote!(110), base!(1))
                .unwrap()
                .into_pending(meta),
        );
        assert_eq!(
            fees_of_limit_orders::<QuoteCurrency, ()>(&orders, maker_fee),
            quote!(0.062)
        );
    }
}
