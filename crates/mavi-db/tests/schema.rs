//! The schema, against a real Postgres.
//!
//! Every constraint in these migrations was written as a sentence about what
//! cannot happen, and until something ran them they were sentences. Most of
//! what has ever been wrong in this system looked right in the file and was
//! only visible by running it: a regular expression that matched one character
//! less than it meant to, a partial index that made the address free when it
//! should not, a cascade that took a row's children and left its grandchildren.
//!
//! Each test gets a database of its own. Two tests sharing one would be two
//! installations sharing one site, and the second to write would be failing on
//! the first one's rows.

use mavi_db::Db;
use sqlx::{Connection, PgConnection, Row};
use uuid::Uuid;

/// Where Postgres is. Absent, these do not run — except in CI, where a test
/// that quietly does not run is worse than no test at all.
fn postgres() -> Option<String> {
    match std::env::var("TEST_DATABASE_URL") {
        Ok(address) => Some(address),
        Err(_) => {
            assert!(
                std::env::var("CI").is_err(),
                "CI has no TEST_DATABASE_URL, so the schema was never run"
            );

            None
        }
    }
}

/// A database of this test's own, migrated.
async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");

    let named = format!("mavi_{named}_{}", Uuid::now_v7().simple());

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

/// The one word that says what happened, so a test can assert on the rule
/// rather than on Postgres's wording.
fn broke(error: &sqlx::Error) -> String {
    match error {
        sqlx::Error::Database(db) => db
            .constraint()
            .map_or_else(|| db.message().to_owned(), ToOwned::to_owned),
        other => panic!("not the database's refusal: {other}"),
    }
}

async fn a_writing(db: &Db, slug: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "insert into writings (id, kind, language, slug, title) values ($1, 'post', 'en', $2, 'A Title')",
    )
    .bind(Uuid::now_v7())
    .bind(slug)
    .execute(db.pool())
    .await
    .map(|_| ())
}

#[tokio::test]
async fn every_migration_applies_to_an_empty_database() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("all").await;

    // Named rather than counted: a count says nothing about which one is
    // missing, and this is the list every other test here stands on.
    for table in [
        "writings",
        "terms",
        "filed_under",
        "files",
        "forms",
        "filled",
        "letters",
        "readers",
        "mail_lists",
        "on_a_list",
    ] {
        let there: bool = sqlx::query_scalar("select to_regclass($1) is not null")
            .bind(table)
            .fetch_one(db.pool())
            .await
            .expect("an answer");

        assert!(there, "{table} is not in the schema the migrations build");
    }
}

#[tokio::test]
async fn an_address_in_use_is_taken_and_one_thrown_away_is_free() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("address").await;

    a_writing(&db, "hello").await.expect("the first");

    let again = a_writing(&db, "hello").await.expect_err("the second");
    assert_eq!(broke(&again), "writings_address");

    sqlx::query("update writings set deleted_at = now() where slug = 'hello'")
        .execute(db.pool())
        .await
        .expect("into the bin");

    // The reason the index is partial, stated as the thing somebody actually
    // does: a page thrown away last year does not hold its address for ever.
    a_writing(&db, "hello").await.expect("the address is free");
}

#[tokio::test]
async fn a_published_row_says_when_and_a_draft_does_not() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("published").await;

    let half = sqlx::query(
        "insert into writings (id, kind, language, slug, title, state)
         values ($1, 'post', 'en', 'out', 'A Title', 'published')",
    )
    .bind(Uuid::now_v7())
    .execute(db.pool())
    .await
    .expect_err("published, with no date");

    assert_eq!(broke(&half), "published_says_when");
}

#[tokio::test]
async fn a_tag_cannot_be_given_a_parent() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("terms").await;

    let category = Uuid::now_v7();
    sqlx::query(
        "insert into terms (id, sort, language, slug, name)
         values ($1, 'category', 'en', 'news', 'News')",
    )
    .bind(category)
    .execute(db.pool())
    .await
    .expect("a category");

    let flat = sqlx::query(
        "insert into terms (id, sort, language, slug, name, parent)
         values ($1, 'tag', 'en', 'blue', 'Blue', $2)",
    )
    .bind(Uuid::now_v7())
    .bind(category)
    .execute(db.pool())
    .await
    .expect_err("a tag with a parent");

    assert_eq!(broke(&flat), "only_a_category_has_a_parent");
}

#[tokio::test]
async fn a_file_is_not_kept_under_the_name_somebody_chose() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("files").await;

    let kept = |at: &'static str| {
        sqlx::query(
            "insert into files (id, kind, mime, name, kept_at, bytes)
             values ($1, 'image', 'image/png', 'holiday.png', $2, 100)",
        )
        .bind(Uuid::now_v7())
        .bind(at)
        .execute(db.pool())
    };

    // What the code makes: two characters of the id, then the rest of it.
    kept("ab/cdef0123456789abcdef01234567.png")
        .await
        .expect("where the code puts one");

    for wrong in [
        "holiday.png",
        "../../etc/passwd",
        "ab/../cdef0123456789abcdef012345.png",
    ] {
        let refused = kept(wrong).await.expect_err(wrong);

        assert!(
            broke(&refused).contains("kept_at"),
            "{wrong} was accepted as somewhere to keep a file"
        );
    }
}

#[tokio::test]
async fn what_people_sent_a_form_goes_when_the_form_does() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("forms").await;

    let form = Uuid::now_v7();
    sqlx::query("insert into forms (id, slug, name) values ($1, 'contact', 'Contact')")
        .bind(form)
        .execute(db.pool())
        .await
        .expect("a form");

    sqlx::query("insert into filled (id, form_id, answers) values ($1, $2, '{}'::jsonb)")
        .bind(Uuid::now_v7())
        .bind(form)
        .execute(db.pool())
        .await
        .expect("something sent");

    sqlx::query("delete from forms where id = $1")
        .bind(form)
        .execute(db.pool())
        .await
        .expect("the form goes");

    let left: i64 = sqlx::query("select count(*) from filled")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    // Somebody's own words, kept against a form that no longer exists, is a
    // row nothing in the panel can reach and nothing ever deletes.
    assert_eq!(
        left, 0,
        "what people sent outlived the form they sent it to"
    );
}

#[tokio::test]
async fn one_address_cannot_be_two_readers() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("readers").await;

    let reader = |email: &'static str, way_out: &'static [u8]| {
        sqlx::query("insert into readers (id, email, way_out) values ($1, $2, $3)")
            .bind(Uuid::now_v7())
            .bind(email)
            .bind(way_out)
            .execute(db.pool())
    };

    reader("someone@example.test", b"one").await.expect("one");

    let twice = reader("someone@example.test", b"two")
        .await
        .expect_err("the same address again");
    assert_eq!(broke(&twice), "readers_address");

    // The fold is what makes that index mean anything: without it the same
    // address written with a capital is a second reader, and whichever of the
    // two a letter reaches is whichever the mail host decided.
    let shouted = reader("SomeOne@Example.test", b"three")
        .await
        .expect_err("an address that was not folded");
    assert!(broke(&shouted).contains("email"), "{}", broke(&shouted));
}
