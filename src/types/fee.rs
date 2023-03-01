use derive_more::Display;
use malachite::Rational;

/// Allows the quick construction of `Fee`
#[macro_export]
macro_rules! fee {
    ( $a:expr ) => {{
        Fee::from_f64($a)
    }};
}

/// Fee as a fraction
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Display)]
pub struct Fee(Rational);

impl Fee {
    #[inline(always)]
    pub(crate) fn new(val: Rational) -> Self {
        Self(val)
    }

    #[inline]
    pub(crate) fn from_f64(val: f64) -> Self {
        Self(Rational::try_from_float_simplest(val).expect("Unable to get Rational from float"))
    }

    #[inline(always)]
    pub(crate) fn inner(self) -> Rational {
        self.0
    }

    #[inline(always)]
    pub(crate) fn inner_ref(&self) -> &Rational {
        &self.0
    }
}
