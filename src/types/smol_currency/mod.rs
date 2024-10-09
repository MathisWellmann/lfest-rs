use num::Integer;

pub(crate) struct Monies<T, Marker>
where
    T: Integer,
    Marker: CurrencyMarker,
{
    value: T,
    _marker: std::marker::PhantomData<Marker>,
}

pub trait CurrencyMarker {
    type PairedCurrency;
}

struct Base;

struct Quote;

impl CurrencyMarker for Base {
    type PairedCurrency = Quote;
}

impl CurrencyMarker for Quote {
    type PairedCurrency = Base;
}
