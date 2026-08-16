//! What a shop sells, and what happened to somebody's money.
//!
//! Three things are decided here rather than in whichever handler is written
//! next: where an order may go from where it is, what an order comes to, and
//! the order in which two checkouts reach for the same shelf.
//!
//! Money is [`mavi_core::money::Money`] everywhere — minor units and a
//! currency, never a float and never a bare number. A column holding `1250`
//! says nothing until something says twelve fifty of what.

pub mod coupon;
pub mod order;
pub mod stock;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use mavi_work::Kind as Work;

pub use coupon::{Coupon, off};
pub use order::{Line, State, comes_to, moves};
pub use stock::{Wanted, enough, reached_for};

id!(
    /// One thing a shop sells.
    ProductId
);

id!(
    /// One order.
    OrderId
);

pub const SHOP: &str = "shop";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(SHOP, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(SHOP, Access::Write)
}

/// Putting stock back that a checkout took and nobody paid for.
///
/// Worth trying again for as long as it takes: the alternative is a shelf that
/// says nothing is left when there is. Declared here so the queue refuses to
/// take work of any other name.
pub const PUT_BACK_WHAT_NOBODY_PAID_FOR: Work = Work::new("shop.put-back", 20);

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_shelf();
    all.extend(the_orders());
    all.extend(for_anybody());
    all
}

/// What is for sale, and what it costs.
fn the_shelf() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/products",
            named: "products.list",
            about: "What this shop sells, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("ProductPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/products",
            named: "products.make",
            about: "Puts something on the shelf.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewProduct"),
            answers: Answers::Made("Product"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/products/{id}",
            named: "products.change",
            about: "Changes what something is called, what it costs, or how many there are.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which product.")],
            takes: Some("ProductChanges"),
            answers: Answers::With("Product"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/products/{id}",
            named: "products.remove",
            about: "Takes something off the shelf. What was already ordered keeps its own words.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which product.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/coupons",
            named: "coupons.list",
            about: "The codes this shop honours.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("CouponList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/coupons",
            named: "coupons.make",
            about: "Makes one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewCoupon"),
            answers: Answers::Made("Coupon"),
            refuses: &[Code::Conflict],
            changes: true,
        },
    ]
}

/// What happened to somebody's money.
fn the_orders() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/orders",
            named: "orders.list",
            about: "Every order, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("state", Is::Text, "Only orders sitting here."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("OrderPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/orders/{id}",
            named: "orders.read",
            about: "One order, its lines, and what it came to.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which order.")],
            takes: None,
            answers: Answers::With("Order"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/orders/{id}/moves",
            named: "orders.move",
            about: "Says where an order has got to: paid for, sent, called off, given back.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which order.")],
            // Where it is going, rather than an endpoint per destination: the
            // rule about what may follow what is one rule, and four endpoints
            // is four places to forget half of it.
            takes: Some("WhereItGoes"),
            answers: Answers::With("Order"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
    ]
}

/// What a visitor with a basket reaches.
fn for_anybody() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/open/products",
            named: "open.products",
            about: "What is for sale, as a page shows it.",
            who: Who::Anybody,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            // Never how many are left as a number: a shop that answers "one"
            // to anybody who asks has told a competitor its whole stock list.
            // What a page needs is whether it can be bought.
            answers: Answers::With("ForSalePage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/open/orders",
            named: "open.order",
            about: "Takes a basket and makes an order. Stock is held, not yet sold.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: Some("Basket"),
            answers: Answers::Made("Placed"),
            // Not enough of something, or a code that has run out. Never which
            // of the two until the basket is otherwise fine, because a refusal
            // is also an answer about what this shop has.
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn no_two_of_these_are_the_same_route() {
        assert!(Api::of(endpoints()).clashes().is_empty());
    }

    #[test]
    fn what_anybody_can_reach_says_so_in_its_path() {
        for endpoint in endpoints() {
            assert_eq!(
                endpoint.who == Who::Anybody,
                endpoint.path.starts_with("/api/open/"),
                "{} is one thing in its path and another in its audience",
                endpoint.named
            );
        }
    }

    #[test]
    fn where_an_order_goes_is_one_endpoint_because_it_is_one_rule() {
        // Four endpoints — pay, send, call off, give back — is four places to
        // forget half of what may follow what.
        let moving: Vec<&str> = endpoints()
            .iter()
            .filter(|e| e.changes && e.path.starts_with("/api/orders"))
            .map(|e| e.named)
            .collect();

        assert_eq!(moving, ["orders.move"]);
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(SHOP));
    }
}
