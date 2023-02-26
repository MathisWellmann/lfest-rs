use derive_more::Display;
use malachite::Rational;

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
    pub(crate) fn inner(&self) -> &Rational {
        &self.0
    }
}
