#[derive(Debug, Clone)]
pub struct Config {
    pub max_leverage: f64,
    pub min_leverage: f64,
    pub max_active_orders: usize,
    pub base_risk_limit: u32,
    pub step: u32,
    pub base_maintenance_margin: f64,
    pub base_initial_margin: f64,
}

impl Config {
    // bitmex xbtusd contract
    pub fn xbt_usd() -> Config {
        return Config{
            max_leverage: 100.0,
            min_leverage: 1.0,
            max_active_orders: 100,
            base_risk_limit: 200,
            step: 100,
            base_maintenance_margin: 0.0045,
            base_initial_margin: 0.01,
        }
    }

    // TODO: query more configs from bitmex api

}