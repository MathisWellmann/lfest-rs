use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct ConfigDecimal {
    pub fee_maker: Decimal,
    pub fee_taker: Decimal,
}

impl ConfigDecimal {
    pub fn perpetuals() -> Self {
        Self {
            fee_maker: Decimal::new(-000025, 5),
            fee_taker: Decimal::new(000075, 5),
        }
    }

    pub fn altcoin_futures() -> Self {
        Self {
            fee_maker: Decimal::new(-00005, 4),
            fee_taker: Decimal::new(00025, 4),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_perpetuals() {
        let config = ConfigDecimal::perpetuals();

        assert!(config.fee_maker.is_sign_negative());
        assert_eq!(config.fee_maker.to_string(), "-0.00025");
        assert_eq!(config.fee_taker.to_string(), "0.00075");
    }

    #[test]
    fn config_futures() {
        let config = ConfigDecimal::altcoin_futures();

        assert!(config.fee_maker.is_sign_negative());
        assert_eq!(config.fee_maker.to_string(), "-0.0005");
        assert_eq!(config.fee_taker.to_string(), "0.0025");
    }
}
