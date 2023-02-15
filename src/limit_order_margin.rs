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
    let mut buy_size: f64 = 0.0;
    let mut sell_size: f64 = 0.0;
    let mut buy_price_weight: f64 = 0.0;
    let mut sell_price_weight: f64 = 0.0;
    let mut buy_side_fees: f64 = 0.0;
    let mut sell_side_fees: f64 = 0.0;
    for o in orders {
        let price = o.limit_price().unwrap();
        let price_mult = match futures_type {
            FuturesTypes::Linear => price,
            FuturesTypes::Inverse => 1.0 / price,
        };
        let fee = o.size() * price_mult * fee_maker;
        match o.side() {
            Side::Buy => {
                buy_size += o.size();
                buy_price_weight += o.limit_price().unwrap() * o.size();
                buy_side_fees += fee;
            }
            Side::Sell => {
                sell_size += o.size();
                sell_price_weight += o.limit_price().unwrap() * o.size();
                sell_side_fees += fee;
            }
        }
    }

    let bsd = buy_size - min(pos_size, 0.0).abs();
    let ssd = sell_size - max(pos_size, 0.0);
    let mut fees = 0.0;
    let order_margin: f64 = if (buy_size == 0.0 && sell_size == 0.0) || (bsd == 0.0 && ssd == 0.0) {
        0.0
    } else if ssd > bsd {
        if ssd == 0.0 {
            return 0.0;
        }
        let price_mult = match futures_type {
            FuturesTypes::Linear => sell_price_weight / sell_size,
            FuturesTypes::Inverse => 1.0 / (sell_price_weight / sell_size),
        };
        fees = sell_side_fees;
        ssd * price_mult
    } else {
        if bsd == 0.0 {
            return 0.0;
        }
        let price_mult = match futures_type {
            FuturesTypes::Linear => buy_price_weight / buy_size,
            FuturesTypes::Inverse => 1.0 / (buy_price_weight / buy_size),
        };
        fees = buy_side_fees;
        bsd * price_mult
    };
    debug!(
        "pos_size: {}, bsd: {}, ssd: {}, buy_price_weight {}, sell_price_weight {}, buy_size: {}, sell_size: {}, om: {}, buy_side_fees: {}, sell_side_fees: {}",
        pos_size, bsd, ssd, buy_price_weight, sell_price_weight, buy_size, sell_size, order_margin, buy_side_fees, sell_side_fees,
    );

    // TODO: not sure if this method of including the fees is correct, but its about
    // right xD
    (order_margin / leverage) + fees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn order_margin_linear_futures_with_fees() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let f_m = Fee(0.0002);
        let ft = FuturesTypes::Linear;

        let p = base!(0.0);
        let orders = vec![Order::limit(Side::Buy, base!(1.0), quote!(1000.0)).unwrap()];
        assert_eq!(order_margin(orders.iter(), p, ft, 1.0, f_m), quote!(1000.2));
        let orders = vec![Order::limit(Side::Sell, base!(1.0), quote!(1000.0)).unwrap()];
        assert_eq!(order_margin(orders.iter(), p, ft, 1.0, f_m), quote!(1000.2));

        let p = 1000.0;
        let orders = vec![Order::limit(Side::Sell, base!(1.0), quote!(1000.0)).unwrap()];
        assert_eq!(order_margin(orders.iter(), p, ft, 1.0, f_m), quote!(0.0));
        let orders = vec![Order::limit(Side::Buy, base!(1.0), quote!(1000.0)).unwrap()];
        assert_eq!(order_margin(orders.iter(), p, ft, 1.0, f_m), quote!(1000.2));
    }

    #[test]
    fn order_margin_linear_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let p = 0.0;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 300.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 0.5 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 0.5 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);
        }
    }

    #[test]
    fn order_margin_linear_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let p = l;

            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 300.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 0.5 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);
        }
    }

    #[test]
    fn order_margin_linear_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            let p = -l;

            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 100.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 200.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 300.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 1.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 0.5 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 1.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 150.0);
        }
    }

    #[test]
    fn order_margin_inverse_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let p = 0.0;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 3.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 3.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 50.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.5);
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_long_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let p = l * 100.0;

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 3.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 50.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);
        }
    }

    #[test]
    fn order_margin_inverse_futures_with_short_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Inverse;
        let f_m = 0.0;

        for l in [1.0, 2.0, 3.0, 4.0, 5.0] {
            debug!("leverage: {}", l);

            let p = -l * 100.0;

            let orders = vec![];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 0.0);

            let orders = vec![Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap()];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 2.0);

            let orders = vec![
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 3.0);

            let orders = vec![
                Order::limit(Side::Buy, 100.0, 100.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 50.0 * l).unwrap(),
                Order::limit(Side::Sell, 100.0, 100.0 * l).unwrap(),
            ];
            assert_eq!(order_margin(orders.iter(), p, ft, l, f_m), 1.5);
        }
    }
}
