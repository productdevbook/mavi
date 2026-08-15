//! What something outside this crate needs in order to write a test of its
//! own against it: a database migrated once, a tenant, a role, a user — and
//! nothing more, since signing one in is already reached through the public
//! `/api/auth/session` endpoint rather than anything this module would have
//! to add.
//!
//! Behind the `testing` feature and nowhere else, so none of it is in a
//! release build. This crate's own `server/tests/common` is built on top of
//! it rather than keeping a second copy.
#![cfg(feature = "testing")]

use sqlx::Row;
use uuid::Uuid;

use crate::kernel::authz::every_grant;
use crate::kernel::db::Db;
use crate::kernel::tenant::TenantId;

/// The role the application runs requests as day to day: not a superuser, so
/// row-level security applies to it the same way it applies to a live
/// deployment, and a test that cannot fail proves nothing.
pub const APP_ROLE: &str = "mavi_app";

/// Set up once under an advisory lock, because several test binaries reach the
/// same database at the same time. The application's role is not a superuser:
/// row-level security does not apply to one, and a test that cannot fail
/// proves nothing.
pub async fn harness() -> Db {
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

/// A site to build a test against.
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

/// A role with exactly the grants asked for, so a test says what it wants
/// rather than picking from a fixed list.
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

/// Everything a site's owner can do.
pub async fn an_owner_role(db: &Db, tenant: TenantId) -> Uuid {
    a_role(db, tenant, "owner", &every_grant()).await
}

/// A user with a known password, so a test can sign them in through the
/// public `/api/auth/session` endpoint the same way anyone else would.
pub async fn a_user(db: &Db, tenant: TenantId, role_id: Uuid, password: &str) -> (Uuid, String) {
    let email = format!("someone-{}@example.test", Uuid::now_v7().simple());
    let hash = crate::kernel::password::hash(password).expect("hash");

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
