use crate::{max, min, FuturesTypes, Order, Side};

/// Compute the needed order margin with a newly added order
pub(crate) fn order_margin<'a>(
    orders: impl Iterator<Item = &'a Order>,
    pos_size: f64,
    futures_type: FuturesTypes,
    leverage: f64,
    fee_maker: f64,
) -> f64 {
    let mut buy_size: f64 = 0.0;
    let mut sell_size: f64 = 0.0;
    let mut buy_price_weight: f64 = 0.0;
    let mut sell_price_weight: f64 = 0.0;
    let mut cumulative_fees: f64 = 0.0;
    for o in orders {
        let price = o.limit_price().unwrap();
        let price_mult = match futures_type {
            FuturesTypes::Linear => price,
            FuturesTypes::Inverse => 1.0 / price,
        };
        match o.side() {
            Side::Buy => {
                buy_size += o.size();
                buy_price_weight += o.limit_price().unwrap() * o.size();
            }
            Side::Sell => {
                sell_size += o.size();
                sell_price_weight += o.limit_price().unwrap() * o.size();
            }
        }
        cumulative_fees += o.size() * price_mult * fee_maker;
    }

    let bsd = buy_size - min(pos_size, 0.0).abs();
    let ssd = sell_size - max(pos_size, 0.0);
    let order_margin: f64 = if buy_size == 0.0 && sell_size == 0.0 {
        0.0
    } else if bsd == 0.0 && ssd == 0.0 {
        0.0
    } else if ssd > bsd {
        if ssd == 0.0 {
            return cumulative_fees;
        }
        let price_mult = match futures_type {
            FuturesTypes::Linear => (sell_price_weight / sell_size),
            FuturesTypes::Inverse => 1.0 / (sell_price_weight / sell_size),
        };
        ssd * price_mult
    } else {
        if bsd == 0.0 {
            return cumulative_fees;
        }
        let price_mult = match futures_type {
            FuturesTypes::Linear => (buy_price_weight / buy_size),
            FuturesTypes::Inverse => 1.0 / (buy_price_weight / buy_size),
        };
        bsd * price_mult
    };
    debug!(
        "pos_size: {}, bsd: {}, ssd: {}, buy_price_weight {}, sell_price_weight {}, buy_size: {}, sell_size: {}, om: {}, fees: {}",
        pos_size, bsd, ssd, buy_price_weight, sell_price_weight, buy_size, sell_size, order_margin, cumulative_fees,
    );

    // TODO: not sure if this method of including the fees is correct, but its about right xD
    (order_margin / leverage) + cumulative_fees
}

#[cfg(test)]
mod tests {
    use super::*;

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
