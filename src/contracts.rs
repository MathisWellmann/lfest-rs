#[derive(Debug, Clone, PartialEq)]
pub enum ContractType {
    Perpetual,
    Future,
}

impl ContractType {
    // returns the contract type of a given string: e.g: XBTUSD -> Perpetual, XBTM20 -> Future
    pub fn parse(symbol: &str) -> Self {
        let end = &symbol[symbol.len() - 3..];
        if end == "USD" {
            return ContractType::Perpetual
        }
        let fut = &symbol[symbol.len() - 3..symbol.len() - 2].to_uppercase();
        return if fut == "H" || fut == "M" || fut == "U" || fut == "Z" {
            ContractType::Future
        } else {
            ContractType::Perpetual
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_type_parse() {
        let p = "XBTUSD";
        let ct = ContractType::parse(p);
        assert_eq!(ct, ContractType::Perpetual);

        let f = "XBTM20";
        let ct = ContractType::parse(f);
        assert_eq!(ct, ContractType::Future);

        let f = "XBTU20";
        let ct = ContractType::parse(f);
        assert_eq!(ct, ContractType::Future);

        let f = "XBTZ20";
        let ct = ContractType::parse(f);
        assert_eq!(ct, ContractType::Future);

        let f = "XBTH20";
        let ct = ContractType::parse(f);
        assert_eq!(ct, ContractType::Future);

    }
}