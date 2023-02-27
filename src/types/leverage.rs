use derive_more::Display;
use malachite::Rational;

/// Allows the quick construction of `Leverage`
#[macro_export]
macro_rules! leverage {
    ( $a:expr ) => {{
        Leverage::from_f64($a)
    }};
}

/// Leverage
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Display)]
pub struct Leverage(Rational);

impl Leverage {
    #[inline(always)]
    pub(crate) fn new(val: Rational) -> Self {
        Self(val)
    }

    #[inline]
    pub(crate) fn from_f64(val: f64) -> Self {
        Self(Rational::try_from_float_simplest(val).expect("Unable to get Rational from float"))
    }

    #[inline(always)]
    pub(crate) fn inner(&self) -> Rational {
        self.0
    }
}
