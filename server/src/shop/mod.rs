//! Selling things.
//!
//! Products and what is left of them, a checkout that holds stock while
//! somebody pays, discount codes with conditions, and orders numbered per
//! site. Card details never reach this machine.
mod checkout;
mod coupons;
mod products;

pub use checkout::{Checkout, Order, OrderState, Placed};
pub use products::{NewProduct, Product};

use crate::kernel::authz::{Access, Capability, Needs};
use crate::kernel::http::Endpoint;

pub(crate) fn shop(access: Access) -> Needs {
    Needs::new(Capability::Shop, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = products::endpoints();
    all.extend(checkout::endpoints());
    all.extend(coupons::endpoints());
    all
}

pub use checkout::{
    DropStuck, Reconcile, ReleaseHolds, SweepOrders, WarnOnLowStock, drop_stuck, reconcile,
    release_holds, sweep_orders, warn_on_low_stock,
};

#[must_use]
pub fn kinds() -> Vec<String> {
    use crate::kernel::queue::Task;

    vec![
        ReleaseHolds::KIND.to_owned(),
        DropStuck::KIND.to_owned(),
        WarnOnLowStock::KIND.to_owned(),
        SweepOrders::KIND.to_owned(),
        Reconcile::KIND.to_owned(),
    ]
}
