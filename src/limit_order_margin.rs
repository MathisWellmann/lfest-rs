use crate::{
    prelude::Position,
    types::{Currency, Fee, MarginCurrency, Order, Side},
    utils::{max, min},
};

/// Compute the needed order margin with a newly added order
///
/// # Arguments:
/// `orders`: All the open orders
/// `pos_size`: The current position size
/// `leverage`: The positions leverage
/// `fee_maker`: Fee of the maker
///
/// # Returns:
/// The margin required for those orders, measured in the margin currency which
/// is the pair of the order size currency.
///
/// TODO: rework this, its too complex
pub(crate) fn order_margin<S>(
    orders: impl Iterator<Item = Order<S>>,
    position: &Position<S>,
    fee_maker: Fee,
) -> S::PairedCurrency
where
    S: Currency,
    S::PairedCurrency: MarginCurrency,
{
    let mut cumulative_buy_value = S::PairedCurrency::new_zero();
    let mut cumulative_sell_value = S::PairedCurrency::new_zero();
    let mut buy_side_fees = S::PairedCurrency::new_zero();
    let mut sell_side_fees = S::PairedCurrency::new_zero();
    for o in orders {
        let limit_price = o.limit_price().expect("Limit price must exist; qed");
        let fee_margin = o.quantity().convert(limit_price).fee_portion(fee_maker);
        match o.side() {
            Side::Buy => {
                cumulative_buy_value += o.quantity().convert(limit_price);
                buy_side_fees += fee_margin;
            }
            Side::Sell => {
                cumulative_sell_value += o.quantity().convert(limit_price);
                sell_side_fees += fee_margin;
            }
        }
    }

    let pos_value = if position.size().is_zero() {
        S::PairedCurrency::new_zero()
    } else {
        position.size().convert(position.entry_price())
    };
    let bsd = cumulative_buy_value - min(pos_value, S::PairedCurrency::new_zero()).abs();
    let ssd = cumulative_sell_value - max(pos_value, S::PairedCurrency::new_zero());
    dbg!(bsd, ssd);

    let fees = if ssd > bsd {
        sell_side_fees
    } else if ssd < bsd {
        buy_side_fees
    } else {
        S::PairedCurrency::new_zero()
    };

    let order_margin = max(bsd, ssd);

    order_margin / position.leverage().inner() + fees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn order_margin_linear_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p = Position::new_init(Leverage::new(l));
            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(0.0));

            let orders =
                vec![Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders =
                vec![Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l / Decimal::TWO))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l / Decimal::TWO))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));
        }
    }

    #[test]
    fn order_margin_linear_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p = Position::new(BaseCurrency::new(l), quote!(100.0), Leverage::new(l), quote!(0));

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(0.0));

            let orders =
                vec![Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders =
                vec![Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(0.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l / Decimal::TWO))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));
        }
    }

    #[test]
    fn order_margin_linear_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p =
                Position::new(BaseCurrency::new(-l), quote!(100.0), Leverage::new(l), quote!(0));

            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(0.0));

            let orders =
                vec![Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(0.0));

            let orders =
                vec![Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), BaseCurrency::new(l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l / Decimal::TWO))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), BaseCurrency::new(l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), quote!(150.0));
        }
    }

    #[test]
    fn order_margin_inverse_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p = Position::new(quote!(0), quote!(100.0), Leverage::new(l), base!(0));

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(0.0));

            let orders = vec![Order::limit(
                Side::Buy,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![Order::limit(
                Side::Sell,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(50) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.5));
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p = Position::new(
                QuoteCurrency::new(l * Decimal::from(100)),
                quote!(100.0),
                Leverage::new(l),
                base!(0),
            );

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(0.0));

            let orders = vec![Order::limit(
                Side::Buy,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![Order::limit(
                Side::Sell,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(0.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(50) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = fee!(0.0);

        for l in (1..5).map(|v| Decimal::from(v)) {
            let p = Position::new(
                QuoteCurrency::new(-l * Decimal::from(100)),
                quote!(100.0),
                Leverage::new(l),
                base!(0),
            );

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(0.0));

            let orders = vec![Order::limit(
                Side::Buy,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(0.0));

            let orders = vec![Order::limit(
                Side::Sell,
                quote!(100.0),
                QuoteCurrency::new(Decimal::from(100) * l),
            )
            .unwrap()];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(50) * l))
                    .unwrap(),
                Order::limit(Side::Sell, quote!(100.0), QuoteCurrency::new(Decimal::from(100) * l))
                    .unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), &p, f_m), base!(1.5));
        }
    }
}
