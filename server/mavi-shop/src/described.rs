//! What a shop sells, and what buying something is.

use mavi_api::{Field, Is, Of, Shape};

/// Money is never a number on its own here.
///
/// The smallest unit as an integer, so nothing is ever half a kuruş, and the
/// currency beside it — because "fifty off" is not an amount until something
/// says fifty of what.
fn money(name: &'static str, about: &'static str) -> Field {
    Field::new(name, Of::Another("Money"), about)
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    let mut all = vec![
        an_amount(),
        a_product(),
        for_sale(),
        something_new(),
        what_may_change(),
    ];

    all.extend([
        Shape::page_of("ProductPage", "Product", "What a shop has."),
        Shape::page_of(
            "ForSalePage",
            "ForSale",
            "What a shop is selling, as a page shows it.",
        ),
    ]);
    all.extend(the_codes());
    all.extend(the_orders());

    all
}

fn an_amount() -> Shape {
    Shape::new(
        "Money",
        "An amount, in a currency. Never a number on its own: adding lira to \
         euros is not an arithmetic problem, and answering zero would be worse \
         than refusing.",
        vec![
            Field::new(
                "minor",
                Of::One(Is::Number),
                "In the currency's smallest unit — kuruş, cents, pence. A whole \
                 number, so nothing is ever half of one.",
            ),
            Field::new(
                "currency",
                Of::One(Is::Text),
                "Which currency, as ISO 4217.",
            ),
        ],
    )
}

fn a_product() -> Shape {
    Shape::new(
        "Product",
        "Something a shop sells, as whoever runs it sees it.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new("about", Of::One(Is::Text), "What it is.").or_null(),
            money("price", "What it costs."),
            Field::new(
                "on_the_shelf",
                Of::One(Is::Number),
                "How many there are. Not answered to anybody outside — see \
                 `ForSale`.",
            ),
            Field::new("for_sale", Of::One(Is::Bool), "Whether it is being sold."),
            Field::new("created_at", Of::One(Is::Moment), "When it was added."),
        ],
    )
}

fn for_sale() -> Shape {
    Shape::new(
        "ForSale",
        "The same thing as a page shows it. What it leaves out is the number: a \
         shop that answers \"one left\" to anybody who asks has published its \
         stock list. What a page needs is whether it can be bought.",
        vec![
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new("about", Of::One(Is::Text), "What it is.").or_null(),
            money("price", "What it costs."),
            Field::new(
                "can_be_bought",
                Of::One(Is::Bool),
                "Whether somebody may buy it now.",
            ),
        ],
    )
}

fn something_new() -> Shape {
    Shape::new(
        "NewProduct",
        "Something to put on the shelf.",
        vec![
            Field::new("slug", Of::One(Is::Text), "Where it should answer."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new("about", Of::One(Is::Text), "What it is.")
                .maybe()
                .or_null(),
            Field::new(
                "price_minor",
                Of::One(Is::Number),
                "What it costs, in the currency's smallest unit.",
            ),
            Field::new(
                "currency",
                Of::One(Is::Text),
                "Which currency, as ISO 4217.",
            ),
            Field::new("on_the_shelf", Of::One(Is::Number), "How many there are."),
        ],
    )
}

fn what_may_change() -> Shape {
    Shape::new(
        "ProductChanges",
        "What may be changed. Not its currency: an order already placed in one \
         and a price now in another is a shop that cannot add up its own \
         orders.",
        vec![
            Field::new("name", Of::One(Is::Text), "What it is called.").maybe(),
            Field::new("about", Of::One(Is::Text), "What it is.").maybe(),
            Field::new(
                "price_minor",
                Of::One(Is::Number),
                "What it costs, in the currency's smallest unit.",
            )
            .maybe(),
            Field::new("on_the_shelf", Of::One(Is::Number), "How many there are.").maybe(),
            Field::new("for_sale", Of::One(Is::Bool), "Whether it is being sold.").maybe(),
        ],
    )
}

fn the_codes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Coupon",
            "A code somebody types in.",
            vec![
                Field::new(
                    "code",
                    Of::One(Is::Text),
                    "Upper case, always. A code read off a poster and typed in \
                     lower case is the same code, and the alternative is a \
                     discount that works for half the people who try it.",
                ),
                Field::new(
                    "kind",
                    Of::OneOf(&["percent", "amount"]),
                    "Which of the two it takes off.",
                ),
                Field::new("percent", Of::One(Is::Number), "How many per cent.").or_null(),
                money("amount", "How much money.").or_null(),
                Field::new(
                    "at_most_uses",
                    Of::One(Is::Number),
                    "How many times it may be used at all. Null is as many as \
                     anybody likes, which is a decision somebody made rather \
                     than a field left out.",
                )
                .or_null(),
                Field::new("expires_at", Of::One(Is::Moment), "When it stops working.").or_null(),
            ],
        ),
        Shape::list_of(
            "CouponList",
            "Coupon",
            "Every code a shop has. A handful, with nothing to page through.",
        ),
        Shape::new(
            "NewCoupon",
            "A code to make. Either a percentage or an amount with its currency \
             — never both, and never neither.",
            vec![
                Field::new("code", Of::One(Is::Text), "What somebody types in."),
                Field::new(
                    "percent",
                    Of::One(Is::Number),
                    "How many per cent, one to a hundred.",
                )
                .maybe()
                .or_null(),
                Field::new(
                    "amount_minor",
                    Of::One(Is::Number),
                    "How much, in the currency's smallest unit.",
                )
                .maybe()
                .or_null(),
                Field::new(
                    "currency",
                    Of::One(Is::Text),
                    "Which currency the amount is in.",
                )
                .maybe()
                .or_null(),
                Field::new(
                    "at_most_uses",
                    Of::One(Is::Number),
                    "How many times it may be used at all.",
                )
                .maybe()
                .or_null(),
                Field::new("expires_at", Of::One(Is::Moment), "When it stops working.")
                    .maybe()
                    .or_null(),
            ],
        ),
    ]
}

fn the_orders() -> Vec<Shape> {
    vec![
        Shape::new(
            "OrderLine",
            "One thing on an order, as it was when the order was placed. The \
             name and the price are copied rather than pointed at: what \
             somebody bought does not change because the shop renamed \
             something afterwards.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it was called."),
                money("each", "What one cost."),
                Field::new("how_many", Of::One(Is::Number), "How many."),
            ],
        ),
        Shape::new(
            "Order",
            "Something somebody bought.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new(
                    "number",
                    Of::One(Is::Number),
                    "What somebody reads down a telephone.",
                ),
                Field::new(
                    "state",
                    Of::OneOf(&["waiting", "paid", "sent", "called_off", "given_back"]),
                    "Where it has got to. Stock is held against one that is \
                     waiting, and put back when it runs out.",
                ),
                Field::new(
                    "email",
                    Of::One(Is::Text),
                    "Where to reach whoever bought it.",
                ),
                money("total", "What it came to."),
                Field::new("lines", Of::ManyOf("OrderLine"), "What is on it."),
                Field::new("created_at", Of::One(Is::Moment), "When it was placed."),
            ],
        ),
        Shape::page_of("OrderPage", "Order", "What a shop has sold."),
        Shape::new(
            "WhereItGoes",
            "Where an order goes next. Which moves are allowed is the order's \
             own rule rather than the caller's — one that has gone out does not \
             go back to waiting.",
            vec![Field::new(
                "to",
                Of::OneOf(&["paid", "sent", "called_off", "given_back"]),
                "Where to move it.",
            )],
        ),
        Shape::new(
            "Wanted",
            "One thing somebody is buying, and how many.",
            vec![
                Field::new("product", Of::One(Is::Id), "Which one."),
                Field::new("how_many", Of::One(Is::Number), "How many of it."),
            ],
        ),
        a_basket(),
        Shape::new(
            "Placed",
            "The order, and what it came to. Nothing else: whoever bought \
             something is not somebody this tells what else the shop has.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which order."),
                Field::new(
                    "number",
                    Of::One(Is::Number),
                    "What they read down a telephone.",
                ),
                money("total", "What it came to."),
            ],
        ),
    ]
}

fn a_basket() -> Shape {
    Shape::new(
        "Basket",
        "What somebody is buying. Anybody may send this, which is why every \
         rule is on this side.",
        vec![
            Field::new("email", Of::One(Is::Text), "Where to reach them."),
            Field::new(
                "wanted",
                Of::ManyOf("Wanted"),
                "What they are buying. Two lines for one thing is what pressing \
                 \"add\" twice looks like from here, and is read as one line \
                 for the sum.",
            ),
            Field::new("code", Of::One(Is::Text), "A coupon, if they typed one.")
                .maybe()
                .or_null(),
            Field::new(
                "said_once",
                Of::One(Is::Text),
                "The caller's own, and the caller's to repeat: the same request \
                 twice is one order. Held against the address it came with, so \
                 a guessed one does not answer somebody else's order.",
            ),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coupon::Coupon;
    use crate::order::{Line, State};
    use crate::stock::Wanted;
    use crate::store::{Basket, ForSale, NewCoupon, NewProduct, Order, Product, ProductChanges};
    use mavi_core::money::{Currency, Money};
    use std::collections::BTreeSet;

    fn fields_of(named: &str) -> BTreeSet<&'static str> {
        shapes()
            .iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect()
    }

    fn keys(what: &serde_json::Value) -> BTreeSet<&str> {
        what.as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    fn some() -> Money {
        Money::of(1000, Currency::parse("TRY").expect("a currency"))
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        assert_eq!(
            keys(&serde_json::to_value(some()).expect("money")),
            fields_of("Money")
        );

        let product = Product {
            id: uuid::Uuid::nil(),
            slug: "a-thing".to_owned(),
            name: "A Thing".to_owned(),
            about: None,
            price: some(),
            on_the_shelf: 3,
            for_sale: true,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&product).expect("a product")),
            fields_of("Product")
        );

        // The pair that must stay two. A shop answering "one left" to anybody
        // who asks has published its stock list.
        let shown = ForSale {
            slug: "a-thing".to_owned(),
            name: "A Thing".to_owned(),
            about: None,
            price: some(),
            can_be_bought: true,
        };

        let shown = serde_json::to_value(&shown).expect("what a page shows");

        assert!(shown.get("on_the_shelf").is_none());
        assert_eq!(keys(&shown), fields_of("ForSale"));

        let order = Order {
            id: uuid::Uuid::nil(),
            number: 1,
            state: State::Waiting,
            email: "somebody@example.test".to_owned(),
            total: some(),
            lines: Vec::new(),
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&order).expect("an order")),
            fields_of("Order")
        );

        let line = Line {
            name: "A Thing".to_owned(),
            each: some(),
            how_many: 1,
        };

        assert_eq!(
            keys(&serde_json::to_value(&line).expect("a line")),
            fields_of("OrderLine")
        );

        let coupon = Coupon::percent("TEN", 10).expect("a coupon");

        assert_eq!(
            keys(&serde_json::to_value(&coupon).expect("a coupon")),
            fields_of("Coupon")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let new = serde_json::to_value(NewProduct {
            slug: "a-thing".to_owned(),
            name: "A Thing".to_owned(),
            about: None,
            price_minor: 1000,
            currency: "TRY".to_owned(),
            on_the_shelf: 3,
        })
        .expect("a new product");

        assert_eq!(keys(&new), fields_of("NewProduct"));

        assert_eq!(
            keys(&serde_json::to_value(ProductChanges::default()).expect("changes")),
            fields_of("ProductChanges")
        );

        let code = serde_json::to_value(NewCoupon {
            code: "TEN".to_owned(),
            percent: Some(10),
            amount_minor: None,
            currency: None,
            at_most_uses: None,
            expires_at: None,
        })
        .expect("a new coupon");

        assert_eq!(keys(&code), fields_of("NewCoupon"));

        let basket = serde_json::to_value(Basket {
            email: "somebody@example.test".to_owned(),
            wanted: vec![Wanted {
                product: uuid::Uuid::nil(),
                how_many: 1,
            }],
            code: None,
            said_once: "whatever-the-caller-chose".to_owned(),
        })
        .expect("a basket");

        assert_eq!(keys(&basket), fields_of("Basket"));

        assert_eq!(
            keys(
                &serde_json::to_value(Wanted {
                    product: uuid::Uuid::nil(),
                    how_many: 1,
                })
                .expect("something wanted")
            ),
            fields_of("Wanted")
        );
    }
}
