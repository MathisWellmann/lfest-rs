use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct ConfigDecimal {
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
    pub starting_balance_base: Decimal,
}

impl ConfigDecimal {
    pub fn bitmex_perpetuals() -> Self {
        Self {
            fee_maker: Decimal::new(-000025, 5),
            fee_taker: Decimal::new(000075, 5),
            starting_balance_base: Decimal::new(1, 0),
        }
    }

    pub fn bitmex_futures() -> Self {
        Self {
            fee_maker: Decimal::new(-00005, 4),
            fee_taker: Decimal::new(00025, 4),
            starting_balance_base: Decimal::new(1, 0),
        }
    }

    pub fn deribit_futures() -> Self {
        Self {
            fee_maker: Decimal::new(0, 0),
            fee_taker: Decimal::new(0005, 4),
            starting_balance_base: Decimal::new(1, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_perpetuals() {
        let config = ConfigDecimal::bitmex_perpetuals();

        assert!(config.fee_maker.is_sign_negative());
        assert_eq!(config.fee_maker.to_string(), "-0.00025");
        assert_eq!(config.fee_taker.to_string(), "0.00075");
    }

    #[test]
    fn config_futures() {
        let config = ConfigDecimal::bitmex_futures();

        assert!(config.fee_maker.is_sign_negative());
        assert_eq!(config.fee_maker.to_string(), "-0.0005");
        assert_eq!(config.fee_taker.to_string(), "0.0025");
    }
}
