#[derive(Debug, Clone)]
pub struct Config {
    pub fee_maker: f64,
    pub fee_taker: f64,
}

impl Config {
    pub fn bitmex_perpetuals() -> Self {
        Config{
            fee_maker: -0.00025,
            fee_taker: 0.00075,
        }
    }

    pub fn bitmex_futures() -> Self {
        Config {
            fee_maker: -0.0005,
            fee_taker: 0.0025,
        }
    }

    pub fn deribit_futures() -> Self {
        Config{
            fee_maker: 0.0,
            fee_taker: 0.0005,
        }
    }
}
