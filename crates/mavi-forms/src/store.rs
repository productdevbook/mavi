//! Reading and writing forms, and what people sent them.
//!
//! The public half is what this file is careful about. Everything a visitor
//! reaches is here: a form drawn from its own row, and a submission checked
//! against what that row declared before a single byte of it is written.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use mavi_db::{Tx, Walk};
use serde::Deserialize;
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::field::{Declared, Field};
use crate::filled::Filled;
use crate::{BY_RECENT, Form, KEPT_FOR_DAYS, OpenForm, Sent};

pub const NOTHING_IS_ASKED_AT_THAT_ADDRESS: &str = "nothing_is_asked_at_that_address";
pub const SOMETHING_ELSE_ASKS_AT_THAT_ADDRESS: &str = "something_else_asks_at_that_address";

const COLUMNS: &str = "id, slug, name, fields, open, kept_days, created_at, updated_at";

fn a_form(row: &PgRow) -> Result<Form> {
    let slug: String = row.try_get("slug").map_err(Error::internal)?;
    let fields: serde_json::Value = row.try_get("fields").map_err(Error::internal)?;

    // Checked on the way out as well as on the way in. A row written by an
    // older version of this code, or by a hand at two in the morning, is still
    // held to what a form may be — and a form that cannot be checked is one
    // nothing should be answering with.
    let fields: Vec<Field> = serde_json::from_value(fields).map_err(Error::internal)?;

    Ok(Form {
        id: crate::FormId(row.try_get("id").map_err(Error::internal)?),
        slug: Slug::parse(&slug)?,
        name: row.try_get("name").map_err(Error::internal)?,
        fields: Declared::checked(fields)?,
        open: row.try_get("open").map_err(Error::internal)?,
        kept_days: row.try_get("kept_days").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
        updated_at: row.try_get("updated_at").map_err(Error::internal)?,
    })
}

/// What this site asks people.
pub async fn list(tx: &mut Tx, query: &Query) -> Result<Page<Form>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];

    let cursor = walk.after(1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select {COLUMNS} from forms where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(a_form)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |form| {
        vec![form.created_at.to_rfc3339(), form.id.to_string()]
    })
}

/// What making one asks for.
#[derive(Clone, Debug, Deserialize)]
pub struct NewForm {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    pub kept_days: Option<i32>,
}

/// Makes one, having checked everything about it that can be checked now.
pub async fn make(tx: &mut Tx, new: &NewForm) -> Result<Form> {
    let slug = Slug::parse(&new.slug)?;
    let declared = Declared::checked(new.fields.clone())?;

    let row = sqlx::query(&format!(
        "insert into forms (id, slug, name, fields, kept_days)
         values ($1, $2, $3, $4, coalesce($5, $6))
         returning {COLUMNS}"
    ))
    .bind(Uuid::now_v7())
    .bind(slug.as_str())
    .bind(new.name.trim())
    .bind(serde_json::to_value(declared.fields()).map_err(Error::internal)?)
    .bind(new.kept_days)
    .bind(KEPT_FOR_DAYS)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| taken(&cause))?;

    a_form(&row)
}

fn taken(cause: &sqlx::Error) -> Error {
    match cause {
        sqlx::Error::Database(db) if db.constraint() == Some("forms_address") => {
            Error::conflict(Say::of(SOMETHING_ELSE_ASKS_AT_THAT_ADDRESS))
        }
        other => Error::internal(std::io::Error::other(other.to_string())),
    }
}

/// What may be changed about one.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct FormChanges {
    pub name: Option<String>,
    pub fields: Option<Vec<Field>>,
    pub open: Option<bool>,
    pub kept_days: Option<i32>,
}

/// Changes one.
pub async fn change(tx: &mut Tx, id: Uuid, changes: &FormChanges) -> Result<Form> {
    let fields = match &changes.fields {
        Some(fields) => Some(
            serde_json::to_value(Declared::checked(fields.clone())?.fields())
                .map_err(Error::internal)?,
        ),
        None => None,
    };

    let row = sqlx::query(&format!(
        "update forms
            set name = coalesce($2, name),
                fields = coalesce($3, fields),
                open = coalesce($4, open),
                kept_days = coalesce($5, kept_days),
                updated_at = now()
          where id = $1 and deleted_at is null
         returning {COLUMNS}"
    ))
    .bind(id)
    .bind(changes.name.as_deref())
    .bind(fields)
    .bind(changes.open)
    .bind(changes.kept_days)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_form)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_ASKED_AT_THAT_ADDRESS)))
}

/// Removes one, and what people sent it with it.
pub async fn remove(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query("delete from forms where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(NOTHING_IS_ASKED_AT_THAT_ADDRESS)));
    }

    Ok(())
}

/// What a page needs to draw an open form.
///
/// Read by its address and only when it is open. A form that has been closed
/// answers the same as one that was never made, so this is not a way to ask
/// what forms a site has.
pub async fn open_form(tx: &mut Tx, slug: &str) -> Result<(Uuid, OpenForm, Declared)> {
    let slug = Slug::parse(slug)?;

    let row = sqlx::query(&format!(
        "select {COLUMNS} from forms where slug = $1 and open and deleted_at is null"
    ))
    .bind(slug.as_str())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_ASKED_AT_THAT_ADDRESS)))?;

    let form = a_form(&row)?;

    Ok((
        Uuid::from(form.id),
        OpenForm {
            slug: form.slug.clone(),
            name: form.name.clone(),
            fields: form.fields.clone(),
        },
        form.fields,
    ))
}

/// Takes what somebody sent, having held it against what the form asked for.
///
/// Checked before anything is written, so a submission that does not fit is a
/// refusal rather than a row somebody has to look at later.
pub async fn fill_in(
    tx: &mut Tx,
    slug: &str,
    filled: &Filled,
    from_where: Option<&str>,
) -> Result<Uuid> {
    let (form, _, declared) = open_form(tx, slug).await?;

    filled.fits(&declared)?;

    let id = Uuid::now_v7();

    sqlx::query(
        "insert into filled (id, form_id, answers, from_where)
         values ($1, $2, $3, $4::text::inet)",
    )
    .bind(id)
    .bind(form)
    .bind(serde_json::Value::Object(filled.answers.clone()))
    .bind(from_where)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(id)
}

/// What people sent one form.
pub async fn what_came_in(
    tx: &mut Tx,
    form: Uuid,
    unseen: bool,
    query: &Query,
) -> Result<Page<Sent>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["form_id = $1".to_owned(), "deleted_at is null".to_owned()];

    if unseen {
        wheres.push("seen_at is null".to_owned());
    }

    let cursor = walk.after(2);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select id, form_id, answers, seen_at, created_at from filled
          where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql).bind(form);

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows: Vec<Sent> = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(|row| {
            // `jsonb` comes back as a `Value`, and what was written there is
            // always an object — the column says so with a check constraint.
            // Taken apart here rather than trusted: a row that is somehow not
            // an object is a row this refuses to answer with.
            let answers: serde_json::Value = row.try_get("answers").map_err(Error::internal)?;
            let answers = answers.as_object().cloned().ok_or_else(|| {
                Error::internal(std::io::Error::other("answers that are not an object"))
            })?;

            Ok(Sent {
                id: crate::FilledId(row.try_get("id").map_err(Error::internal)?),
                form_id: crate::FormId(row.try_get("form_id").map_err(Error::internal)?),
                answers,
                seen_at: row.try_get("seen_at").map_err(Error::internal)?,
                created_at: row.try_get("created_at").map_err(Error::internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |sent| {
        vec![sent.created_at.to_rfc3339(), sent.id.to_string()]
    })
}

/// Says everything sent to this form up to now has been read.
///
/// Up to now rather than everything: something that arrives while somebody is
/// reading the screen has not been read, and marking it so is how a message
/// goes unanswered.
pub async fn all_seen(tx: &mut Tx, form: Uuid, up_to: DateTime<Utc>) -> Result<u64> {
    let seen = sqlx::query(
        "update filled set seen_at = now()
          where form_id = $1 and seen_at is null and created_at <= $2 and deleted_at is null",
    )
    .bind(form)
    .bind(up_to)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(seen.rows_affected())
}

/// Forgets one thing somebody sent.
pub async fn forget(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone =
        sqlx::query("update filled set deleted_at = now() where id = $1 and deleted_at is null")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(NOTHING_IS_ASKED_AT_THAT_ADDRESS)));
    }

    Ok(())
}
