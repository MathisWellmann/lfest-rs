use crate::{max, min, Currency, Fee, FuturesTypes, Order, Side};

/// Compute the needed order margin with a newly added order
///
/// # Arguments:
/// `orders`: All the open orders
/// `pos_size`: The current position size
/// `futures_type`: The type of futures contract this is computed for
/// `leverage`: The positions leverage
/// `fee_maker`: Fee of the maker
///
/// # Returns:
/// The margin required for those orders, measured in the margin currency which
/// is the pair of the order size currency.
pub(crate) fn order_margin<S>(
    orders: impl Iterator<Item = Order<S>>,
    pos_size: S,
    futures_type: FuturesTypes,
    leverage: f64,
    fee_maker: Fee,
) -> S::PairedCurrency
where
    S: Currency,
{
    let mut buy_size = S::new_zero();
    let mut sell_size = S::new_zero();
    let mut buy_price_weight: f64 = 0.0;
    let mut sell_price_weight: f64 = 0.0;
    let mut buy_side_fees = S::PairedCurrency::new_zero();
    let mut sell_side_fees = S::PairedCurrency::new_zero();
    for o in orders {
        let limit_price = o.limit_price().expect("Limit price must exist; qed");
        let fee_margin = o.size().convert(limit_price).fee_portion(fee_maker);
        let size: f64 = o.size().into();
        match o.side() {
            Side::Buy => {
                buy_size += o.size();
                buy_price_weight += limit_price.into() * size;
                buy_side_fees += fee_margin;
            }
            Side::Sell => {
                sell_size += o.size();
                sell_price_weight += limit_price.into() * size;
                sell_side_fees += fee_margin;
            }
        }
    }

    let bsd: f64 = (buy_size - min(pos_size, S::new_zero()).abs()).into();
    let ssd: f64 = (sell_size - max(pos_size, S::new_zero())).into();
    let mut fees = S::PairedCurrency::new_zero();
    let order_margin: S::PairedCurrency = if (buy_size == S::new_zero()
        && sell_size == S::new_zero())
        || (bsd == 0.0 && ssd == 0.0)
    {
        S::PairedCurrency::new_zero()
    } else if ssd > bsd {
        if ssd == 0.0 {
            return S::PairedCurrency::new_zero();
        }
        fees = sell_side_fees;
        let price_mult: f64 = match futures_type {
            FuturesTypes::Linear => sell_price_weight / sell_size.into(),
            FuturesTypes::Inverse => 1.0 / (sell_price_weight / sell_size.into()),
        };
        (ssd * price_mult).into()
    } else {
        if bsd == 0.0 {
            return S::PairedCurrency::new_zero();
        }
        fees = buy_side_fees;
        let price_mult: f64 = match futures_type {
            FuturesTypes::Linear => buy_price_weight / buy_size.into(),
            FuturesTypes::Inverse => 1.0 / (buy_price_weight / buy_size.into()),
        };
        (bsd * price_mult).into()
    };
    debug!(
        "pos_size: {}, bsd: {}, ssd: {}, buy_price_weight {}, sell_price_weight {}, buy_size: {}, sell_size: {}, om: {}, buy_side_fees: {}, sell_side_fees: {}",
        pos_size, bsd, ssd, buy_price_weight, sell_price_weight, buy_size, sell_size, order_margin, buy_side_fees, sell_side_fees,
    );

    // TODO: not sure if this method of including the fees is correct, but its about
    // right xD
    (order_margin / leverage.into()) + fees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn order_margin_linear_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let p = base!(0.0);
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(0.5 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(0.5 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));
        }
    }

    #[test]
    fn order_margin_linear_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let p = base!(l);

            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(0.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(0.5 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));
        }
    }

    #[test]
    fn order_margin_linear_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let p = base!(-l);

            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(0.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(100.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(200.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(300.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), base!(1.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(0.5 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), base!(1.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), quote!(150.0));
        }
    }

    #[test]
    fn order_margin_inverse_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let p = quote!(0.0);
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(50.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.5));
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let p = quote!(l * 100.0);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(0.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(50.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let f_m = Fee(0.0);

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let p = quote!(-l * 100.0);

            let orders = vec![];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(0.0));

            let orders = vec![Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(0.0));

            let orders = vec![Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap()];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(2.0));

            let orders = vec![
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(3.0));

            let orders = vec![
                Order::limit(Side::Buy, quote!(100.0), quote!(100.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(50.0 * l)).unwrap(),
                Order::limit(Side::Sell, quote!(100.0), quote!(100.0 * l)).unwrap(),
            ];
            assert_eq!(order_margin(orders.into_iter(), p, ft, l, f_m), base!(1.5));
        }
    }
}
