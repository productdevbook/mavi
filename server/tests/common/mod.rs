#![allow(dead_code)]

pub mod queries;

use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::tenant::TenantId;
use sqlx::Connection as _;
use sqlx::Row;
use uuid::Uuid;

pub const APP_ROLE: &str = "mavicms_app";

/// Set up once under an advisory lock, because several test binaries reach the
/// same database at the same time. The application's role is not a superuser:
/// row-level security does not apply to one, and a test that cannot fail proves
/// nothing.
pub async fn harness() -> Db {
    // What a failing request logged, on the test's own stderr: the body a
    // caller gets says nothing about the database on purpose, so without this
    // a five hundred is a number and nothing else.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_test_writer()
        .try_init();

    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");

    let admin = Db::connect(&url, 2).await.expect("connect");
    let mut tx = admin.operator().await.expect("begin");

    sqlx::query("select pg_advisory_xact_lock(4711)")
        .execute(tx.conn())
        .await
        .expect("lock");

    admin.migrate().await.expect("migrate");

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

    Db::connect_as(&url, 8, Some(APP_ROLE))
        .await
        .expect("connect as app")
}

/// A database nothing else is using, for the handful of tests about things
/// that are the machine's rather than a site's — who runs it, and whether it
/// has been set up. Those live in one global table, so a test that asserts
/// "there is nobody yet" cannot share a database with a test that makes
/// somebody.
pub async fn a_machine_of_its_own() -> Db {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let name = format!("v1_alone_{}", Uuid::now_v7().simple());

    // On a connection of its own rather than through `Db`: making a database
    // is the one thing Postgres refuses to do inside a transaction, and every
    // handle this codebase hands out is already in one.
    let mut making = sqlx::PgConnection::connect(&url).await.expect("connect");

    sqlx::query(&format!("create database {name}"))
        .execute(&mut making)
        .await
        .expect("a database of its own");

    let (before, _) = url.rsplit_once('/').expect("a database in the url");
    let its_own = format!("{before}/{name}");

    let db = Db::connect(&its_own, 4).await.expect("connect");
    db.migrate().await.expect("migrate");

    db
}

pub async fn a_tenant(db: &Db, host: &str) -> TenantId {
    let slug = format!("t-{}", Uuid::now_v7().simple());
    let mut tx = db.operator().await.expect("begin");

    // Making a site is the machine's own work, and the tables it writes here
    // belong to sites: saying so is what the policy asks for.
    tx.across_sites().await.expect("across sites");

    let row = sqlx::query("insert into tenants (slug, state) values ($1, 'live') returning id")
        .bind(&slug)
        .fetch_one(tx.conn())
        .await
        .expect("tenant");

    let id: Uuid = row.get("id");

    sqlx::query("insert into tenant_domains (host, tenant_id, is_primary) values ($1, $2, true)")
        .bind(host)
        .bind(id)
        .execute(tx.conn())
        .await
        .expect("domain");

    tx.commit().await.expect("commit");

    TenantId(id)
}

pub async fn a_role(db: &Db, tenant: TenantId, key: &str, grants: &[String]) -> Uuid {
    let mut conn = db.tenant(tenant).await.expect("begin");

    let row = sqlx::query(
        "insert into roles (tenant_id, key, name, grants) values ($1, $2, $3, $4) returning id",
    )
    .bind(tenant.0)
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

/// Everything a site's owner can do. Roles are data, so a test says what it
/// wants rather than picking from a fixed list.
pub async fn an_owner_role(db: &Db, tenant: TenantId) -> Uuid {
    a_role(db, tenant, "owner", &every_grant()).await
}

pub async fn a_user(db: &Db, tenant: TenantId, role_id: Uuid, password: &str) -> (Uuid, String) {
    let email = format!("someone-{}@example.test", Uuid::now_v7().simple());
    let hash = mavi::kernel::password::hash(password).expect("hash");

    let mut conn = db.tenant(tenant).await.expect("begin");

    let row = sqlx::query(
        "insert into users (tenant_id, role_id, email, name, password_hash, state)
         values ($1, $2, $3, 'A Person', $4, 'active')
         returning id",
    )
    .bind(tenant.0)
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
