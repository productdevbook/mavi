//! What something outside this crate needs in order to write a test of its
//! own against it: a machine, a role, a user — and nothing more,
//! since signing one in is already reached through the public
//! `/api/auth/session` endpoint rather than anything this module would have
//! to add.
//!
//! Behind the `testing` feature and nowhere else, so none of it is in a
//! release build. This crate's own `server/tests/common` is built on top of
//! it rather than keeping a second copy.
#![cfg(feature = "testing")]

use sqlx::{Connection as _, PgConnection, Row};
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::kernel::authz::every_grant;
use crate::kernel::db::Db;

/// The role the application runs requests as day to day: not a superuser, so
/// row-level security applies to it the same way it applies to a live
/// deployment, and a test that cannot fail proves nothing.
pub const APP_ROLE: &str = "mavi_app";

/// How many databases exist at once. A test holds one for as long as its
/// process lives, so this is how many tests run side by side rather than how
/// many there are — and a suite of three hundred leaves a handful behind
/// rather than three hundred.
const AT_ONCE: i32 = 32;

/// Two locks: one on a database, held for the life of a test, and one on the
/// work of getting a database into shape, held for the moment that takes.
const LEASE: i32 = 4712;
const SHAPE: i32 = 4711;

/// Asked for more than once in one test, the same one comes back — the second
/// call is a test wanting the database it already has, not a second machine.
static HELD: OnceCell<Db> = OnceCell::const_new();

/// A machine of this test's own.
///
/// One installation is one site, and nothing in a request says which site it
/// is about any more, so two tests cannot share a database and still be two
/// installations. Migrating one per test would run every migration once per
/// test; instead a few are kept and leased, and a test finds the one it is
/// handed emptied of whatever the last holder left in it.
pub async fn harness() -> Db {
    HELD.get_or_init(lease).await.clone()
}

/// A second machine, for the handful of tests that genuinely need two
/// installations rather than two sites — something taken out of one and read
/// into the other, which is what moving a site somewhere else is.
///
/// A lease of its own, so it is another database rather than another row in
/// this one. Everything else wants [`harness`], which is the machine the test
/// itself is.
pub async fn another_machine() -> Db {
    lease().await
}

async fn lease() -> Db {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let (server, _) = url.rsplit_once('/').expect("a database in the url");

    // On a connection of its own rather than through `Db`: making a database
    // is the one thing Postgres refuses to do inside a transaction, and every
    // handle this crate hands out is already in one.
    let mut holding = PgConnection::connect(&url).await.expect("connect");

    let slot = claim(&mut holding).await;
    let name = format!("mavi_test_{slot}");
    let its_own = format!("{server}/{name}");

    // Several test binaries reach the same server at once and all want the
    // same thing done to it.
    sqlx::query("select pg_advisory_lock($1)")
        .bind(SHAPE)
        .execute(&mut holding)
        .await
        .expect("wait for the shape");

    // Emptied rather than remade, and nothing can leave anything behind that
    // emptying it would miss: a leased database is only ever handed out as the
    // role a request is served as, and that role cannot make a table.
    if !exists(&mut holding, &name).await {
        sqlx::query(&format!("create database {name}"))
            .execute(&mut holding)
            .await
            .expect("a database of its own");
    }

    let admin = Db::connect(&its_own, 2).await.expect("connect");
    shape(&admin).await;

    sqlx::query("select pg_advisory_unlock($1)")
        .bind(SHAPE)
        .execute(&mut holding)
        .await
        .expect("let go of the shape");

    empty(&admin).await;
    admin.pool().close().await;

    // Leaked on purpose: the lock is the lease, Postgres lets go of it when
    // the connection closes, and the connection closing is what would hand
    // this database to another test while this one is still in it.
    let _lease: &'static mut PgConnection = Box::leak(Box::new(holding));

    Db::connect_as(&its_own, 8, Some(APP_ROLE))
        .await
        .expect("connect as app")
}

/// A database nothing else is using. Nothing blocks: a slot somebody holds is
/// a test still running, and the next one along is as good.
async fn claim(holding: &mut PgConnection) -> i32 {
    loop {
        for slot in 0..AT_ONCE {
            let (claimed,): (bool,) = sqlx::query_as("select pg_try_advisory_lock($1, $2)")
                .bind(LEASE)
                .bind(slot)
                .fetch_one(&mut *holding)
                .await
                .expect("ask for a database");

            if claimed {
                return slot;
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

async fn exists(holding: &mut PgConnection, name: &str) -> bool {
    let (there,): (bool,) =
        sqlx::query_as("select exists (select 1 from pg_database where datname = $1)")
            .bind(name)
            .fetch_one(&mut *holding)
            .await
            .expect("ask after a database");

    there
}

/// The migrations, and the role a request is served as. Every time rather than
/// once: a migration added since this database was last leased is one the
/// role has no grant on.
async fn shape(db: &Db) {
    db.migrate().await.expect("migrate");

    let mut tx = db.begin().await.expect("begin");

    sqlx::query(&format!(
        "do $$ begin
             if not exists (select from pg_roles where rolname = '{APP_ROLE}') then
                 create role {APP_ROLE} nologin;
             end if;
         end $$;"
    ))
    .execute(tx.conn())
    .await
    .expect("role");

    for grant in [
        format!("grant usage on schema public to {APP_ROLE}"),
        format!(
            "grant select, insert, update, delete on all tables in schema public to {APP_ROLE}"
        ),
    ] {
        sqlx::query(&grant).execute(tx.conn()).await.expect("grant");
    }

    tx.commit().await.expect("commit");
}

/// Whatever the last test to hold this database left in it. The tables rather
/// than the database, because making one costs a second and emptying one
/// costs nothing — and what has run is not a row anybody wrote.
async fn empty(db: &Db) {
    let mut tx = db.begin().await.expect("begin");

    sqlx::query(
        "do $$
         declare tables text;
         begin
             select string_agg(format('%I', tablename), ', ')
               into tables
               from pg_tables
              where schemaname = 'public' and tablename <> '_sqlx_migrations';

             if tables is not null then
                 execute 'truncate table ' || tables || ' cascade';
             end if;
         end $$;",
    )
    .execute(tx.conn())
    .await
    .expect("empty");

    tx.commit().await.expect("commit");
}

/// A role with exactly the grants asked for, so a test says what it wants
/// rather than picking from a fixed list.
pub async fn a_role(db: &Db, key: &str, grants: &[String]) -> Uuid {
    let mut conn = db.begin().await.expect("begin");

    let row = sqlx::query("insert into roles (key, name, grants) values ($1, $2, $3) returning id")
        .bind(key)
        .bind(key)
        .bind(grants)
        .fetch_one(conn.conn())
        .await
        .expect("role");

    let id: Uuid = row.get("id");
    conn.commit().await.expect("commit");

    id
}

/// Everything a site's owner can do.
pub async fn an_owner_role(db: &Db) -> Uuid {
    a_role(db, "owner", &every_grant()).await
}

/// A user with a known password, so a test can sign them in through the
/// public `/api/auth/session` endpoint the same way anyone else would.
pub async fn a_user(db: &Db, role_id: Uuid, password: &str) -> (Uuid, String) {
    let email = format!("someone-{}@example.test", Uuid::now_v7().simple());
    let hash = crate::kernel::password::hash(password).expect("hash");

    let mut conn = db.begin().await.expect("begin");

    let row = sqlx::query(
        "insert into users (role_id, email, name, password_hash, state)
         values ($1, $2, 'A Person', $3, 'active')
         returning id",
    )
    .bind(role_id)
    .bind(&email)
    .bind(&hash)
    .fetch_one(conn.conn())
    .await
    .expect("user");

    let id: Uuid = row.get("id");
    conn.commit().await.expect("commit");

    (id, email)
}
