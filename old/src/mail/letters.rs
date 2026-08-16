//! What a site's own letters say.
//!
//! The invitation, the password link, the receipt. Written by this machine
//! until a site says otherwise, and in the site's own words when it does — a
//! receipt that says somebody else's name in a language the reader does not
//! have is a receipt that looks like a mistake.

use axum::Json;
use axum::extract::{Path, State as Injected};
use serde::{Deserialize, Serialize};

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::say::{self, Say};

fn mail(access: Access) -> Needs {
    Needs::new(Capability::Mail, access)
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/mail/letters",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Vec<Letter>>(),
        Endpoint::put(
            "/api/mail/letters/{kind}",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            write,
        )
        .takes::<Wording>()
        .gives::<Letter>(),
        Endpoint::delete(
            "/api/mail/letters/{kind}",
            Guard {
                audience: Audience::User,
                needs: Some(mail(Access::Write)),
                rate: RatePolicy::None,
            },
            forget,
        ),
    ]
}

/// Every letter this machine sends one person, and what it says when nobody has
/// said otherwise. A kind not here cannot be written, because nothing sends it.
pub const KINDS: &[Kind] = &[
    Kind {
        name: "invitation",
        subject: "You have been invited",
        body: "Hello {name},\n\nSomebody has invited you to {site}. Choose a \
               password here within three days:\n\n{link}\n",
        names: &["name", "site", "link"],
    },
    Kind {
        name: "password",
        subject: "A new password",
        body: "Hello {name},\n\nFollow the link below to choose a new password. \
               If you did not ask for this, you can ignore this letter.\n\n{link}\n",
        names: &["name", "site", "link"],
    },
    Kind {
        name: "email_proof",
        subject: "Confirm your new address",
        body: "Hello {name},\n\nConfirm this is your address for {site} by \
               following the link below and choosing a password again, \
               within three days:\n\n{link}\n",
        names: &["name", "site", "link"],
    },
    Kind {
        name: "order.paid",
        subject: "Your order",
        body: "Hello {name},\n\nThank you — your order is paid for. You can see \
               it here:\n\n{link}\n\n{site}\n",
        names: &["name", "site", "link", "total"],
    },
    Kind {
        name: "order.fulfilled",
        subject: "Your order is on its way",
        body: "Hello {name},\n\nYour order is on its way.\n\n{link}\n\n{site}\n",
        names: &["name", "site", "link"],
    },
    Kind {
        name: "student.invited",
        subject: "Your course",
        body: "Hello {name},\n\nYou have been put on a course at {site}. Sign in \
               here:\n\n{link}\n",
        names: &["name", "site", "link"],
    },
];

/// A kind [`KINDS`] carries, together with why nothing presses it. A kind can
/// be written and worded through `/api/mail/letters/{kind}` for no reason at
/// all otherwise — `tests/letters.rs` reads this crate's own source for every
/// call to [`press`] and holds it against `KINDS`, and a name here is what
/// lets one through that check rather than failing it.
pub const NEVER_PRESSED: &[(&str, &str)] = &[];

/// One letter's kind: its name, what it says by default, and what it may name.
#[derive(Clone, Copy, Debug)]
pub struct Kind {
    pub name: &'static str,
    pub subject: &'static str,
    pub body: &'static str,
    /// What this letter can put in. A name not here is a hole in somebody's
    /// receipt, so writing one is refused rather than left to print `{total}`.
    pub names: &'static [&'static str],
}

#[must_use]
pub fn kind(name: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|kind| kind.name == name)
}

/// One letter as a screen sees it: what it says, and whether that is the site's
/// own wording or this machine's.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Letter {
    pub kind: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    /// False where this is what the machine says because the site has not said
    /// anything.
    pub theirs: bool,
    pub names: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Wording {
    pub language: String,
    pub subject: String,
    pub body: String,
}

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<Letter>>> {
    let mut conn = state.db.begin().await?;

    let theirs: Vec<(String, String, String, String)> =
        sqlx::query_as("select kind, language, subject, body from letters order by kind, language")
            .fetch_all(conn.conn())
            .await?;

    conn.commit().await?;

    let mut all = Vec::with_capacity(KINDS.len() + theirs.len());

    for kind in KINDS {
        let written: Vec<&(String, String, String, String)> = theirs
            .iter()
            .filter(|(name, ..)| name == kind.name)
            .collect();

        if written.is_empty() {
            all.push(told(kind, "", kind.subject, kind.body, false));
            continue;
        }

        for (_, language, subject, body) in written {
            all.push(told(kind, language, subject, body, true));
        }
    }

    Ok(Json(all))
}

fn told(kind: &Kind, language: &str, subject: &str, body: &str, theirs: bool) -> Letter {
    Letter {
        kind: kind.name.to_owned(),
        language: language.to_owned(),
        subject: subject.to_owned(),
        body: body.to_owned(),
        theirs,
        names: kind.names.iter().map(|name| (*name).to_owned()).collect(),
    }
}

async fn write(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(name): Path<String>,
    Json(body): Json<Wording>,
) -> Result<Audited<Json<Letter>>> {
    let kind = kind(&name).ok_or(AppError::NotFound("letter"))?;

    // A name nothing fills in is a hole in somebody's receipt, and the moment
    // to say so is now rather than when the letter goes out.
    if let Some(unknown) = holes(&body.subject, &body.body, kind.names).first() {
        return Err(AppError::Invalid(
            Say::of(say::THAT_LETTER_CANNOT_NAME_THAT).naming("name", unknown),
        ));
    }

    let mut conn = state.db.begin().await?;

    sqlx::query(
        "insert into letters (kind, language, subject, body)
         values ($1, $2, $3, $4)
         on conflict (kind, language)
           do update set subject = excluded.subject, body = excluded.body",
    )
    .bind(kind.name)
    .bind(&body.language)
    .bind(&body.subject)
    .bind(&body.body)
    .execute(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "wrote a letter",
        "letter",
        Some(kind.name),
        &serde_json::json!({ "language": body.language }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(told(kind, &body.language, &body.subject, &body.body, true)),
    ))
}

/// Back to what this machine says. Not a deletion of anything anybody reads:
/// the letter still goes out, in the words it had before the site changed them.
async fn forget(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(name): Path<String>,
) -> Result<Audited<axum::http::StatusCode>> {
    let kind = kind(&name).ok_or(AppError::NotFound("letter"))?;

    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from letters where kind = $1")
        .bind(kind.name)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("letter"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "went back to the machine's own wording",
        "letter",
        Some(kind.name),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, axum::http::StatusCode::NO_CONTENT))
}

/// Which names a wording asks for that the letter cannot fill in.
#[must_use]
pub fn holes(subject: &str, body: &str, names: &[&str]) -> Vec<String> {
    let mut asked = Vec::new();

    for text in [subject, body] {
        let mut left = text;

        while let Some(open) = left.find('{') {
            let after = &left[open + 1..];

            let Some(close) = after.find('}') else {
                break;
            };

            let name = &after[..close];

            if !names.contains(&name) && !asked.iter().any(|seen| seen == name) {
                asked.push(name.to_owned());
            }

            left = &after[close + 1..];
        }
    }

    asked
}

/// What to send, in the site's words where it has any and this machine's
/// otherwise, in the reader's language where the site wrote one.
pub async fn press(
    conn: &mut Tx,
    name: &str,
    language: &str,
    values: &[(&str, String)],
) -> Result<(String, String)> {
    let kind = kind(name).ok_or(AppError::Bug("a letter nothing sends"))?;

    let theirs: Option<(String, String)> = sqlx::query_as(
        "select subject, body from letters
          where kind = $1 and language = $2
          -- The site's own language where the reader's is not written: better
          -- their words in the wrong language than a stranger's in the right
          -- one.
          union all
         select subject, body from letters where kind = $1
          limit 1",
    )
    .bind(kind.name)
    .bind(language)
    .fetch_optional(conn.conn())
    .await?;

    let (subject, body) = theirs.unwrap_or_else(|| (kind.subject.to_owned(), kind.body.to_owned()));

    Ok((fill(&subject, values), fill(&body, values)))
}

/// Puts the values in. A name nothing was given for is left as it is rather
/// than blanked: an empty line in a receipt says less than an obvious hole.
#[must_use]
pub fn fill(text: &str, values: &[(&str, String)]) -> String {
    let mut out = text.to_owned();

    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_letter_can_fill_in_what_it_says() {
        for kind in KINDS {
            let missing = holes(kind.subject, kind.body, kind.names);

            assert!(
                missing.is_empty(),
                "{} says {missing:?} and cannot fill them in",
                kind.name
            );
        }
    }

    #[test]
    fn a_wording_that_asks_for_something_nothing_fills_in_is_caught() {
        let kind = kind("invitation").expect("the invitation");

        assert_eq!(
            holes("Hello", "Your order {total} is ready", kind.names),
            vec!["total".to_owned()]
        );

        assert!(holes("Hello {name}", "Sign in: {link}", kind.names).is_empty());
    }

    #[test]
    fn what_is_given_goes_in_and_what_is_not_stays_visible() {
        let said = fill(
            "Hello {name}, see {link}",
            &[("name", "Somebody".to_owned())],
        );

        assert_eq!(said, "Hello Somebody, see {link}");
    }
}
