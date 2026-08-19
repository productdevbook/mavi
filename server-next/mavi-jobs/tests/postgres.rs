use std::env;

use chrono::{Duration, Utc};
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_jobs::{JobKind, JobState, JobsService, LeaseOutcome};
use mavi_storage::Database;
use serde_json::json;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn jobs_are_scoped_idempotent_leased_and_dead_lettered() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let jobs = JobsService::new([JobKind::new("test.once", 1)]);
    let first_context = SiteContext::public(first_site);
    let job_id = {
        let mut tx = database.begin(&first_context).await.expect("enqueue scope");
        let payload = json!({"value": 1});
        let first = jobs
            .enqueue(
                &mut tx,
                &first_context,
                "test.once",
                &payload,
                None,
                Some("event-1"),
            )
            .await
            .expect("first enqueue");
        let duplicate = jobs
            .enqueue(
                &mut tx,
                &first_context,
                "test.once",
                &payload,
                None,
                Some("event-1"),
            )
            .await
            .expect("idempotent enqueue");
        assert_eq!(first, duplicate);
        assert!(matches!(
            jobs.enqueue(
                &mut tx,
                &first_context,
                "test.once",
                &json!({"value": 2}),
                None,
                Some("event-1"),
            )
            .await,
            Err(MaviError::Conflict { .. })
        ));
        tx.commit().await.expect("enqueue commit");
        first
    };

    let claim = {
        let mut tx = database.begin(&first_context).await.expect("claim scope");
        let claim = jobs
            .claim(&mut tx, "worker-a", &["test.once"], 30)
            .await
            .expect("claim")
            .expect("one claim");
        assert_eq!(claim.id, job_id);
        assert_eq!(claim.attempts, 1);
        tx.commit().await.expect("claim commit");
        claim
    };

    {
        let mut tx = database.begin(&first_context).await.expect("fail scope");
        assert_eq!(
            jobs.fail(&mut tx, &first_context, &claim, "provider down")
                .await
                .expect("dead letter"),
            LeaseOutcome::Completed
        );
        tx.commit().await.expect("fail commit");
    }

    {
        let mut tx = database.begin(&first_context).await.expect("read scope");
        let job = jobs.get(&mut tx, job_id).await.expect("dead job");
        assert_eq!(job.state, JobState::Dead);
        let page = jobs
            .list(
                &mut tx,
                &mavi_jobs::JobListFilter {
                    page: PageRequest {
                        after: None,
                        limit: Some(1),
                    },
                    state: Some(JobState::Dead),
                    kind: None,
                },
            )
            .await
            .expect("job list");
        assert_eq!(page.items.len(), 1);
        assert!(page.next_cursor.is_none());
        tx.commit().await.expect("read commit");
    }

    {
        let run_at = Utc::now() + Duration::minutes(1);
        let mut tx = database.begin(&first_context).await.expect("retry scope");
        let retried = jobs
            .retry_at(&mut tx, &first_context, job_id, run_at)
            .await
            .expect("retry with cooldown");
        assert_eq!(retried.state, JobState::Ready);
        assert!(retried.run_at > Utc::now() + Duration::seconds(59));
        tx.commit().await.expect("retry commit");
    }

    let (old_claim, new_claim) = {
        let mut tx = database.begin(&first_context).await.expect("enqueue scope");
        let replacement_id = jobs
            .enqueue(
                &mut tx,
                &first_context,
                "test.once",
                &json!({"replacement": true}),
                None,
                None,
            )
            .await
            .expect("replacement job");
        tx.commit().await.expect("replacement enqueue commit");

        let old_claim = {
            let mut tx = database
                .begin(&first_context)
                .await
                .expect("old claim scope");
            let claim = jobs
                .claim(&mut tx, "worker-restarted", &["test.once"], 30)
                .await
                .expect("old claim")
                .expect("old job");
            tx.commit().await.expect("old claim commit");
            claim
        };

        {
            let mut tx = database.begin(&first_context).await.expect("expire scope");
            sqlx::query(
                "update jobs
                    set claimed_until = now() - interval '1 second'
                  where id = $1",
            )
            .bind(replacement_id.into_uuid())
            .execute(tx.conn())
            .await
            .expect("expire old claim");
            tx.commit().await.expect("expire commit");
        }

        let new_claim = {
            let mut tx = database
                .begin(&first_context)
                .await
                .expect("new claim scope");
            let claim = jobs
                .claim(&mut tx, "worker-restarted", &["test.once"], 30)
                .await
                .expect("new claim")
                .expect("replacement job");
            tx.commit().await.expect("new claim commit");
            claim
        };

        assert_eq!(old_claim.id, new_claim.id);
        assert_ne!(
            old_claim.claim_token, new_claim.claim_token,
            "a restarted worker must receive a new fencing token"
        );
        (old_claim, new_claim)
    };

    {
        let mut tx = database
            .begin(&first_context)
            .await
            .expect("stale finish scope");
        assert_eq!(
            jobs.complete(&mut tx, &first_context, &old_claim)
                .await
                .expect("stale finish"),
            LeaseOutcome::Lost,
            "an old process must not complete the replacement claim"
        );
        tx.commit().await.expect("stale finish commit");
    }

    {
        let mut tx = database
            .begin(&first_context)
            .await
            .expect("new finish scope");
        assert_eq!(
            jobs.complete(&mut tx, &first_context, &new_claim)
                .await
                .expect("new finish"),
            LeaseOutcome::Completed
        );
        tx.commit().await.expect("new finish commit");
    }

    let second_context = SiteContext::public(second_site);
    let mut tx = database
        .begin(&second_context)
        .await
        .expect("isolation scope");
    assert!(
        jobs.list(&mut tx, &mavi_jobs::JobListFilter::default())
            .await
            .expect("second list")
            .items
            .is_empty()
    );
    assert!(matches!(
        jobs.get(&mut tx, job_id).await,
        Err(MaviError::NotFound { .. })
    ));
    tx.commit().await.expect("isolation commit");
}
