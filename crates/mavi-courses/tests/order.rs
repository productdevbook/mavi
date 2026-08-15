//! Reordering a course, against a real Postgres.
//!
//! The whole argument for a deferred constraint is about what happens *during*
//! a transaction, which is exactly what cannot be read off the file. So: two
//! modules swapped in one statement, which an ordinary unique constraint
//! refuses half way through, and a duplicate that is still refused when the
//! transaction tries to commit.

use mavi_courses::in_this_order;
use mavi_db::Db;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nothing was ever reordered"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_courses_{}_{}",
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
    let db = Db::open(&format!("{front}/{named}"), 2)
        .await
        .expect("the new database");

    db.migrate().await.expect("every migration");

    db
}

async fn a_course_of(db: &Db, how_many: usize) -> (Uuid, Vec<Uuid>) {
    let course = Uuid::now_v7();

    sqlx::query("insert into courses (id, slug, title) values ($1, 'a-course', 'A Course')")
        .bind(course)
        .execute(db.pool())
        .await
        .expect("a course");

    let mut modules = Vec::with_capacity(how_many);

    for place in 0..how_many {
        let id = Uuid::now_v7();

        sqlx::query(
            "insert into modules (id, course_id, title, place) values ($1, $2, 'A Part', $3)",
        )
        .bind(id)
        .bind(course)
        .bind(i32::try_from(place).expect("a place"))
        .execute(db.pool())
        .await
        .expect("a part of it");

        modules.push(id);
    }

    (course, modules)
}

#[tokio::test]
async fn two_modules_swap_places_in_one_transaction() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("swap").await;
    let (_, modules) = a_course_of(&db, 3).await;

    let dragged = vec![modules[2], modules[0], modules[1]];
    let places = in_this_order(&modules, &dragged).expect("an order");

    let mut tx = db.begin().await.expect("a transaction");

    // Every one of these writes a place another module is still in at the
    // moment it is written. Checked per row, the second statement here is
    // refused; checked at commit, the whole thing is one new order.
    for (id, place) in &places {
        sqlx::query("update modules set place = $2 where id = $1")
            .bind(id)
            .bind(place)
            .execute(tx.conn())
            .await
            .expect("a module moved");
    }

    tx.commit().await.expect("the new order");

    let in_order: Vec<Uuid> = sqlx::query("select id from modules order by place")
        .fetch_all(db.pool())
        .await
        .expect("the modules")
        .into_iter()
        .map(|row| row.get("id"))
        .collect();

    assert_eq!(in_order, dragged);
}

#[tokio::test]
async fn two_modules_in_one_place_is_still_refused_at_the_end() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("clash").await;
    let (_, modules) = a_course_of(&db, 2).await;

    let mut tx = db.begin().await.expect("a transaction");

    // Deferring the check is not dropping it. This is what would otherwise be
    // a course with two second parts and no first one.
    sqlx::query("update modules set place = 1 where id = $1")
        .bind(modules[0])
        .execute(tx.conn())
        .await
        .expect("allowed, for now");

    let refused = tx.commit().await.expect_err("two modules in one place");

    assert!(
        format!("{refused:?}").contains("one_module_to_a_place"),
        "a course with two second parts: {refused:?}"
    );
}
