#[derive(Debug, Clone)]
pub struct Config {
    pub max_leverage: f64,
    pub min_leverage: f64,
    pub max_active_orders: usize,
    pub fee_maker: f64,
    pub fee_taker: f64,
}

impl Config {
    // bitmex xbtusd contract
    pub fn xbt_usd() -> Config {
        return Config{
            max_leverage: 100.0,
            min_leverage: 1.0,
            max_active_orders: 100,
            fee_maker: -0.00025,
            fee_taker: 0.00075,
        }
    }

    // TODO: query more configs from bitmex api

}
