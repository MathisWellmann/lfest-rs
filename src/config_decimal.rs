use rust_decimal::Decimal;


#[derive(Debug, Clone)]
pub struct Config {
    pub max_leverage: Decimal,
    pub min_leverage: Decimal,
    pub max_active_orders: usize,
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
}

impl Config {
    // bitmex xbtusd contract
    pub fn xbt_usd() -> Config {
        return Config{
            max_leverage: Decimal::new(100, 0),
            min_leverage: Decimal::new(1, 0),
            max_active_orders: 100,
            fee_maker: Decimal::new(-000025, 5),
            fee_taker: Decimal::new(00075, 5),
        }
    }

    // TODO: query more configs from bitmex api

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xbt_usd() {
        let config = Config::xbt_usd();

        assert!(config.fee_maker.is_sign_negative());
        assert_eq!(config.fee_maker.to_string(), "-0.00025");
        assert_eq!(config.fee_taker.to_string(), "0.00075");
    }
}