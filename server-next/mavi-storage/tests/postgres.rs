use std::env;

use mavi_core::{SiteContext, SiteId};
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
async fn migrations_and_site_scope_are_exercised_against_postgres() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");

    let first_context = SiteContext::public(first);
    let mut first_tx = database.begin(&first_context).await.expect("first scope");
    sqlx::query(
        "insert into content_entries (site_id, id, kind, language, slug, title)
         values ($1, $2, 'post', 'en', 'first', 'First')",
    )
    .bind(first.into_uuid())
    .bind(uuid::Uuid::now_v7())
    .execute(first_tx.conn())
    .await
    .expect("write first site");
    first_tx.commit().await.expect("commit first site");

    let second_context = SiteContext::public(second);
    let mut second_tx = database.begin(&second_context).await.expect("second scope");
    let visible: i64 = sqlx::query_scalar("select count(*) from content_entries")
        .fetch_one(second_tx.conn())
        .await
        .expect("count second site");
    assert_eq!(visible, 0);

    let first_id: Option<uuid::Uuid> =
        sqlx::query_scalar("select id from content_entries where site_id = $1 and slug = 'first'")
            .bind(first.into_uuid())
            .fetch_optional(second_tx.conn())
            .await
            .expect("cross-site lookup");
    assert!(first_id.is_none());
    second_tx.commit().await.expect("commit second site");
}
