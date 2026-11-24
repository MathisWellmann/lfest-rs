//! Native off-book conditional / stop orders, lazy `PendingStops` store, and
//! `StopProtection` collar (algorithmically adapted from `OrderBook-rs`).

mod pending_stops;
mod stop_order;
mod stop_protection;

pub use pending_stops::{
    ElectedStop,
    PendingStops,
};
pub use stop_order::{
    StopCancelReason,
    StopKind,
    StopOrder,
    StopOrderError,
    StopOrderEvent,
    TrailDistance,
    TriggerDirection,
    TriggerPriceType,
};
pub use stop_protection::StopProtection;
