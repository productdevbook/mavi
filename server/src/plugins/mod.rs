//! What a site plugs into: its own mail server, its own payment provider.
//!
//! Which integrations exist is the list below rather than whatever somebody
//! writes into a table. A machine-wide setting is what a site gets when it has
//! configured nothing, so a site that says nothing keeps working.

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::mailer::Mailer;
use crate::kernel::payments::Payments;
use crate::kernel::say::{self, Say};
use crate::kernel::secret::Secret;
use crate::kernel::tenant::TenantId;
use crate::kernel::{crypto, mailer, payments};

/// One integration a site may configure, and what it is made of.
#[derive(Clone, Copy, Debug)]
pub struct Known {
    pub key: &'static str,
    /// What must be given for it to be worth switching on.
    pub wants: &'static [&'static str],
    /// Which of those are secrets: sealed, and never read back out.
    pub keeps: &'static [&'static str],
}

/// Everything a site can plug into. Adding to this list is a change to this
/// file, which is the point.
pub const KNOWN: &[Known] = &[
    Known {
        // A site's own mail server, so that what it sends comes from it rather
        // than from whoever runs the machine.
        key: "mail",
        wants: &["url", "from"],
        keeps: &["url"],
    },
    Known {
        // A site's own payment provider. Card details never reach this
        // machine, here or anywhere: what is stored is how to ask.
        key: "payments",
        wants: &["name", "at", "key", "signing"],
        keeps: &["key", "signing"],
    },
];

#[must_use]
pub fn known(key: &str) -> Option<&'static Known> {
    KNOWN.iter().find(|plugin| plugin.key == key)
}

fn settings(access: Access) -> Needs {
    Needs::new(Capability::Settings, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/plugins",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Vec<Plugged>>(),
        Endpoint::put(
            "/api/plugins/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::Write)),
                rate: RatePolicy::None,
            },
            configure,
        )
        .takes::<Plugging>()
        .gives::<Plugged>(),
        Endpoint::delete(
            "/api/plugins/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::Delete)),
                rate: RatePolicy::None,
            },
            unplug,
        ),
        Endpoint::post(
            "/api/plugins/{key}/check",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::Write)),
                rate: RatePolicy::None,
            },
            check,
        )
        .gives::<Plugged>(),
    ]
}

/// One integration, as a screen sees it. The secret half is not here and there
/// is no endpoint that answers with it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Plugged {
    pub key: String,
    pub configured: bool,
    pub enabled: bool,
    /// What was given, minus everything the plugin keeps.
    pub settings: Value,
    /// Which secrets are set, without saying what they are.
    pub holds: Vec<String>,
    pub working: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Plugging {
    /// Everything the plugin wants, secrets included. What is not given again
    /// is kept as it was, so a screen does not have to re-type a password to
    /// change a port.
    pub settings: Value,
    #[serde(default = "yes")]
    pub enabled: bool,
}

const fn yes() -> bool {
    true
}

async fn list(State(state): State<AppState>, caller: Caller) -> Result<Json<Vec<Plugged>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let stored = all(&mut conn).await?;
    conn.commit().await?;

    Ok(Json(
        KNOWN
            .iter()
            .map(|plugin| {
                stored
                    .iter()
                    .find(|(key, _)| key == plugin.key)
                    .map_or_else(
                        || nothing_plugged(plugin),
                        |(_, row)| told(plugin, row.clone()),
                    )
            })
            .collect(),
    ))
}

async fn configure(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
    Json(body): Json<Plugging>,
) -> Result<Audited<Json<Plugged>>> {
    let plugin = known(&key).ok_or(AppError::NotFound("plugin"))?;

    let Value::Object(given) = body.settings else {
        return Err(AppError::Invalid(say::SETTINGS_NAMED_VALUES.into()));
    };

    let mut conn = state.db.tenant(caller.tenant()).await?;

    // What was there before, so that changing one field does not mean sending
    // every secret again.
    let before = one(&mut conn, &key).await?;
    let mut open = match &before {
        Some(row) => opened(&state, row)?,
        None => serde_json::Map::new(),
    };

    for (name, value) in given {
        if !plugin.wants.contains(&name.as_str()) {
            return Err(AppError::Invalid(
                Say::of(say::PLUGIN_TAKES_NO_SUCH_SETTING)
                    .naming("plugin", &key)
                    .naming("setting", &name),
            ));
        }

        open.insert(name, value);
    }

    for wanted in plugin.wants {
        let missing = open
            .get(*wanted)
            .is_none_or(|value| value.as_str().is_some_and(str::is_empty));

        if missing {
            return Err(AppError::Invalid(
                Say::of(say::PLUGIN_WANTS_SETTING)
                    .naming("plugin", &key)
                    .naming("setting", wanted),
            ));
        }
    }

    let (plain, secret) = split(plugin, &open);
    let sealed = crypto::seal(&state.keyring, &secret.to_string())?;

    let row: Row = sqlx::query_as(
        "insert into plugins (tenant_id, key, enabled, settings, sealed)
         values ($1, $2, $3, $4, $5)
         on conflict (tenant_id, key) do update set
            enabled = excluded.enabled,
            settings = excluded.settings,
            sealed = excluded.sealed,
            working = null,
            note = null,
            checked_at = null
         returning key, enabled, settings, sealed, working, note",
    )
    .bind(caller.tenant().0)
    .bind(&key)
    .bind(body.enabled)
    .bind(Value::Object(plain))
    .bind(&sealed)
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "configured an integration",
        "plugin",
        Some(&key),
        &serde_json::json!({ "enabled": body.enabled }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(told(plugin, row))))
}

async fn unplug(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
) -> Result<Audited<axum::http::StatusCode>> {
    known(&key).ok_or(AppError::NotFound("plugin"))?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone = sqlx::query("delete from plugins where key = $1")
        .bind(&key)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("plugin"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "unplugged an integration",
        "plugin",
        Some(&key),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, axum::http::StatusCode::NO_CONTENT))
}

/// Whether it works, asked now rather than found out from a customer.
async fn check(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
) -> Result<Audited<Json<Plugged>>> {
    let plugin = known(&key).ok_or(AppError::NotFound("plugin"))?;

    let mut conn = state.db.tenant(caller.tenant()).await?;
    let row = one(&mut conn, &key)
        .await?
        .ok_or(AppError::NotFound("plugin"))?;
    let open = opened(&state, &row)?;

    let (working, note) = match key.as_str() {
        "mail" => match mail_from(&open) {
            Ok(mailer) => match mailer.reachable().await {
                Ok(()) => (true, None),
                Err(why) => (false, Some(why.to_string())),
            },
            Err(why) => (false, Some(why.to_string())),
        },
        // Nothing is asked of a payment provider here: the question a site
        // wants answered is whether a payment goes through, and inventing one
        // to find out is a payment somebody has to refund.
        _ => (true, None),
    };

    let row: Row = sqlx::query_as(
        "update plugins set working = $2, note = $3, checked_at = now()
          where key = $1
         returning key, enabled, settings, sealed, working, note",
    )
    .bind(&key)
    .bind(working)
    .bind(note.as_deref())
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "checked an integration",
        "plugin",
        Some(&key),
        &serde_json::json!({ "working": working }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(told(plugin, row))))
}

/// key, enabled, settings, sealed, working, note
type Row = (
    String,
    bool,
    Value,
    Option<String>,
    Option<bool>,
    Option<String>,
);

async fn all(conn: &mut TenantConn) -> Result<Vec<(String, Row)>> {
    let rows: Vec<Row> =
        sqlx::query_as("select key, enabled, settings, sealed, working, note from plugins")
            .fetch_all(conn.conn())
            .await?;

    Ok(rows.into_iter().map(|row| (row.0.clone(), row)).collect())
}

async fn one(conn: &mut TenantConn, key: &str) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "select key, enabled, settings, sealed, working, note from plugins where key = $1",
    )
    .bind(key)
    .fetch_optional(conn.conn())
    .await?)
}

fn nothing_plugged(plugin: &Known) -> Plugged {
    Plugged {
        key: plugin.key.to_owned(),
        configured: false,
        enabled: false,
        settings: serde_json::json!({}),
        holds: Vec::new(),
        working: None,
        note: None,
    }
}

fn told(plugin: &Known, row: Row) -> Plugged {
    Plugged {
        key: plugin.key.to_owned(),
        configured: true,
        enabled: row.1,
        settings: row.2,
        holds: plugin.keeps.iter().map(|name| (*name).to_owned()).collect(),
        working: row.4,
        note: row.5,
    }
}

/// The two halves: what a screen may read back, and what is sealed.
fn split(
    plugin: &Known,
    open: &serde_json::Map<String, Value>,
) -> (serde_json::Map<String, Value>, Value) {
    let mut plain = serde_json::Map::new();
    let mut secret = serde_json::Map::new();

    for (name, value) in open {
        if plugin.keeps.contains(&name.as_str()) {
            secret.insert(name.clone(), value.clone());
        } else {
            plain.insert(name.clone(), value.clone());
        }
    }

    (plain, Value::Object(secret))
}

/// Both halves, put back together, for the code that has to use them.
fn opened(state: &AppState, row: &Row) -> Result<serde_json::Map<String, Value>> {
    let mut open = match &row.2 {
        Value::Object(plain) => plain.clone(),
        _ => serde_json::Map::new(),
    };

    if let Some(sealed) = &row.3 {
        let secret = crypto::open(&state.keyring, sealed)?;
        let parsed: Value = serde_json::from_str(secret.expose())
            .map_err(|_| AppError::Bug("a sealed setting that is not what was sealed"))?;

        if let Value::Object(secret) = parsed {
            open.extend(secret);
        }
    }

    Ok(open)
}

fn text(open: &serde_json::Map<String, Value>, name: &str) -> Result<String> {
    open.get(name)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or(AppError::Bug("an integration missing something it wants"))
}

fn mail_from(open: &serde_json::Map<String, Value>) -> Result<Mailer> {
    Ok(Mailer::Smtp(mailer::Smtp::new(
        Secret::new(text(open, "url")?),
        text(open, "from")?,
    )))
}

fn payments_from(open: &serde_json::Map<String, Value>) -> Result<Payments> {
    Ok(Payments::Hosted(payments::Hosted {
        name: text(open, "name")?,
        at: text(open, "at")?,
        key: Secret::new(text(open, "key")?),
        signing: Secret::new(text(open, "signing")?),
    }))
}

/// Where this site's mail goes.
///
/// Its own server where it has one, and the machine's where it has not — a
/// site that has configured nothing keeps working rather than quietly stopping.
pub async fn mailer_for(state: &AppState, tenant: TenantId) -> Result<Mailer> {
    let mut conn = state.db.tenant(tenant).await?;
    let found = enabled(&mut conn, "mail").await?;
    conn.commit().await?;

    match found {
        Some(row) => mail_from(&opened(state, &row)?),
        None => Ok(Mailer::clone(&state.mailer)),
    }
}

/// Who takes this site's money. The same rule.
pub async fn payments_for(state: &AppState, tenant: TenantId) -> Result<Payments> {
    let mut conn = state.db.tenant(tenant).await?;
    let found = enabled(&mut conn, "payments").await?;
    conn.commit().await?;

    match found {
        Some(row) => payments_from(&opened(state, &row)?),
        None => Ok(Payments::clone(&state.payments)),
    }
}

async fn enabled(conn: &mut TenantConn, key: &str) -> Result<Option<Row>> {
    Ok(sqlx::query_as(
        "select key, enabled, settings, sealed, working, note
           from plugins where key = $1 and enabled",
    )
    .bind(key)
    .fetch_optional(conn.conn())
    .await?)
}

/// A plugin nobody declared cannot be configured, so nothing else has to check.
#[must_use]
pub fn keys() -> Vec<String> {
    KNOWN.iter().map(|plugin| plugin.key.to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_kept_is_kept_out_of_what_is_read_back() {
        for plugin in KNOWN {
            for kept in plugin.keeps {
                assert!(
                    plugin.wants.contains(kept),
                    "{} keeps {kept}, which it never asked for",
                    plugin.key
                );
            }
        }
    }

    #[test]
    fn the_two_halves_are_the_whole() {
        let plugin = known("mail").expect("mail is a plugin");
        let mut open = serde_json::Map::new();
        open.insert("url".to_owned(), Value::String("smtp://…".to_owned()));
        open.insert(
            "from".to_owned(),
            Value::String("post@example.test".to_owned()),
        );

        let (plain, secret) = split(plugin, &open);

        assert!(
            plain.get("url").is_none(),
            "a password was left in the open"
        );
        assert!(plain.contains_key("from"));
        assert!(secret.get("url").is_some());
    }
}
