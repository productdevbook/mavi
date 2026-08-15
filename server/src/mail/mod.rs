//! What a site writes to people.
//!
//! Lists and the people on them, campaigns written once and sent to a list,
//! and the letters the machine sends one person — an invitation, a password
//! link, a receipt. Everything that leaves goes through one place, so what was
//! sent is written down whether or not it was a campaign.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::TenantId;
use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::queue::{self, Task};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::secret::Secret;
use crate::kernel::token;
use crate::kernel::types::{Email, Title};

/// How many go out in one batch. Small enough that a worker gives the queue
/// back regularly, large enough that a list of ten thousand is not ten thousand
/// jobs.
const BATCH: i64 = 100;

const UNSUBSCRIBE_LIMIT: Limit = Limit::new(30, 60);

fn mail(access: Access) -> Needs {
    Needs::new(Capability::Mail, access)
}

pub mod letters;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = letters::endpoints();
    all.extend(its_own());
    all
}

fn its_own() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/mail/lists",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::View)),
                rate: RatePolicy::None,
            },
            lists,
        )
        .gives::<Page<MailList>>(),
        Endpoint::post(
            "/api/mail/lists",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            make_list,
        )
        .takes::<NewList>()
        .gives::<MailList>(),
        Endpoint::get(
            "/api/mail/lists/{id}/subscribers",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::View)),
                rate: RatePolicy::None,
            },
            subscribers,
        )
        .gives::<Page<Subscriber>>(),
        Endpoint::post(
            "/api/mail/lists/{id}/subscribers",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            add_subscriber,
        )
        .takes::<NewSubscriber>()
        .gives::<Subscriber>(),
        Endpoint::post(
            "/api/mail/campaigns",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            make_campaign,
        )
        .takes::<NewCampaign>()
        .gives::<Campaign>(),
        Endpoint::post(
            "/api/mail/campaigns/{id}/send",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            start,
        )
        .gives::<Campaign>(),
        Endpoint::get(
            "/api/mail/campaigns",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::View)),
                rate: RatePolicy::None,
            },
            campaigns,
        )
        .gives::<Page<Campaign>>(),
        Endpoint::post(
            "/api/mail/events",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            note_event,
        )
        .takes::<Heard>(),
        Endpoint::post(
            "/api/sites/unsubscribe",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(UNSUBSCRIBE_LIMIT),
            },
            unsubscribe,
        )
        .takes::<Leaving>(),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct MailList {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Subscriber {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub state: SubscriberState,
}

impl Auditable for Subscriber {
    const SUBJECT: &'static str = "subscriber";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({ "email": self.email, "state": self.state })
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "campaign_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum CampaignState {
    Draft,
    Sending,
    Sent,
    Cancelled,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "subscriber_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SubscriberState {
    Subscribed,
    Unsubscribed,
    Bounced,
    Complained,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Campaign {
    pub id: Uuid,
    pub list_id: Uuid,
    pub subject: String,
    pub state: CampaignState,
    pub sent_count: i32,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Campaign {
    const SUBJECT: &'static str = "campaign";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "subject": self.subject,
            "state": self.state,
            "sent": self.sent_count,
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewList {
    pub name: Title,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewSubscriber {
    pub email: Email,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewCampaign {
    pub list_id: Uuid,
    pub subject: Title,
    pub body: String,
}

/// What a provider says happened afterwards, as the panel or an integration
/// passes it on. Whoever hands this over is signed in and holds `mail:write`:
/// a provider's own callback arrives at the machine's edge and is not a thing
/// a site's address answers.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Heard {
    /// What the provider calls this event. The same one twice is one.
    pub provider_ref: String,
    pub kind: String,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Leaving {
    pub token: Secret<String>,
}

async fn lists(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<MailList>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<MailList> = sqlx::query_as(
        "select id, name, created_at from mail_lists
          where ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc limit $2",
    )
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |list| {
        list.created_at.to_rfc3339()
    })))
}

fn cursor(after: Option<&str>) -> Option<DateTime<Utc>> {
    after.and_then(|value| DateTime::parse_from_rfc3339(value).ok().map(Into::into))
}

async fn make_list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewList>,
) -> Result<Audited<(StatusCode, Json<MailList>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let list: MailList = sqlx::query_as(
        "insert into mail_lists (tenant_id, name) values ($1, $2)
         returning id, name, created_at",
    )
    .bind(caller.tenant().0)
    .bind(body.name.as_str())
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a list",
        "mail_list",
        Some(&list.id.to_string()),
        &serde_json::json!({ "name": list.name }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(list))))
}

/// Who is on a list. What a screen shows beside it, and the answer to "did
/// that address actually go on".
async fn subscribers(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(list_id): Path<Uuid>,
    HttpQuery(page): HttpQuery<Query>,
) -> Result<Json<Page<Subscriber>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Subscriber> = sqlx::query_as(
        "select s.id, s.email, s.name, s.state
           from subscriber_lists l join subscribers s on s.id = l.subscriber_id
          where l.list_id = $1
            and ($2::timestamptz is null or s.created_at < $2)
          order by s.created_at desc
          limit $3",
    )
    .bind(list_id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    // The cursor is the subscriber's id rather than a moment: two addresses
    // added in the same second are otherwise one page for ever.
    Ok(Json(Page::build(&page, rows, |subscriber| {
        subscriber.id.to_string()
    })))
}

async fn add_subscriber(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(list_id): Path<Uuid>,
    Json(body): Json<NewSubscriber>,
) -> Result<Audited<(StatusCode, Json<Subscriber>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    // Somebody who already left stays left: adding them to a list again is not
    // a way to start sending to them.
    let subscriber: Subscriber = sqlx::query_as(
        "insert into subscribers (tenant_id, email, name, token_hash)
         values ($1, $2, $3, $4)
         on conflict (tenant_id, email) do update set name = coalesce(excluded.name, subscribers.name)
         returning id, email, name, state",
    )
    .bind(caller.tenant().0)
    .bind(body.email.as_str())
    .bind(body.name.as_deref())
    .bind(&token::hash(&token::generate())[..])
    .fetch_one(conn.conn())
    .await?;

    sqlx::query(
        "insert into subscriber_lists (subscriber_id, list_id, tenant_id)
         values ($1, $2, $3) on conflict do nothing",
    )
    .bind(subscriber.id)
    .bind(list_id)
    .bind(caller.tenant().0)
    .execute(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => AppError::NotFound("list"),
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "subscribed",
        None,
        Some(&subscriber),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (StatusCode::CREATED, Json(subscriber)),
    ))
}

async fn campaigns(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Campaign>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Campaign> = sqlx::query_as(
        "select id, list_id, subject, state, sent_count, created_at
           from campaigns
          where ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc limit $2",
    )
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |campaign| {
        campaign.created_at.to_rfc3339()
    })))
}

async fn make_campaign(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewCampaign>,
) -> Result<Audited<(StatusCode, Json<Campaign>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let campaign: Campaign = sqlx::query_as(
        "insert into campaigns (tenant_id, list_id, subject, body)
         values ($1, $2, $3, $4)
         returning id, list_id, subject, state, sent_count, created_at",
    )
    .bind(caller.tenant().0)
    .bind(body.list_id)
    .bind(body.subject.as_str())
    .bind(&body.body)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => AppError::NotFound("list"),
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "wrote a campaign",
        None,
        Some(&campaign),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(campaign))))
}

/// Starting one queues the first batch. Each batch queues the next, so a list
/// of ten thousand is a hundred jobs that each do a hundred, rather than one
/// job holding a worker for an hour.
async fn start(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<Json<Campaign>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let campaign: Option<Campaign> = sqlx::query_as(
        "update campaigns
            set state = 'sending', started_at = now()
          where id = $1 and state = 'draft'
         returning id, list_id, subject, state, sent_count, created_at",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let Some(campaign) = campaign else {
        return Err(AppError::Conflict(say::CAMPAIGN_NOT_ONE_CAN_STARTED.into()));
    };

    queue::enqueue(&mut conn, &SendBatch { campaign_id: id }, None).await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "started sending",
        None,
        Some(&campaign),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(campaign)))
}

/// The link at the bottom of everything sent. It takes a token rather than an
/// address, so that unsubscribing somebody else is not a matter of typing
/// their address.
async fn note_event(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<Heard>,
) -> Result<Audited<StatusCode>> {
    let fresh = heard_back(
        &state,
        caller.tenant(),
        &body.provider_ref,
        &body.kind,
        body.detail.as_deref(),
    )
    .await?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "heard back about a message",
        "mail_event",
        Some(&body.provider_ref),
        &serde_json::json!({ "kind": body.kind, "new": fresh }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        if fresh {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
    ))
}

async fn unsubscribe(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Leaving>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone: Option<(Uuid,)> = sqlx::query_as(
        "update subscribers
            set state = 'unsubscribed', unsubscribed_at = now()
          where token_hash = $1 and state <> 'unsubscribed'
         returning id",
    )
    .bind(&token::hash(body.token.expose())[..])
    .fetch_optional(conn.conn())
    .await?;

    // The same answer either way. Whether a token is one of ours is not a
    // question this will answer.
    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "unsubscribed",
        "subscriber",
        gone.map(|(id,)| id.to_string()).as_deref(),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendBatch {
    pub campaign_id: Uuid,
}

impl Task for SendBatch {
    const KIND: &'static str = "mail.send-batch";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![
        SendBatch::KIND.to_owned(),
        SweepLog::KIND.to_owned(),
        Deliver::KIND.to_owned(),
    ]
}

/// Writes a message down and asks for it to be handed over.
///
/// Everything that leaves this machine goes through here: an invitation, a
/// reset, a campaign's next hundred. What is written down is what is billed,
/// and the handing over is a job so that a request never waits on somebody
/// else's mail server.
pub async fn post(conn: &mut TenantConn, tenant: TenantId, letter: &Outgoing<'_>) -> Result<Uuid> {
    let row = sqlx::query(
        "insert into email_log
             (tenant_id, campaign_id, subscriber_id, to_email, subject, body, purpose)
         values ($1, $2, $3, $4, $5, $6, $7::mail_purpose)
         returning id",
    )
    .bind(tenant.0)
    .bind(letter.campaign_id)
    .bind(letter.subscriber_id)
    .bind(letter.to)
    .bind(letter.subject)
    .bind(letter.body)
    .bind(match letter.purpose {
        Purpose::Transactional => "transactional",
        Purpose::Campaign => "campaign",
    })
    .fetch_one(conn.conn())
    .await?;

    let id: Uuid = row.get("id");

    queue::enqueue(conn, &Deliver { email_log_id: id }, None).await?;

    Ok(id)
}

/// What a message is for. A campaign carries a way out of the list; something
/// somebody asked for by acting does not, because there is no list to leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Transactional,
    Campaign,
}

#[derive(Debug)]
pub struct Outgoing<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    pub purpose: Purpose,
    pub campaign_id: Option<Uuid>,
    pub subscriber_id: Option<Uuid>,
    pub unsubscribe: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Deliver {
    pub email_log_id: Uuid,
}

impl Task for Deliver {
    const KIND: &'static str = "mail.deliver";
}

/// Hands one message to whatever is configured to take it.
///
/// A provider that refuses for good — a malformed address, somebody who has
/// blocked this site — is a failure that is written down and not retried; one
/// that cannot be reached is an error, which the queue backs off and tries
/// again.
pub async fn deliver(state: &AppState, tenant: TenantId, task: &Deliver) -> Result<()> {
    let mut conn = state.db.tenant(tenant).await?;

    let waiting: Option<(String, String, String, Option<Uuid>)> = sqlx::query_as(
        "update email_log set attempts = attempts + 1
          where id = $1 and state = 'queued'
         returning to_email, subject, body, subscriber_id",
    )
    .bind(task.email_log_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((to, subject, body, subscriber_id)) = waiting else {
        return Ok(());
    };

    // The way out of the list travels with the message rather than being
    // pasted into its body by whoever wrote it.
    let unsubscribe = match subscriber_id {
        Some(id) => {
            let token: Option<(String,)> =
                sqlx::query_as("select encode(token_hash, 'hex') from subscribers where id = $1")
                    .bind(id)
                    .fetch_optional(conn.conn())
                    .await?;

            token.map(|(token,)| format!("/unsubscribe?token={token}"))
        }
        None => None,
    };

    conn.commit().await?;

    // The site's own mail server where it has one: what a site sends should
    // come from the site rather than from whoever runs the machine.
    let mailer = crate::plugins::mailer_for(state, tenant).await?;

    let letter = crate::kernel::mailer::Letter {
        to: to.clone(),
        subject,
        body,
        from: mailer.from(),
        unsubscribe,
    };

    let handed = mailer.hand_over(&letter).await;

    let mut conn = state.db.tenant(tenant).await?;

    match handed {
        Ok(crate::kernel::mailer::Handed::Over(reference)) => {
            sqlx::query(
                "update email_log set state = 'sent', sent_at = now(), provider_ref = $2
                  where id = $1",
            )
            .bind(task.email_log_id)
            .bind(&reference)
            .execute(conn.conn())
            .await?;
        }
        Ok(crate::kernel::mailer::Handed::Refused(why)) => {
            sqlx::query("update email_log set state = 'failed', failure = $2 where id = $1")
                .bind(task.email_log_id)
                .bind(&why)
                .execute(conn.conn())
                .await?;
        }
        Err(why) => {
            sqlx::query("update email_log set state = 'queued', failure = $2 where id = $1")
                .bind(task.email_log_id)
                .bind(why.to_string())
                .execute(conn.conn())
                .await?;

            conn.commit().await?;

            return Err(why);
        }
    }

    conn.commit().await?;

    Ok(())
}

/// What a provider says afterwards: it arrived, it bounced, somebody marked it
/// as unwanted.
///
/// A bounce is an address that does not work and a complaint is somebody who
/// does not want this; both stop the site writing to them again, and the
/// provider sending the same event twice is one row.
pub async fn heard_back(
    state: &AppState,
    tenant: TenantId,
    provider_ref: &str,
    kind: &str,
    detail: Option<&str>,
) -> Result<bool> {
    if !matches!(kind, "delivered" | "bounced" | "complained") {
        return Err(AppError::Invalid(say::NOT_SOMETHING_HAPPENS.into()));
    }

    let mut conn = state.db.tenant(tenant).await?;

    let found: Option<(Uuid, Option<Uuid>)> =
        sqlx::query_as("select id, subscriber_id from email_log where provider_ref = $1")
            .bind(provider_ref)
            .fetch_optional(conn.conn())
            .await?;

    let (log_id, subscriber_id) = match found {
        Some((id, subscriber)) => (Some(id), subscriber),
        None => (None, None),
    };

    let noted = sqlx::query(
        "insert into mail_events (tenant_id, email_log_id, kind, provider_ref, detail)
         values ($1, $2, $3, $4, $5)
         on conflict (tenant_id, provider_ref) do nothing",
    )
    .bind(tenant.0)
    .bind(log_id)
    .bind(kind)
    .bind(provider_ref)
    .bind(detail)
    .execute(conn.conn())
    .await?
    .rows_affected();

    if noted == 0 {
        // The same event twice. Nothing more happens, which is the point of
        // the key on it.
        conn.commit().await?;
        return Ok(false);
    }

    if let Some(id) = log_id
        && kind != "delivered"
    {
        sqlx::query("update email_log set state = $2::mail_state where id = $1")
            .bind(id)
            .bind(kind)
            .execute(conn.conn())
            .await?;
    }

    if let Some(id) = subscriber_id
        && kind != "delivered"
    {
        sqlx::query("update subscribers set state = $2::subscriber_state where id = $1")
            .bind(id)
            .bind(kind)
            .execute(conn.conn())
            .await?;
    }

    conn.commit().await?;

    Ok(true)
}

/// One batch, from where the last one stopped.
///
/// The cursor is a column on the campaign rather than a count of what has been
/// sent: reading everything already sent in order to find the next hundred is
/// what made this quadratic before, and a list that got slower the further it
/// got was the symptom.
pub async fn send_batch(state: &AppState, tenant: TenantId, task: &SendBatch) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let campaign: Option<(Uuid, String, String, Option<Uuid>)> = sqlx::query_as(
        "select list_id, subject, body, sent_through from campaigns
          where id = $1 and state = 'sending'
            for update",
    )
    .bind(task.campaign_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((list_id, subject, body, sent_through)) = campaign else {
        return Ok(0);
    };

    let batch = sqlx::query(
        "select s.id, s.email
           from subscriber_lists sl
           join subscribers s on s.id = sl.subscriber_id
          where sl.list_id = $1
            and s.state = 'subscribed'
            and ($2::uuid is null or s.id > $2)
          order by s.id
          limit $3",
    )
    .bind(list_id)
    .bind(sent_through)
    .bind(BATCH)
    .fetch_all(conn.conn())
    .await?;

    if batch.is_empty() {
        sqlx::query("update campaigns set state = 'sent', finished_at = now() where id = $1")
            .bind(task.campaign_id)
            .execute(conn.conn())
            .await?;

        conn.commit().await?;

        return Ok(0);
    }

    let mut last = sent_through;

    for subscriber in &batch {
        let id: Uuid = subscriber.get("id");
        let email: String = subscriber.get("email");

        // Written down before it is handed to anything, campaign or not: what
        // is not written down is not billed, and mail that was not a campaign
        // is exactly what went uncounted before.
        post(
            &mut conn,
            tenant,
            &Outgoing {
                to: &email,
                subject: &subject,
                body: &body,
                purpose: Purpose::Campaign,
                campaign_id: Some(task.campaign_id),
                subscriber_id: Some(id),
                unsubscribe: None,
            },
        )
        .await?;

        last = Some(id);
    }

    let sent = i64::try_from(batch.len()).unwrap_or(i64::MAX);

    sqlx::query(
        "update campaigns
            set sent_through = $2, sent_count = sent_count + $3
          where id = $1",
    )
    .bind(task.campaign_id)
    .bind(last)
    .bind(i32::try_from(sent).unwrap_or(i32::MAX))
    .execute(conn.conn())
    .await?;

    // The next batch, queued in the transaction that finished this one.
    queue::enqueue(
        &mut conn,
        &SendBatch {
            campaign_id: task.campaign_id,
        },
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(u64::try_from(sent).unwrap_or(0))
}

/// What a site has sent this month, campaign or not. The answer a bill is
/// written from, and one query rather than a walk through live state.
pub async fn counted(conn: &mut TenantConn, since: DateTime<Utc>) -> Result<i64> {
    let counted: (i64,) = sqlx::query_as("select count(*) from email_log where created_at >= $1")
        .bind(since)
        .fetch_one(conn.conn())
        .await?;

    Ok(counted.0)
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepLog;

impl Task for SweepLog {
    const KIND: &'static str = "mail.sweep-log";
}

/// What was sent to whom, kept for two years and then not.
pub async fn sweep_log(state: &AppState, tenant: TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let taken = sqlx::query("delete from email_log where created_at < now() - interval '730 days'")
        .execute(conn.conn())
        .await?
        .rows_affected();

    conn.commit().await?;

    Ok(taken)
}
