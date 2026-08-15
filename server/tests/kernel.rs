//! Asked of a real Postgres, because every claim here is a claim about
//! Postgres: that a policy hides a row, that `skip locked` hands two workers
//! two different jobs, that a constraint refuses a state nothing should reach.

use chrono::Duration;
use mavi::kernel::audit::{Actor, ActorKind};
use mavi::kernel::http::RequestId;
use mavi::kernel::{audit, queue};
use sqlx::Row;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::a_tenant;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Counted {
    n: i32,
}

impl queue::Task for Counted {
    const KIND: &'static str = "test.counted";
}

#[derive(Debug)]
struct Thing(Uuid);

impl audit::Auditable for Thing {
    const SUBJECT: &'static str = "thing";

    fn subject_id(&self) -> String {
        self.0.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}

#[tokio::test]
async fn a_tenant_sees_only_its_own_rows() {
    let db = harness().await;

    let one = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;
    let two = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;

    for tenant in [one, two] {
        let mut tx = db.tenant(tenant).await.expect("begin");
        audit::record(
            &mut tx,
            Actor {
                id: None,
                kind: ActorKind::System,
                request_id: RequestId(Uuid::now_v7()),
            },
            "made",
            None,
            Some(&Thing(Uuid::now_v7())),
        )
        .await
        .expect("record");
        tx.commit().await.expect("commit");
    }

    // No `where tenant_id`: the policy is what makes this one row rather than
    // two, and that is the whole claim.
    let mut tx = db.tenant(one).await.expect("begin");
    let rows = sqlx::query("select tenant_id from audit_log")
        .fetch_all(tx.conn())
        .await
        .expect("select");

    assert_eq!(rows.len(), 1, "a tenant read another tenant's audit log");
    assert_eq!(rows[0].get::<Uuid, _>("tenant_id"), one.0);
}

#[tokio::test]
async fn two_workers_take_two_different_jobs() {
    let db = harness().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;

    let mut tx = db.tenant(tenant).await.expect("begin");
    let first = queue::enqueue(&mut tx, &Counted { n: 1 }, None)
        .await
        .expect("enqueue");
    let second = queue::enqueue(&mut tx, &Counted { n: 2 }, None)
        .await
        .expect("enqueue");
    tx.commit().await.expect("commit");

    let kinds = vec![<Counted as queue::Task>::KIND.to_owned()];
    let a = queue::claim_within(&db, "a", &kinds, Some(tenant))
        .await
        .expect("claim")
        .expect("a job");
    let b = queue::claim_within(&db, "b", &kinds, Some(tenant))
        .await
        .expect("claim")
        .expect("a job");

    assert_ne!(a.id, b.id, "two workers were handed the same job");
    assert_eq!([first, second].iter().filter(|id| **id == a.id).count(), 1);

    assert!(
        queue::claim_within(&db, "c", &kinds, Some(tenant))
            .await
            .expect("claim")
            .is_none()
    );

    queue::complete(&db, a.id).await.expect("complete");
    queue::fail(&db, b.id, "nope", Duration::seconds(0))
        .await
        .expect("fail");

    // The failed one is ready again; the finished one is not offered twice.
    let again = queue::claim_within(&db, "d", &kinds, Some(tenant))
        .await
        .expect("claim")
        .expect("a job");
    assert_eq!(again.id, b.id);
    assert_eq!(again.attempts, 2);
}

#[tokio::test]
async fn a_job_that_keeps_failing_is_given_up_on() {
    let db = harness().await;
    let tenant = a_tenant(&db, &format!("{}.example", Uuid::now_v7().simple())).await;

    let mut tx = db.tenant(tenant).await.expect("begin");
    queue::enqueue(&mut tx, &Counted { n: 0 }, None)
        .await
        .expect("enqueue");
    tx.commit().await.expect("commit");

    let kinds = vec![<Counted as queue::Task>::KIND.to_owned()];
    let mut last = None;

    for _ in 0..5 {
        let job = queue::claim_within(&db, "w", &kinds, Some(tenant))
            .await
            .expect("claim")
            .expect("a job");
        queue::fail(&db, job.id, "still no", Duration::seconds(0))
            .await
            .expect("fail");
        last = Some(job.id);
    }

    assert!(
        queue::claim_within(&db, "w", &kinds, Some(tenant))
            .await
            .expect("claim")
            .is_none(),
        "a job past its attempts was offered again"
    );

    let mut tx = db.tenant(tenant).await.expect("begin");
    let row = sqlx::query("select state::text as state, last_error from jobs where id = $1")
        .bind(last.expect("a job"))
        .fetch_one(tx.conn())
        .await
        .expect("select");

    assert_eq!(row.get::<String, _>("state"), "dead");
    assert_eq!(row.get::<String, _>("last_error"), "still no");
}

#[test]
fn a_page_says_whether_there_is_another() {
    use mavi::kernel::page::{Page, Query};

    let query = Query {
        after: None,
        limit: Some(2),
    };

    let page = Page::build(&query, vec![1, 2, 3], ToString::to_string);
    assert_eq!(page.items, vec![1, 2]);
    assert_eq!(page.next.as_deref(), Some("2"));

    let page = Page::build(&query, vec![1, 2], ToString::to_string);
    assert_eq!(page.items, vec![1, 2]);
    assert!(page.next.is_none());

    assert_eq!(Query::default().limit(), 25);
    assert_eq!(
        Query {
            after: None,
            limit: Some(5_000)
        }
        .limit(),
        100
    );
}

/// A key given and misread used to become an invented one at the same version
/// a real first key has, so what a machine sealed after that was sealed under
/// something nobody held.
#[test]
fn a_key_that_was_given_and_cannot_be_read_is_no_keyring_at_all() {
    use mavi::kernel::crypto::Keyring;

    for wrong in [
        "",
        "  ",
        "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=",
        "one:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=",
        "1:BwcHBwcHBwcHBwc  HBwcHBwcHBwcHBwcHBwcHBwcHBwc=",
        "1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
        "1:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=,2:",
    ] {
        assert!(
            Keyring::given(Some(wrong)).is_err(),
            "a keyring was built out of a key that cannot be read: {wrong:?}"
        );
    }

    // Nothing given is the other case, and stays as it was: a machine with
    // nothing sealed on it yet.
    assert!(Keyring::given(None).is_ok());
    assert!(Keyring::given(Some("1:BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=")).is_ok());
}

/// The thing a test could not do before this: choose what a state's mailer is
/// without setting a variable every other test in the process would see too.
#[tokio::test]
async fn a_state_can_be_handed_a_mailer_that_goes_nowhere() {
    use mavi::kernel::config::Config;
    use mavi::kernel::http::AppState;
    use mavi::kernel::mailer::{Letter, Mailer, Recorder};

    let db = harness().await;
    let post = Recorder::default();

    let state = AppState::new_with(
        db,
        Config {
            mailer: Mailer::Recorded(post.clone()),
            ..Config::nothing_configured()
        },
    );

    state
        .mailer
        .hand_over(&Letter {
            to: "somebody@example.test".to_owned(),
            subject: "a subject".to_owned(),
            body: "a body".to_owned(),
            from: state.mailer.from(),
            unsubscribe: None,
        })
        .await
        .expect("handed over");

    assert_eq!(post.all().len(), 1);

    // Nothing else was configured either, and nothing pretends to be: an
    // order made against this state has no way to be paid for and says so.
    assert_eq!(state.payments.name(), "none");
}
