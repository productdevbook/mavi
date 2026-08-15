//! Two people reaching for the same shelf at the same moment.
//!
//! This is the one thing in the shop that cannot be shown by reading it, and
//! the one that gets worse the better the shop is doing. Two baskets holding
//! the same two things in different orders lock them in different orders, wait
//! for each other, and neither ever finishes — until Postgres notices and
//! kills one, so somebody sees a shop that broke for no reason they could
//! describe.
//!
//! The first test makes that happen on purpose. The second shows that reaching
//! for them in the order [`mavi_shop::reached_for`] gives does not.

use mavi_db::Db;
use mavi_shop::{Wanted, reached_for};
use sqlx::{Connection, Executor, PgConnection};
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nobody ever reached for the same shelf twice"
    );

    address
}

/// A database of this test's own, and the address to open more connections to
/// it with — two shoppers are two connections, or they are not two shoppers.
async fn fresh(named: &str) -> (Db, String) {
    let address = postgres().expect("checked by the caller");
    // Underscores, not dashes: an unquoted identifier is what `create
    // database` takes, and a dash in one is a syntax error rather than a
    // name.
    let named = format!(
        "mavi_shop_{}_{}",
        named.replace('-', "_"),
        Uuid::now_v7().simple()
    );

    let mut admin = PgConnection::connect(&address).await.expect("a connection");
    sqlx::query(&format!("create database {named}"))
        .execute(&mut admin)
        .await
        .expect("a database of its own");

    let (front, _) = address
        .rsplit_once('/')
        .expect("an address with a database");
    let its_own = format!("{front}/{named}");

    let db = Db::open(&its_own, 4).await.expect("the new database");
    db.migrate().await.expect("every migration");

    (db, its_own)
}

async fn on_the_shelf(db: &Db, slug: &str, how_many: i32) -> Uuid {
    let id = Uuid::now_v7();

    sqlx::query(
        "insert into products (id, slug, name, price_minor, currency, on_the_shelf)
         values ($1, $2, 'A Thing', 1250, 'TRY', $3)",
    )
    .bind(id)
    .bind(slug)
    .bind(how_many)
    .execute(db.pool())
    .await
    .expect("something on the shelf");

    id
}

async fn a_shopper(address: &str) -> PgConnection {
    let mut shopper = PgConnection::connect(address).await.expect("a connection");

    // So that a test that would otherwise hang for ever fails instead, and
    // says which of the two it was waiting on.
    shopper
        .execute("set lock_timeout = '10s'")
        .await
        .expect("a limit on waiting");

    shopper
        .execute("begin")
        .await
        .expect("a transaction of their own");

    shopper
}

async fn reaches_for(shopper: &mut PgConnection, product: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("select on_the_shelf from products where id = $1 for update")
        .bind(product)
        .fetch_one(shopper)
        .await
        .map(|_| ())
}

fn deadlocked(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("40P01"))
}

#[tokio::test]
async fn two_baskets_in_different_orders_deadlock() {
    if postgres().is_none() {
        return;
    }

    let (db, address) = fresh("deadlock").await;

    let salt = on_the_shelf(&db, "salt", 10).await;
    let pepper = on_the_shelf(&db, "pepper", 10).await;

    let mut one = a_shopper(&address).await;
    let mut two = a_shopper(&address).await;

    // Each takes the first thing in their own basket, and both succeed.
    reaches_for(&mut one, salt).await.expect("the salt");
    reaches_for(&mut two, pepper).await.expect("the pepper");

    // Now each reaches for what the other is holding.
    let (first, second) = tokio::join!(reaches_for(&mut one, pepper), reaches_for(&mut two, salt));

    let killed = [&first, &second]
        .iter()
        .filter(|answer| answer.as_ref().err().is_some_and(deadlocked))
        .count();

    assert_eq!(
        killed, 1,
        "two baskets in different orders did not deadlock, so this test proves nothing: \
         {first:?} {second:?}"
    );
}

#[tokio::test]
async fn two_baskets_reaching_in_the_same_order_do_not() {
    if postgres().is_none() {
        return;
    }

    let (db, address) = fresh("in-order").await;

    let salt = on_the_shelf(&db, "salt", 10).await;
    let pepper = on_the_shelf(&db, "pepper", 10).await;

    // The same two things, listed the two ways round, put in one order.
    let ones = reached_for(&[
        Wanted {
            product: salt,
            how_many: 1,
        },
        Wanted {
            product: pepper,
            how_many: 1,
        },
    ])
    .expect("an order");

    let twos = reached_for(&[
        Wanted {
            product: pepper,
            how_many: 1,
        },
        Wanted {
            product: salt,
            how_many: 1,
        },
    ])
    .expect("an order");

    assert_eq!(ones, twos);

    let mut one = a_shopper(&address).await;
    let mut two = a_shopper(&address).await;

    // The first shopper takes both, in that order.
    for wanted in &ones {
        reaches_for(&mut one, wanted.product)
            .await
            .expect("the first shopper");
    }

    // The second reaches for the first of them and waits — and gets it the
    // moment the first shopper is finished, rather than never.
    let waiting = reaches_for(&mut two, twos[0].product);
    let finishing = async {
        one.execute("commit").await.expect("the first shopper left");
    };

    let (got, ()) = tokio::join!(waiting, finishing);
    got.expect("the second shopper waited rather than deadlocking");

    for wanted in twos.iter().skip(1) {
        reaches_for(&mut two, wanted.product)
            .await
            .expect("the second shopper");
    }

    two.execute("commit")
        .await
        .expect("the second shopper left");
}

#[tokio::test]
async fn the_shelf_never_goes_below_nothing() {
    if postgres().is_none() {
        return;
    }

    let (db, _) = fresh("shelf").await;

    let salt = on_the_shelf(&db, "salt", 1).await;

    sqlx::query("update products set on_the_shelf = on_the_shelf - 1 where id = $1")
        .bind(salt)
        .execute(db.pool())
        .await
        .expect("the last one");

    // A race that gets past the check in the code is a transaction that fails
    // rather than a shop that owes somebody something.
    let refused = sqlx::query("update products set on_the_shelf = on_the_shelf - 1 where id = $1")
        .bind(salt)
        .execute(db.pool())
        .await
        .expect_err("one more than there was");

    assert!(
        refused.to_string().contains("on_the_shelf"),
        "the shelf went below nothing: {refused}"
    );
}
