//! Whether a site is well, and whether its addresses work.
//!
//! Everything here is a question somebody asks after something has gone wrong,
//! so the answers are kept where a screen can show them rather than worked out
//! by whoever is on the phone.

use axum::Json;
use axum::extract::State as Injected;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::queue::{self, Task};

fn settings(access: Access) -> Needs {
    Needs::new(Capability::Settings, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/health",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::View)),
                rate: RatePolicy::None,
            },
            site,
        )
        .gives::<Health>(),
        Endpoint::get(
            "/api/domains",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::View)),
                rate: RatePolicy::None,
            },
            domains,
        )
        .gives::<Vec<Domain>>(),
    ]
}

/// One thing that is either well or not, in a word a screen can show and a
/// name a panel can translate.
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Check {
    /// What was looked at, as a key rather than a sentence.
    pub what: &'static str,
    pub well: bool,
    /// What was found, where a number is what makes it interesting.
    pub detail: serde_json::Value,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Health {
    pub well: bool,
    pub checks: Vec<Check>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Domain {
    pub host: String,
    /// Null where nothing has looked yet, which is not the same as "broken".
    pub resolves: Option<bool>,
    pub answered: Option<bool>,
    pub note: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
}

/// Looking at the installation's own address. On a schedule rather than on a
/// request: it is somebody else's DNS being asked, and a screen should not wait
/// for that.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CheckDomains;

impl Task for CheckDomains {
    const KIND: &'static str = "domains.check";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![CheckDomains::KIND.to_owned()]
}

async fn site(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
) -> Result<Json<Health>> {
    let mut conn = state.db.begin().await?;
    let checks = look_at(&mut conn).await?;
    conn.commit().await?;

    Ok(Json(Health {
        well: checks.iter().all(|check| check.well),
        checks,
    }))
}

/// The handful of things that are actually worth asking, in one query each.
///
/// Not everything that could be measured: a health screen with forty rows on it
/// is one nobody reads, and the ones here are the ones that have gone wrong.
pub async fn look_at(conn: &mut Tx) -> Result<Vec<Check>> {
    let mut checks = Vec::with_capacity(5);

    let published: (i64, Option<DateTime<Utc>>) = sqlx::query_as(
        "select count(*), max(published_at) from posts
          where state = 'published' and deleted_at is null",
    )
    .fetch_one(conn.conn())
    .await?;

    checks.push(Check {
        what: "site.has-pages",
        well: published.0 > 0,
        detail: serde_json::json!({ "published": published.0, "last": published.1 }),
    });

    let publishing: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "select state::text, created_at from publishes order by created_at desc limit 1",
    )
    .fetch_optional(conn.conn())
    .await?;

    checks.push(Check {
        what: "site.last-publish",
        // Nothing published yet is not a fault; a publish that failed is.
        well: publishing
            .as_ref()
            .is_none_or(|(state, _)| state != "failed"),
        detail: publishing.map_or(
            serde_json::Value::Null,
            |(state, at)| serde_json::json!({ "state": state, "at": at }),
        ),
    });

    let issues: (i64,) =
        sqlx::query_as("select count(*) from page_issues where weight = 'warning'")
            .fetch_one(conn.conn())
            .await?;

    checks.push(Check {
        what: "pages.warnings",
        well: issues.0 == 0,
        detail: serde_json::json!({ "warnings": issues.0 }),
    });

    let mail: Option<(bool, Option<bool>)> =
        sqlx::query_as("select enabled, working from plugins where key = 'mail'")
            .fetch_optional(conn.conn())
            .await?;

    checks.push(Check {
        what: "mail.working",
        // A site sending through the machine's own server is not asked about:
        // what this checks is a site's own, once it has one.
        well: mail
            .as_ref()
            .is_none_or(|(enabled, working)| !enabled || working.unwrap_or(true)),
        detail: mail.map_or(
            serde_json::Value::Null,
            |(enabled, working)| serde_json::json!({ "enabled": enabled, "working": working }),
        ),
    });

    let addresses: (i64, i64) =
        sqlx::query_as("select count(*), count(*) filter (where not answered) from domain_checks")
            .fetch_one(conn.conn())
            .await?;

    checks.push(Check {
        what: "domains.answering",
        well: addresses.1 == 0,
        detail: serde_json::json!({ "checked": addresses.0, "not_answering": addresses.1 }),
    });

    Ok(checks)
}

async fn domains(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<Domain>>> {
    let mut conn = state.db.begin().await?;

    // One address, and it is the one the process was started with rather than a
    // row somebody added: an installation that cannot name its own address does
    // not start at all, so there is nowhere else for this to come from and
    // nothing that can drift out of step with it.
    let host = state.address.host().to_owned();

    let looked: Option<Domain> = sqlx::query_as(
        "select host, resolves, answered, note, checked_at
           from domain_checks where host = $1",
    )
    .bind(&host)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    // Nothing has looked yet is its own answer, and not the same as broken.
    Ok(Json(vec![looked.unwrap_or(Domain {
        host,
        resolves: None,
        answered: None,
        note: None,
        checked_at: None,
    })]))
}

/// Asks whether this installation's own address resolves, and whether this
/// machine answered on it.
///
/// What it cannot ask about is the certificate: the certificates are somebody
/// else's on this machine — the ingress asks for them and holds them — and a
/// second thing guessing at their expiry would be a second thing to be wrong.
pub async fn check_domains(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;

    let mut looked = 0;

    for host in [state.address.host().to_owned()] {
        let found = look_up(state, &host).await;

        sqlx::query(
            "insert into domain_checks (host, resolves, answered, note, checked_at)
             values ($1, $2, $3, $4, now())
             on conflict (host) do update set
                resolves = excluded.resolves,
                answered = excluded.answered,
                note = excluded.note,
                checked_at = excluded.checked_at",
        )
        .bind(&host)
        .bind(found.resolves)
        .bind(found.answered)
        .bind(found.note.as_deref())
        .execute(conn.conn())
        .await?;

        looked += 1;
    }

    conn.commit().await?;

    Ok(looked)
}

struct Found {
    resolves: bool,
    answered: bool,
    note: Option<String>,
}

/// Asking one address whether this machine answers on it.
///
/// Through the kernel's own outbound door rather than a client of its own: an
/// address a site attached is somewhere somebody else chose, so it is resolved
/// once, checked against everything that is not the public internet, and then
/// pinned to what it resolved to — otherwise "check my domain" is a way to ask
/// this machine to fetch from inside its own network.
async fn look_up(state: &AppState, host: &str) -> Found {
    // An address with a port on it is a test's; nothing anybody visits has one.
    let scheme = if state.allow_private_destinations {
        "http"
    } else {
        "https"
    };

    let reaching = crate::kernel::outbound::reach(
        &format!("{scheme}://{host}/healthz"),
        std::time::Duration::from_secs(10),
        state.allow_private_destinations,
    )
    .await;

    let reaching = match reaching {
        Ok(reaching) => reaching,
        Err(why) => {
            return Found {
                resolves: false,
                answered: false,
                note: Some(why.to_string()),
            };
        }
    };

    match reaching.client.get(reaching.url).send().await {
        Ok(answer) if answer.status().is_success() => Found {
            resolves: true,
            answered: true,
            note: None,
        },
        Ok(answer) => Found {
            resolves: true,
            answered: false,
            note: Some(format!("it answered {}", answer.status().as_u16())),
        },
        Err(_) => Found {
            resolves: true,
            answered: false,
            note: Some("it did not answer".to_owned()),
        },
    }
}

/// Puts the day's checking in the queue.
pub async fn schedule(state: &AppState) -> Result<Option<usize>> {
    crate::kernel::scheduler::daily::<CheckDomains>(state).await
}

/// Unused by anything but the queue, and named here so the job that runs it can
/// be found from the kind.
pub async fn run(state: &AppState, job: &queue::Job) -> Result<()> {
    let _ = job;
    check_domains(state).await.map(|_| ())
}
