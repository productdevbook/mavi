//! Reading and writing what a shop sells.
//!
//! Everything careful in this crate meets the database here: the order rows
//! are locked in, the check that there are enough, the state machine, and the
//! discount arithmetic. Each of them is called rather than reimplemented — the
//! rules live beside the types they are about, and this is what asks them.

use chrono::{DateTime, Duration, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::money::{Currency, Money};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::BY_RECENT;
use crate::coupon::{Coupon, Kind, off};
use crate::order::{Line, State, comes_to, moves};
use crate::stock::{HELD_FOR_MINUTES, Wanted, enough, reached_for};

pub const THERE_IS_NO_SUCH_THING_FOR_SALE: &str = "there_is_no_such_thing_for_sale";
pub const THERE_IS_NO_ORDER_LIKE_THAT: &str = "there_is_no_order_like_that";
pub const SOMETHING_ELSE_IS_SOLD_AT_THAT_ADDRESS: &str = "something_else_is_sold_at_that_address";
pub const THAT_IS_NOT_A_CODE_THIS_SHOP_HONOURS: &str = "that_is_not_a_code_this_shop_honours";
pub const THAT_IS_NOT_WHERE_AN_ORDER_GOES: &str = "that_is_not_where_an_order_goes";

/// One thing a shop sells.
#[derive(Clone, Debug, Serialize)]
pub struct Product {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub about: Option<String>,
    pub price: Money,
    pub on_the_shelf: i32,
    pub for_sale: bool,
    pub created_at: DateTime<Utc>,
}

/// The same thing, as a page shows it.
///
/// A separate type, and what it leaves out is the number: a shop that answers
/// "one left" to anybody who asks has published its stock list. What a page
/// needs is whether it can be bought.
#[derive(Clone, Debug, Serialize)]
pub struct ForSale {
    pub slug: String,
    pub name: String,
    pub about: Option<String>,
    pub price: Money,
    pub can_be_bought: bool,
}

fn a_product(row: &PgRow) -> Result<Product> {
    let currency: String = row.try_get("currency").map_err(Error::internal)?;
    let minor: i64 = row.try_get("price_minor").map_err(Error::internal)?;

    Ok(Product {
        id: row.try_get("id").map_err(Error::internal)?,
        slug: row.try_get("slug").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        about: row.try_get("about").map_err(Error::internal)?,
        price: Money::of(minor, Currency::parse(&currency)?),
        on_the_shelf: row.try_get("on_the_shelf").map_err(Error::internal)?,
        for_sale: row.try_get("for_sale").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

const PRODUCT: &str = "id, slug, name, about, price_minor, currency, on_the_shelf, for_sale, \
                       created_at";

/// What this shop sells.
pub async fn products(tx: &mut Tx, query: &Query) -> Result<Page<Product>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];

    let cursor = walk.after(1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select {PRODUCT} from products where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(a_product)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |product| {
        vec![product.created_at.to_rfc3339(), product.id.to_string()]
    })
}

/// The same, as a page shows it.
pub async fn for_sale(tx: &mut Tx, query: &Query) -> Result<Page<ForSale>> {
    let page = products(tx, query).await?;

    Ok(Page {
        items: page
            .items
            .into_iter()
            .filter(|product| product.for_sale)
            .map(|product| ForSale {
                slug: product.slug,
                name: product.name,
                about: product.about,
                price: product.price,
                can_be_bought: product.on_the_shelf > 0,
            })
            .collect(),
        next: page.next,
    })
}

/// What putting something on the shelf asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewProduct {
    pub slug: String,
    pub name: String,
    pub about: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub on_the_shelf: i32,
}

/// Puts something on the shelf.
pub async fn add(tx: &mut Tx, new: &NewProduct) -> Result<Product> {
    let slug = Slug::parse(&new.slug)?;
    let currency = Currency::parse(&new.currency)?;

    let row = sqlx::query(&format!(
        "insert into products (id, slug, name, about, price_minor, currency, on_the_shelf)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning {PRODUCT}"
    ))
    .bind(Uuid::now_v7())
    .bind(slug.as_str())
    .bind(new.name.trim())
    .bind(new.about.as_deref())
    .bind(new.price_minor)
    .bind(currency.to_string())
    .bind(new.on_the_shelf)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| match &cause {
        sqlx::Error::Database(db) if db.constraint() == Some("products_address") => {
            Error::conflict(Say::of(SOMETHING_ELSE_IS_SOLD_AT_THAT_ADDRESS))
        }
        _ => Error::internal(cause),
    })?;

    a_product(&row)
}

/// What may be changed about something for sale.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProductChanges {
    pub name: Option<String>,
    pub about: Option<String>,
    pub price_minor: Option<i64>,
    pub on_the_shelf: Option<i32>,
    pub for_sale: Option<bool>,
}

/// Changes one.
pub async fn change(tx: &mut Tx, id: Uuid, changes: &ProductChanges) -> Result<Product> {
    let row = sqlx::query(&format!(
        "update products
            set name = coalesce($2, name),
                about = coalesce($3, about),
                price_minor = coalesce($4, price_minor),
                on_the_shelf = coalesce($5, on_the_shelf),
                for_sale = coalesce($6, for_sale),
                updated_at = now()
          where id = $1 and deleted_at is null
         returning {PRODUCT}"
    ))
    .bind(id)
    .bind(changes.name.as_deref())
    .bind(changes.about.as_deref())
    .bind(changes.price_minor)
    .bind(changes.on_the_shelf)
    .bind(changes.for_sale)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_product)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_SUCH_THING_FOR_SALE)))
}

/// Takes something off the shelf. What was already ordered keeps its own words.
pub async fn remove(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query(
        "update products set deleted_at = now(), for_sale = false
          where id = $1 and deleted_at is null",
    )
    .bind(id)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_SUCH_THING_FOR_SALE)));
    }

    Ok(())
}

/// One order, as the panel reads it.
#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub id: Uuid,
    pub number: i64,
    pub state: State,
    pub email: String,
    pub total: Money,
    pub lines: Vec<Line>,
    pub created_at: DateTime<Utc>,
}

fn an_order(row: &PgRow) -> Result<Order> {
    let state: String = row.try_get("state").map_err(Error::internal)?;
    let currency: String = row.try_get("currency").map_err(Error::internal)?;
    let minor: i64 = row.try_get("total_minor").map_err(Error::internal)?;

    Ok(Order {
        id: row.try_get("id").map_err(Error::internal)?,
        number: row.try_get("number").map_err(Error::internal)?,
        state: match state.as_str() {
            "paid" => State::Paid,
            "sent" => State::Sent,
            "called_off" => State::CalledOff,
            "given_back" => State::GivenBack,
            _ => State::Waiting,
        },
        email: row.try_get("email").map_err(Error::internal)?,
        total: Money::of(minor, Currency::parse(&currency)?),
        lines: Vec::new(),
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

const ORDER: &str = "id, number, state, email, total_minor, currency, created_at";

/// Every order, newest first.
pub async fn orders(tx: &mut Tx, state: Option<&str>, query: &Query) -> Result<Page<Order>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres: Vec<String> = Vec::new();
    let mut binds: Vec<String> = Vec::new();

    if let Some(state) = state {
        binds.push(state.to_owned());
        wheres.push(format!("state = ${}", binds.len()));
    }

    let cursor = walk.after(binds.len() + 1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let narrowed = if wheres.is_empty() {
        String::new()
    } else {
        format!("where {}", wheres.join(" and "))
    };

    let sql = format!(
        "select {ORDER} from orders {narrowed} order by {} limit {}",
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    for bind in binds {
        asking = asking.bind(bind);
    }

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(an_order)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |order| {
        vec![order.created_at.to_rfc3339(), order.id.to_string()]
    })
}

/// One order, its lines, and what it came to.
pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Order> {
    let row = sqlx::query(&format!("select {ORDER} from orders where id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_ORDER_LIKE_THAT)))?;

    let mut order = an_order(&row)?;
    order.lines = lines(tx, id).await?;

    Ok(order)
}

async fn lines(tx: &mut Tx, order: Uuid) -> Result<Vec<Line>> {
    let rows = sqlx::query(
        "select l.name, l.each_minor, l.how_many, o.currency from order_lines l
           join orders o on o.id = l.order_id
          where l.order_id = $1 order by l.created_at",
    )
    .bind(order)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            let currency: String = row.try_get("currency").map_err(Error::internal)?;
            let each: i64 = row.try_get("each_minor").map_err(Error::internal)?;
            let how_many: i32 = row.try_get("how_many").map_err(Error::internal)?;

            Ok(Line {
                name: row.try_get("name").map_err(Error::internal)?,
                each: Money::of(each, Currency::parse(&currency)?),
                how_many: u32::try_from(how_many).unwrap_or(0),
            })
        })
        .collect()
}

/// A basket, as it arrives.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Basket {
    pub email: String,
    pub wanted: Vec<Wanted>,
    pub code: Option<String>,
    /// The caller's own, and the caller's to repeat: the same request twice is
    /// one order.
    pub said_once: String,
}

/// Takes a basket and makes an order, holding the stock.
///
/// The rows are locked in the order [`reached_for`] gives, which is the whole
/// reason two people buying the same two things at once queue instead of
/// deadlocking. Nothing is read and then trusted: the shelf is checked against
/// the row that has been locked.
pub async fn place(tx: &mut Tx, basket: &Basket) -> Result<Order> {
    let email = mavi_core::email::Email::parse(&basket.email)?;

    // The same request twice is one order. Asked first, so a repeat costs one
    // query rather than a lock on everything in the basket.
    //
    // **Scoped to the address the order is for.** The key is chosen by whoever
    // is placing the order and this endpoint is open to anybody, so a key on
    // its own is something a stranger can guess — and an order read back
    // carries the address somebody typed, what they bought and what they paid.
    // Matching the address as well means a repeat still answers with the same
    // order and a guess answers with nothing.
    let already: Option<Uuid> =
        sqlx::query_scalar("select id from orders where said_once = $1 and email = $2")
            .bind(&basket.said_once)
            .bind(email.as_str())
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

    if let Some(already) = already {
        return read(tx, already).await;
    }
    let in_order = reached_for(&basket.wanted)?;

    let mut lines = Vec::with_capacity(in_order.len());
    let mut taking = Vec::with_capacity(in_order.len());

    for wanted in &in_order {
        let row = sqlx::query(&format!(
            "select {PRODUCT} from products
              where id = $1 and for_sale and deleted_at is null
                for update"
        ))
        .bind(wanted.product)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_SUCH_THING_FOR_SALE)))?;

        let product = a_product(&row)?;

        enough(&product.name, product.on_the_shelf, wanted.how_many)?;

        lines.push(Line {
            name: product.name.clone(),
            each: product.price,
            how_many: wanted.how_many,
        });

        taking.push((product.id, wanted.how_many));
    }

    let total = comes_to(&lines)?;

    let (total, code) = match &basket.code {
        Some(code) => {
            let (coupon, id, used) = a_coupon(tx, code).await?;

            (off(total, &coupon, used, Utc::now())?, Some(id))
        }
        None => (total, None),
    };

    let order = Uuid::now_v7();

    sqlx::query(
        "insert into orders (id, state, email, total_minor, currency, said_once)
         values ($1, 'waiting', $2, $3, $4, $5)",
    )
    .bind(order)
    .bind(email.as_str())
    .bind(total.minor)
    .bind(total.currency.to_string())
    .bind(&basket.said_once)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    take_off_the_shelf(tx, order, &taking, &lines).await?;

    if let Some(code) = code {
        // One row per use, so "used twice" is refused by the database rather
        // than counted and then trusted.
        sqlx::query("insert into coupon_uses (coupon_id, order_id) values ($1, $2)")
            .bind(code)
            .bind(order)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    read(tx, order).await
}

/// Off the shelf and into a hold, one line at a time.
///
/// The check constraint on the shelf refuses a negative, so a race that got
/// past the count is a transaction that fails rather than a shop that owes
/// somebody something.
async fn take_off_the_shelf(
    tx: &mut Tx,
    order: Uuid,
    taking: &[(Uuid, u32)],
    lines: &[Line],
) -> Result<()> {
    let held_until = Utc::now() + Duration::minutes(HELD_FOR_MINUTES);

    for ((product, how_many), line) in taking.iter().zip(lines) {
        sqlx::query(
            "insert into order_lines (id, order_id, product_id, name, each_minor, how_many)
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(Uuid::now_v7())
        .bind(order)
        .bind(product)
        .bind(&line.name)
        .bind(line.each.minor)
        .bind(i32::try_from(*how_many).unwrap_or(i32::MAX))
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

        // Off the shelf and into a hold. The check constraint refuses a
        // negative, so a race that got this far is a transaction that fails
        // rather than a shop that owes somebody something.
        sqlx::query("update products set on_the_shelf = on_the_shelf - $2 where id = $1")
            .bind(product)
            .bind(i32::try_from(*how_many).unwrap_or(i32::MAX))
            .execute(tx.conn())
            .await
            .map_err(|_| {
                Error::conflict(Say::of(crate::stock::NOT_THAT_MANY_LEFT).with("name", &line.name))
            })?;

        sqlx::query(
            "insert into holds (id, order_id, product_id, how_many, until)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(Uuid::now_v7())
        .bind(order)
        .bind(product)
        .bind(i32::try_from(*how_many).unwrap_or(i32::MAX))
        .bind(held_until)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    Ok(())
}

/// A code this shop honours, its id, and how many times it has been used.
async fn a_coupon(tx: &mut Tx, code: &str) -> Result<(Coupon, Uuid, i64)> {
    let row = sqlx::query(
        "select id, code, kind, percent, amount_minor, currency, at_most_uses, expires_at,
                (select count(*) from coupon_uses u where u.coupon_id = c.id) as used
           from coupons c where code = $1",
    )
    .bind(code.trim().to_uppercase())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THAT_IS_NOT_A_CODE_THIS_SHOP_HONOURS)))?;

    let kind: String = row.try_get("kind").map_err(Error::internal)?;
    let percent: Option<i32> = row.try_get("percent").map_err(Error::internal)?;
    let amount: Option<i64> = row.try_get("amount_minor").map_err(Error::internal)?;
    let currency: Option<String> = row.try_get("currency").map_err(Error::internal)?;

    let coupon = Coupon {
        code: row.try_get("code").map_err(Error::internal)?,
        kind: if kind == "percent" {
            Kind::Percent
        } else {
            Kind::Amount
        },
        percent: percent.and_then(|percent| u32::try_from(percent).ok()),
        amount: match (amount, currency) {
            (Some(minor), Some(currency)) => Some(Money::of(minor, Currency::parse(&currency)?)),
            _ => None,
        },
        at_most_uses: row.try_get("at_most_uses").map_err(Error::internal)?,
        expires_at: row.try_get("expires_at").map_err(Error::internal)?,
    };

    Ok((
        coupon,
        row.try_get("id").map_err(Error::internal)?,
        row.try_get("used").map_err(Error::internal)?,
    ))
}

/// The codes this shop honours.
pub async fn coupons(tx: &mut Tx) -> Result<Vec<Coupon>> {
    let rows: Vec<String> = sqlx::query_scalar("select code from coupons order by code")
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    let mut coupons = Vec::with_capacity(rows.len());

    for code in rows {
        let (coupon, _, _) = a_coupon(tx, &code).await?;
        coupons.push(coupon);
    }

    Ok(coupons)
}

/// What making a code asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCoupon {
    pub code: String,
    pub percent: Option<u32>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub at_most_uses: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Makes one.
pub async fn add_coupon(tx: &mut Tx, new: &NewCoupon) -> Result<Coupon> {
    // Built through the constructors, so the arithmetic rules are checked here
    // rather than trusted at the point somebody spends it.
    let mut coupon = match (new.percent, new.amount_minor, &new.currency) {
        (Some(percent), None, None) => Coupon::percent(&new.code, percent)?,
        (None, Some(minor), Some(currency)) => {
            Coupon::amount(&new.code, Money::of(minor, Currency::parse(currency)?))?
        }
        _ => return Err(Error::invalid(Say::of("that_is_a_percentage_or_an_amount"))),
    };

    coupon.at_most_uses = new.at_most_uses;
    coupon.expires_at = new.expires_at;

    sqlx::query(
        "insert into coupons (id, code, kind, percent, amount_minor, currency, at_most_uses, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::now_v7())
    .bind(&coupon.code)
    .bind(if coupon.kind == Kind::Percent { "percent" } else { "amount" })
    .bind(coupon.percent.and_then(|percent| i32::try_from(percent).ok()))
    .bind(coupon.amount.map(|amount| amount.minor))
    .bind(coupon.amount.map(|amount| amount.currency.to_string()))
    .bind(coupon.at_most_uses)
    .bind(coupon.expires_at)
    .execute(tx.conn())
    .await
    .map_err(|_| Error::conflict(Say::of("this_shop_already_honours_that_code")))?;

    Ok(coupon)
}

/// Says where an order has got to.
///
/// The machine decides whether it may go there; this writes the moment and
/// puts stock back where that is what the move means.
pub async fn move_to(tx: &mut Tx, id: Uuid, to: &str) -> Result<Order> {
    let order = read(tx, id).await?;

    let to = match to {
        "paid" => State::Paid,
        "sent" => State::Sent,
        "called_off" => State::CalledOff,
        "given_back" => State::GivenBack,
        _ => return Err(Error::invalid(Say::of(THAT_IS_NOT_WHERE_AN_ORDER_GOES))),
    };

    moves(order.state, to)?;

    let moment = match to {
        State::Paid => "paid_at",
        State::Sent => "sent_at",
        State::CalledOff => "called_off_at",
        State::GivenBack => "given_back_at",
        State::Waiting => return Err(Error::invalid(Say::of(THAT_IS_NOT_WHERE_AN_ORDER_GOES))),
    };

    sqlx::query(&format!(
        "update orders set state = $2, {moment} = now(), updated_at = now() where id = $1"
    ))
    .bind(id)
    .bind(to.as_str())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    match to {
        // Paying for it turns a hold into a sale: the stock is already gone,
        // and nothing puts it back.
        State::Paid => {
            settle(tx, id).await?;
        }
        // Called off before it was paid for, or given back afterwards: either
        // way what was held comes home.
        State::CalledOff | State::GivenBack => {
            put_back(tx, id).await?;
        }
        _ => {}
    }

    read(tx, id).await
}

/// A hold that has become a sale. Settled rather than deleted, so what was
/// held and when is still readable.
async fn settle(tx: &mut Tx, order: Uuid) -> Result<()> {
    sqlx::query("update holds set settled_at = now() where order_id = $1 and settled_at is null")
        .bind(order)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// Puts back what an order was holding, once.
///
/// `settled_at is null` in the `where` is what makes it once: an order called
/// off twice, or given back after being called off, would otherwise put the
/// same stock back twice and invent things the shop does not have.
pub async fn put_back(tx: &mut Tx, order: Uuid) -> Result<u64> {
    let holds = sqlx::query(
        "update holds set settled_at = now()
          where order_id = $1 and settled_at is null
         returning product_id, how_many",
    )
    .bind(order)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    for hold in &holds {
        let product: Uuid = hold.try_get("product_id").map_err(Error::internal)?;
        let how_many: i32 = hold.try_get("how_many").map_err(Error::internal)?;

        sqlx::query("update products set on_the_shelf = on_the_shelf + $2 where id = $1")
            .bind(product)
            .bind(how_many)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    Ok(holds.len() as u64)
}

/// Every order whose hold has run out and which nobody has paid for.
///
/// What the sweeper reads. Answered here rather than in the job, because the
/// query is about what a hold means and this is where that is written down.
pub async fn holds_that_ran_out(tx: &mut Tx) -> Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "select distinct h.order_id from holds h
           join orders o on o.id = h.order_id
          where h.settled_at is null and h.until < now() and o.state = 'waiting'",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)
}
