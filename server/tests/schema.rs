//! Asked of the schema itself rather than of any one domain: a table added
//! without row-level security, a foreign key with nothing to read it by, or a
//! uniqueness that is global where it should be a site's own.

use sqlx::Row;

mod common;

use common::harness;

/// Tables that belong to no tenant, and have no `tenant_id` to belong by.
///
/// Being on this list is not an exemption from row-level security — a table
/// with a `tenant_id` is checked whatever else it is, which is the rule that
/// was missing when `site_settings` and `tenant_domains` sat here for months
/// without a policy and a letter went out naming the wrong site.
const CONTROL_PLANE: [&str; 6] = [
    "tenants",
    "rate_limits",
    "operators",
    "operator_sessions",
    "console_log",
    "_sqlx_migrations",
];

/// Uniqueness that is deliberately global. A session token is 256 bits from the
/// operating system: two sites cannot collide on one, and scoping it per tenant
/// would let the same token be minted twice.
const GLOBAL_UNIQUE: [&str; 5] = [
    "sessions_token_hash_key",
    "student_sessions_token_hash_key",
    "tickets_token_hash_key",
    "subscribers_token_hash_key",
    "tenants_slug_key",
];

/// Personal data the control plane holds about whoever runs the machine, which
/// is not a customer's and goes when the account does.
const OPERATORS: [&str; 3] = ["operators", "operator_sessions", "console_log"];

#[tokio::test]
async fn every_tenant_table_carries_its_tenant_and_the_policy_that_hides_it() {
    let db = harness().await;
    let mut conn = db.operator().await.expect("begin");

    let tables = sqlx::query(
        "select c.relname as name,
                c.relrowsecurity as enabled,
                c.relforcerowsecurity as forced,
                exists (select 1 from pg_policy p where p.polrelid = c.oid) as has_policy,
                exists (
                    select 1 from information_schema.columns col
                     where col.table_name = c.relname and col.column_name = 'tenant_id'
                ) as has_tenant
           from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where n.nspname = 'public' and c.relkind = 'r'",
    )
    .fetch_all(conn.conn())
    .await
    .expect("tables");

    assert!(!tables.is_empty(), "the migration made no tables");

    for table in tables {
        let name: String = table.get("name");
        let has_tenant: bool = table.get("has_tenant");

        // A table with a `tenant_id` is a site's, whatever list it is on. The
        // exemption is for tables with nothing to belong by.
        if CONTROL_PLANE.contains(&name.as_str()) && !has_tenant {
            continue;
        }

        assert!(
            has_tenant,
            "{name} is not the control plane's and has no tenant_id"
        );
        assert!(table.get::<bool, _>("enabled"), "{name} has no RLS");
        assert!(
            table.get::<bool, _>("forced"),
            "{name} does not force RLS, so whoever owns the table reads every tenant"
        );
        assert!(table.get::<bool, _>("has_policy"), "{name} has no policy");
    }
}

#[tokio::test]
async fn every_foreign_key_has_something_to_read_it_by() {
    let db = harness().await;
    let mut conn = db.operator().await.expect("begin");

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

#[tokio::test]
async fn nothing_is_unique_across_every_site_by_accident() {
    let db = harness().await;
    let mut conn = db.operator().await.expect("begin");

    let indexes = sqlx::query(
        "select i.relname as name,
                t.relname as table_name,
                exists (
                    select 1
                      from unnest(ix.indkey) as k(attnum)
                      join pg_attribute a
                        on a.attrelid = t.oid and a.attnum = k.attnum
                     where a.attname = 'tenant_id'
                ) as includes_tenant
           from pg_index ix
           join pg_class i on i.oid = ix.indexrelid
           join pg_class t on t.oid = ix.indrelid
           join pg_namespace n on n.oid = t.relnamespace
          where n.nspname = 'public' and ix.indisunique",
    )
    .fetch_all(conn.conn())
    .await
    .expect("indexes");

    for index in indexes {
        let name: String = index.get("name");
        let table: String = index.get("table_name");

        if CONTROL_PLANE.contains(&table.as_str())
            || GLOBAL_UNIQUE.contains(&name.as_str())
            || name.ends_with("_pkey")
        {
            continue;
        }

        assert!(
            index.get::<bool, _>("includes_tenant"),
            "{name} on {table} is unique across every site, so one site's value \
             can stop another site from using its own"
        );
    }
}

/// A column that holds something belonging to a person. Narrow on purpose: the
/// list is what the check is worth, and a column added to a table with any of
/// these in it is a table that needs to say how long it keeps them.
const PERSONAL: [&str; 5] = ["email", "from_ip", "user_agent", "answers", "actor_id"];

#[tokio::test]
async fn every_table_holding_somebody_s_own_data_says_how_long_it_keeps_it() {
    use mavi::kernel::retention;

    let db = harness().await;
    let mut conn = db.operator().await.expect("begin");

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
        .filter(|table| !OPERATORS.contains(&table.as_str()))
        .filter(|table| retention::policy_for(table).is_none())
        .collect();

    assert!(
        missing.is_empty(),
        "these hold somebody's own data and nothing says when it goes: {missing:?}"
    );
}

#[test]
fn what_a_policy_says_sweeps_it_is_a_job_that_exists() {
    use mavi::kernel::retention::{Keeps, POLICIES};

    let kinds = mavi::jobs::kinds(&mavi::kernel::outside::Outside::default());

    for policy in POLICIES {
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
    let mut conn = db.operator().await.expect("begin");

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

/// The rule the two leaks broke, said as a test rather than as a habit.
///
/// `site_settings` and `tenant_domains` were made with the control plane, given
/// a `tenant_id`, and left without a policy — so any site's connection could
/// read every site's row, and one did. Being on a list of control-plane tables
/// is not an exemption from row-level security; having nothing to belong by is.
#[tokio::test]
async fn nothing_with_a_tenant_id_is_left_without_a_policy() {
    let db = harness().await;
    let mut conn = db.operator().await.expect("begin");

    let unguarded = sqlx::query(
        "select c.relname as name
           from pg_class c
           join pg_namespace n on n.oid = c.relnamespace
          where n.nspname = 'public'
            and c.relkind = 'r'
            and exists (
                select 1 from information_schema.columns col
                 where col.table_name = c.relname and col.column_name = 'tenant_id'
            )
            and not exists (select 1 from pg_policy p where p.polrelid = c.oid)",
    )
    .fetch_all(conn.conn())
    .await
    .expect("tables");

    conn.commit().await.expect("commit");

    let named: Vec<String> = unguarded
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(
        named.is_empty(),
        "these say which site they belong to and nothing stops another site \
         reading them: {named:?}"
    );
}
