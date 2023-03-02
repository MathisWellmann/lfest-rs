use fpdec::Decimal;

use crate::{max, min, Currency, Fee, FuturesTypes, Leverage, Order, Side};

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
///
/// TODO: rework this, its too complex
pub(crate) fn order_margin<S>(
    orders: impl Iterator<Item = Order<S>>,
    pos_size: S,
    futures_type: FuturesTypes,
    leverage: Leverage,
    fee_maker: Fee,
) -> S::PairedCurrency
where
    S: Currency,
{
    let mut buy_size = S::new_zero();
    let mut sell_size = S::new_zero();
    let mut buy_price_weight = Decimal::ZERO;
    let mut sell_price_weight = Decimal::ZERO;
    let mut buy_side_fees = S::PairedCurrency::new_zero();
    let mut sell_side_fees = S::PairedCurrency::new_zero();
    for o in orders {
        let limit_price = o.limit_price().expect("Limit price must exist; qed");
        let fee_margin = o.quantity().convert(limit_price).fee_portion(fee_maker);
        let size = o.quantity();
        match o.side() {
            Side::Buy => {
                buy_size += o.quantity().clone();
                buy_price_weight += limit_price.inner() * size.inner();
                buy_side_fees += fee_margin;
            }
            Side::Sell => {
                sell_size += o.quantity().clone();
                sell_price_weight += limit_price.inner() * size.inner();
                sell_side_fees += fee_margin;
            }
        }
    }

    let bsd = buy_size.inner() - min(pos_size.inner(), Decimal::ZERO).abs();
    let ssd = sell_size.inner() - max(pos_size.inner(), Decimal::ZERO);
    let mut fees = S::PairedCurrency::new_zero();
    let order_margin = if (buy_size == S::new_zero() && sell_size == S::new_zero())
        || (bsd == Decimal::ZERO && ssd == Decimal::ZERO)
    {
        Decimal::ZERO
    } else if ssd > bsd {
        if ssd == Decimal::ZERO {
            return S::PairedCurrency::new_zero();
        }
        fees = sell_side_fees;

        let price_mult = match futures_type {
            FuturesTypes::Linear => sell_price_weight / sell_size.inner(),
            FuturesTypes::Inverse => Decimal::ONE / (sell_price_weight / sell_size.inner()),
        };
        ssd * price_mult
    } else {
        if bsd == Decimal::ZERO {
            return S::PairedCurrency::new_zero();
        }
        fees = buy_side_fees;

        let price_mult = match futures_type {
            FuturesTypes::Linear => buy_price_weight / buy_size.inner(),
            FuturesTypes::Inverse => Decimal::ONE / (buy_price_weight / buy_size.inner()),
        };
        bsd * price_mult
    };
    debug!(
        "pos_size: {}, bsd: {}, ssd: {}, buy_price_weight {}, sell_price_weight {}, buy_size: {}, sell_size: {}, om: {}, buy_side_fees: {}, sell_side_fees: {}",
        pos_size, bsd, ssd, buy_price_weight, sell_price_weight, buy_size, sell_size, order_margin, buy_side_fees, sell_side_fees,
    );

    // TODO: not sure if this method of including the fees is correct, but its about
    // right xD
    S::PairedCurrency::new(order_margin / leverage.inner()) + fees
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{base, fee, leverage, quote, BaseCurrency, QuoteCurrency};

    #[test]
    fn order_margin_linear_futures_without_position() {
        if let Err(_) = pretty_env_logger::try_init() {}

        let ft = FuturesTypes::Linear;
        let p = base!(0.0);
        let f_m = &fee!(0.0);

        for l in (1..5).map(|v| &leverage!(v as f64)) {
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
        let f_m = &fee!(0.0);

        for l in (1..5).map(|v| &leverage!(v as f64)) {
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
        let f_m = &fee!(0.0);

        for l in (1..5).map(|v| &leverage!(v as f64)) {
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
        let f_m = fee!(0.0);

        for l in (1..5).map(|v| leverage!(v as f64)) {
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
        let f_m = fee!(0.0);

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
        let f_m = fee!(0.0);

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
