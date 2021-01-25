#[derive(Debug, Clone)]
pub struct ConfigFloat {
    pub fee_maker: f64,
    pub fee_taker: f64,
    pub starting_balance_base: f64, // the starting balance denoted in BASE currency
}

impl ConfigFloat {
    pub fn bitmex_perpetuals() -> Self {
        Self {
            fee_maker: -0.00025,
            fee_taker: 0.00075,
            starting_balance_base: 1.0,
        }
    }

    pub fn bitmex_futures() -> Self {
        Self {
            fee_maker: -0.0005,
            fee_taker: 0.00025,
            starting_balance_base: 1.0,
        }
    }

    pub fn deribit_futures() -> Self {
        Self {
            fee_maker: 0.0,
            fee_taker: 0.0005,
            starting_balance_base: 1.0,
        }
    }
}
