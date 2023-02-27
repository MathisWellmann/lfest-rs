use std::fmt::Formatter;

/// Side of the order
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum Side {
    /// Buy side
    Buy,
    /// Sell side
    Sell,
}

impl Side {
    /// Returns the inverted side
    pub fn inverted(&self) -> Self {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

impl std::fmt::Display for Side {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
