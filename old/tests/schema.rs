//! Asked of the schema itself rather than of any one domain: a foreign key
//! with nothing to read it by, a table holding somebody's own data and saying
//! nothing about how long it keeps it, a retention policy naming a sweep that
//! does not exist, or a soft delete nothing can undo.
//!
//! Three checks used to live here and were about the tenancy: that every table
//! carried a `tenant_id`, that every one of those had a policy hiding it, and
//! that no uniqueness was global by accident. All three became questions with
//! no subject when the column went, and they are recorded in the migration
//! that removed it rather than kept here as tests that cannot fail.

use sqlx::Row;

mod common;

use common::harness;

#[tokio::test]
async fn every_foreign_key_has_something_to_read_it_by() {
    let db = harness().await;
    let mut conn = db.begin().await.expect("begin");

    let unindexed = sqlx::query(
        "select con.conrelid::regclass::text as table_name, con.conname as name
           from pg_constraint con
          where con.contype = 'f'
            and not exists (
                select 1
                  from pg_index i
                 where i.indrelid = con.conrelid
                   and i.indkey[0] = con.conkey[1]
            )",
    )
    .fetch_all(conn.conn())
    .await
    .expect("constraints");

    let named: Vec<String> = unindexed
        .iter()
        .map(|row| {
            format!(
                "{}.{}",
                row.get::<String, _>("table_name"),
                row.get::<String, _>("name")
            )
        })
        .collect();

    assert!(
        named.is_empty(),
        "a delete on the other side of these turns into a table scan: {named:?}"
    );
}

/// A column that holds something belonging to a person. Narrow on purpose: the
/// list is what the check is worth, and a column added to a table with any of
/// these in it is a table that needs to say how long it keeps them.
const PERSONAL: [&str; 5] = ["email", "from_ip", "user_agent", "answers", "actor_id"];

#[tokio::test]
async fn every_table_holding_somebody_s_own_data_says_how_long_it_keeps_it() {
    use mavi::retention;

    let db = harness().await;
    let mut conn = db.begin().await.expect("begin");

    let holders = sqlx::query(
        "select distinct table_name
           from information_schema.columns
          where table_schema = 'public'
            and column_name = any($1)",
    )
    .bind(PERSONAL.map(str::to_owned).to_vec())
    .fetch_all(conn.conn())
    .await
    .expect("columns");

    let missing: Vec<String> = holders
        .iter()
        .map(|row| row.get::<String, _>("table_name"))
        .filter(|table| retention::policy_for(table).is_none())
        .collect();

    assert!(
        missing.is_empty(),
        "these hold somebody's own data and nothing says when it goes: {missing:?}"
    );
}

/// Checked through [`retention::all`], not `POLICIES` directly, so the same
/// rule reaches whatever an outside crate hands in through `Outside::policies`
/// — `tests/outside.rs` runs this exact check again with a policy of its own,
/// against an `Outside` that also carries the job it names.
#[test]
fn what_a_policy_says_sweeps_it_is_a_job_that_exists() {
    use mavi::kernel::outside::Outside;
    use mavi::retention::{self, Keeps};

    let outside = Outside::default();
    let kinds = mavi::jobs::kinds(&outside);

    for policy in retention::all(&outside) {
        if matches!(policy.keeps, Keeps::WithItsSubject) {
            continue;
        }

        assert!(
            kinds.contains(&policy.swept_by.to_owned()),
            "{} says {} takes it away, and there is no such job",
            policy.table,
            policy.swept_by
        );
    }
}

/// Somebody's account and a student's are removed rather than thrown away: what
/// putting one back would mean is a decision nobody has made, and doing it
/// quietly would put a suspended account back in the panel.
const NOT_THROWN_AWAY: [&str; 2] = ["users", "students"];

/// A table that soft-deletes and is not in the trash registry is a table whose
/// rows nobody can put back and nothing ever empties.
#[tokio::test]
async fn everything_that_soft_deletes_is_in_the_trash() {
    use mavi::kernel::trash;

    let db = harness().await;
    let mut conn = db.begin().await.expect("begin");

    let soft = sqlx::query(
        "select table_name from information_schema.columns
          where table_schema = 'public' and column_name = 'deleted_at'",
    )
    .fetch_all(conn.conn())
    .await
    .expect("columns");

    let missing: Vec<String> = soft
        .iter()
        .map(|row| row.get::<String, _>("table_name"))
        .filter(|table| !NOT_THROWN_AWAY.contains(&table.as_str()))
        .filter(|table| trash::of(table).is_none())
        .collect();

    assert!(
        missing.is_empty(),
        "these soft-delete and are not in the trash: {missing:?}"
    );
}
