//! Reading and writing who can sign in.
//!
//! Three things here are worth reading before the code: setting a site up
//! happens once and in one transaction; signing in answers the same way for an
//! address that has no account and one whose password is wrong; and a ticket
//! is redeemed by asking, in the `where` clause, what it was minted for.

use chrono::{DateTime, Duration, Utc};
use mavi_core::email::Email;
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::owner::{self, Doing};
use crate::password;
use crate::ticket::For;
use crate::token::{self, Minted};

pub const THIS_SITE_IS_ALREADY_SET_UP: &str = "this_site_is_already_set_up";
pub const THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD: &str = "that_is_not_an_address_and_a_password";
pub const SOMEBODY_ALREADY_HAS_THAT_ADDRESS: &str = "somebody_already_has_that_address";
pub const THAT_ACCOUNT_IS_STOPPED: &str = "that_account_is_stopped";
pub const NOBODY_HERE_HAS_THAT_ADDRESS: &str = "nobody_here_has_that_address";

/// How long a session is good for without being used again.
pub const A_SESSION_LASTS_DAYS: i64 = 30;

/// How long a link somebody was sent is good for.
pub const A_LINK_LASTS_DAYS: i64 = 3;

/// The panel's order over people.
pub const BY_RECENT: mavi_core::page::Keyset = mavi_core::page::Keyset(&[
    mavi_core::page::Key::newest("created_at", mavi_core::page::Kind::Moment),
    mavi_core::page::Key::newest("id", mavi_core::page::Kind::Id),
]);

/// Somebody with an account here.
#[derive(Clone, Debug, Serialize)]
pub struct Person {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Uuid,
    pub standing: String,
    pub proved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// What their role holds. Read so the guard has it, and not sent back:
    /// what a role grants is the role's to answer, and repeating it on every
    /// person is two answers to one question.
    #[serde(skip)]
    pub grants: Vec<String>,
}

const COLUMNS: &str = "p.id, p.email, p.name, p.role_id, p.standing, p.proved_at, p.created_at, \
                       r.grants";

fn a_person(row: &PgRow) -> Result<Person> {
    Ok(Person {
        id: row.try_get("id").map_err(Error::internal)?,
        email: row.try_get("email").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        role: row.try_get("role_id").map_err(Error::internal)?,
        standing: row.try_get("standing").map_err(Error::internal)?,
        proved_at: row.try_get("proved_at").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
        grants: row.try_get("grants").map_err(Error::internal)?,
    })
}

/// What setting a site up asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Setup {
    pub site: String,
    pub name: String,
    pub email: String,
    pub password: String,
}

/// What it answers.
#[derive(Clone, Debug, Serialize)]
pub struct Ready {
    pub person: Person,
    /// The token that signs them in. Handed over once, here, because the
    /// alternative is telling somebody to sign in with the password they typed
    /// ten seconds ago and hoping nothing went wrong in between.
    pub token: String,
}

/// The one moment that makes the site, the owner's role, and the account that
/// holds it.
///
/// One transaction, and it answers once. Everything about it is decided by the
/// database rather than by asking first: the settings table takes one row and
/// refuses a second, so two people setting a site up at the same moment is one
/// site and one refusal rather than a race nobody sees.
pub async fn set_up(tx: &mut Tx, asked: &Setup) -> Result<Ready> {
    let email = Email::parse(&asked.email)?;
    let kept = password::kept(&asked.password)?;

    let name = asked.name.trim();
    let site = asked.site.trim();

    if name.is_empty() || site.is_empty() {
        return Err(Error::invalid(Say::of(
            THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD,
        )));
    }

    sqlx::query("insert into settings (name) values ($1)")
        .bind(site)
        .execute(tx.conn())
        .await
        .map_err(|_| Error::conflict(Say::of(THIS_SITE_IS_ALREADY_SET_UP)))?;

    // The site writes in something from the first moment. A site with no
    // language is a site whose first post cannot be filed anywhere.
    sqlx::query(
        "insert into languages (tag, name, is_the_sites_own) values ('en', 'English', true)",
    )
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    let role = Uuid::now_v7();
    sqlx::query(
        "insert into roles (id, name, grants, is_the_owner) values ($1, 'Owner', $2, true)",
    )
    .bind(role)
    .bind(everything())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    let person = Uuid::now_v7();
    let row = sqlx::query(&format!(
        "with made as (
             insert into people (id, email, name, password, role_id, standing, proved_at)
             values ($1, $2, $3, $4, $5, 'here', now())
             returning *
         )
         select {COLUMNS} from made p join roles r on r.id = p.role_id"
    ))
    .bind(person)
    .bind(email.as_str())
    .bind(name)
    .bind(&kept)
    .bind(role)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let minted = begin(tx, person).await?;

    Ok(Ready {
        person: a_person(&row)?,
        token: minted.token,
    })
}

/// Every capability, held whole. What the owner's role is.
///
/// Both accesses, written out: holding `content:write` is not holding
/// `content:view`, because a grant is compared as it is rather than ranked.
/// The owner holds each of them because the owner is the person who cannot be
/// locked out of anything.
fn everything() -> Vec<String> {
    crate::CAPABILITIES
        .iter()
        .flat_map(|what| [format!("{what}:view"), format!("{what}:write")])
        .collect()
}

/// Signs somebody in.
///
/// An address with no account and an address with the wrong password answer
/// the same way, and take about the same time — the difference between them is
/// a way to ask which addresses have accounts here.
pub async fn sign_in(tx: &mut Tx, email: &str, said: &str) -> Result<(Person, String)> {
    let refuse = || Error::forbidden(Say::of(THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD));

    let folded = Email::parse(email).map_err(|_| refuse())?;

    let row = sqlx::query(&format!(
        "select {COLUMNS}, p.password from people p
           join roles r on r.id = p.role_id
          where p.email = $1 and p.deleted_at is null"
    ))
    .bind(folded.as_str())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    let kept: Option<String> = match &row {
        Some(row) => row.try_get("password").map_err(Error::internal)?,
        None => None,
    };

    if !password::is_theirs(said, kept.as_deref()) {
        return Err(refuse());
    }

    let row = row.ok_or_else(refuse)?;
    let person = a_person(&row)?;

    // Stopped is its own refusal: somebody whose account was stopped by
    // whoever runs the site is not somebody who typed their password wrong,
    // and telling them to try again wastes an afternoon.
    if person.standing == "stopped" {
        return Err(Error::forbidden(Say::of(THAT_ACCOUNT_IS_STOPPED)));
    }

    let minted = begin(tx, person.id).await?;

    Ok((person, minted.token))
}

/// A session, written down.
async fn begin(tx: &mut Tx, person: Uuid) -> Result<Minted> {
    let minted = token::mint();

    sqlx::query(
        "insert into sessions (id, person_id, token, expires_at)
         values ($1, $2, $3, now() + make_interval(days => $4))",
    )
    .bind(Uuid::now_v7())
    .bind(person)
    .bind(minted.hash.as_slice())
    .bind(i32::try_from(A_SESSION_LASTS_DAYS).unwrap_or(30))
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(minted)
}

/// Who is holding this token, if anybody still is.
///
/// Asked of every request, so it is one query against one index: the hash, and
/// whether the session is still good. It also says when it was last used,
/// which is what makes "sign out everywhere" mean something afterwards.
pub async fn whoever_holds(tx: &mut Tx, token: &str) -> Result<Option<(Person, Uuid)>> {
    let row = sqlx::query(&format!(
        "select {COLUMNS}, s.id as session from sessions s
           join people p on p.id = s.person_id
           join roles r on r.id = p.role_id
          where s.token = $1
            and s.ended_at is null
            and s.expires_at > now()
            and p.deleted_at is null
            and p.standing = 'here'"
    ))
    .bind(token::hash(token).as_slice())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.map(|row| {
        let session: Uuid = row.try_get("session").map_err(Error::internal)?;

        Ok((a_person(&row)?, session))
    })
    .transpose()
}

/// Ends one session — the one whoever is asking was recognised by.
///
/// By its id rather than by its token, because the token is not something a
/// handler holds and should not have to: what it has is who is asking and the
/// session they came in on.
///
/// Ended rather than deleted: "when did this stop working" is a question
/// somebody asks after the fact, and a row that is gone answers nothing.
pub async fn sign_out(tx: &mut Tx, session: Uuid) -> Result<()> {
    sqlx::query("update sessions set ended_at = now() where id = $1 and ended_at is null")
        .bind(session)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// Who has an account here.
///
/// The listing reads two tables, so the cursor's plain column names — which is
/// what the keyset says and what the index is built on — are made unambiguous
/// by walking a derived table rather than by prefixing them at each use. One
/// `p.` forgotten in a string is an `ambiguous column` at run time and a
/// listing that works until somebody asks for page two.
pub async fn list(tx: &mut Tx, query: &Query) -> Result<Page<Person>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let cursor = walk.after(1);

    let after = match &cursor {
        Some((sql, _)) => format!("where {sql}"),
        None => String::new(),
    };

    let sql = format!(
        "select * from (
             select {COLUMNS} from people p
               join roles r on r.id = p.role_id
              where p.deleted_at is null
         ) as person {after} order by {} limit {}",
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
        .map(a_person)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |person| {
        vec![person.created_at.to_rfc3339(), person.id.to_string()]
    })
}

/// What inviting somebody asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invitation {
    pub email: String,
    pub name: String,
    pub role: Uuid,
}

/// Invites somebody, and mints the link they are sent.
///
/// The account exists immediately and has no password. That is the difference
/// between an invitation and a promise: whoever invited them can see them in
/// the list, and the link is the only way the account becomes usable.
pub async fn invite(tx: &mut Tx, asked: &Invitation) -> Result<(Person, String)> {
    let email = Email::parse(&asked.email)?;
    let name = asked.name.trim();

    let person = Uuid::now_v7();
    let row = sqlx::query(&format!(
        "with made as (
             insert into people (id, email, name, role_id) values ($1, $2, $3, $4)
             returning *
         )
         select {COLUMNS} from made p join roles r on r.id = p.role_id"
    ))
    .bind(person)
    .bind(email.as_str())
    .bind(name)
    .bind(asked.role)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| taken(&cause))?;

    let minted = mint_a_ticket(tx, person, For::AnInvitation, None).await?;

    Ok((a_person(&row)?, minted.token))
}

fn taken(cause: &sqlx::Error) -> Error {
    match cause {
        sqlx::Error::Database(db) if db.constraint() == Some("people_address") => {
            Error::conflict(Say::of(SOMEBODY_ALREADY_HAS_THAT_ADDRESS))
        }
        other => Error::internal(std::io::Error::other(other.to_string())),
    }
}

/// A link, minted for one thing.
pub async fn mint_a_ticket(
    tx: &mut Tx,
    person: Uuid,
    what_for: For,
    becomes: Option<&str>,
) -> Result<Minted> {
    let minted = token::mint();

    sqlx::query(
        "insert into tickets (id, person_id, token, what_for, becomes, expires_at)
         values ($1, $2, $3, $4, $5, now() + make_interval(days => $6))",
    )
    .bind(Uuid::now_v7())
    .bind(person)
    .bind(minted.hash.as_slice())
    .bind(what_for.as_str())
    .bind(becomes)
    .bind(i32::try_from(A_LINK_LASTS_DAYS).unwrap_or(3))
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(minted)
}

/// Redeems a link, for the one thing it was minted for.
///
/// The purpose is in the `where` clause. A ticket minted to prove an address is
/// simply not found by the query that sets a password — which is the whole of
/// the fix, because a branch in Rust after the row is read closes the hole for
/// today and leaves it open for whoever adds a fourth purpose.
pub async fn redeem(tx: &mut Tx, token: &str, what_for: For, said: Option<&str>) -> Result<()> {
    let row = sqlx::query(
        "update tickets
            set used_at = now()
          where token = $1
            and what_for = $2
            and used_at is null
            and expires_at > now()
         returning person_id, becomes",
    )
    .bind(token::hash(token).as_slice())
    .bind(what_for.as_str())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    let row = row.ok_or_else(crate::ticket::no_good)?;
    let person: Uuid = row.try_get("person_id").map_err(Error::internal)?;
    let becomes: Option<String> = row.try_get("becomes").map_err(Error::internal)?;

    if what_for.sets_a_password() {
        let said =
            said.ok_or_else(|| Error::invalid(Say::of(password::A_PASSWORD_IS_AT_LEAST_TWELVE)))?;

        let kept = password::kept(said)?;

        sqlx::query(
            "update people
                set password = $2, standing = 'here', proved_at = coalesce(proved_at, now()),
                    updated_at = now()
              where id = $1",
        )
        .bind(person)
        .bind(&kept)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

        // A password changing ends every session. Somebody who has just had to
        // choose a new one is somebody who may have had their old one taken.
        sqlx::query(
            "update sessions set ended_at = now() where person_id = $1 and ended_at is null",
        )
        .bind(person)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    } else {
        // Proving an address proves the address. It does not set a password,
        // does not change the account's standing, and does not end anybody's
        // sessions — ending them for this is a way to sign somebody out of
        // their own account by editing their address.
        sqlx::query(
            "update people
                set email = coalesce($2, email), proved_at = now(), updated_at = now()
              where id = $1",
        )
        .bind(person)
        .bind(becomes)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    Ok(())
}

/// How many **other** people hold the owner's role and can actually sign in.
///
/// The question the last-owner rule is asked about. Somebody invited and never
/// taken up cannot get in, so they do not count — counting them is how a site
/// ends up locked behind an owner who has never had a password.
pub async fn other_owners_who_can_get_in(tx: &mut Tx, besides: Uuid) -> Result<usize> {
    let how_many: i64 = sqlx::query_scalar(
        "select count(*) from people p
           join roles r on r.id = p.role_id
          where r.is_the_owner
            and p.id <> $1
            and p.deleted_at is null
            and p.standing = 'here'
            and p.password is not null",
    )
    .bind(besides)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(usize::try_from(how_many).unwrap_or(0))
}

/// Takes somebody's account away, unless they are the last way in.
pub async fn remove(tx: &mut Tx, id: Uuid) -> Result<()> {
    let holds_it: bool = sqlx::query_scalar(
        "select r.is_the_owner from people p join roles r on r.id = p.role_id
          where p.id = $1 and p.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(NOBODY_HERE_HAS_THAT_ADDRESS)))?;

    if holds_it {
        owner::may(Doing::Removing, other_owners_who_can_get_in(tx, id).await?)?;
    }

    sqlx::query("update people set deleted_at = now() where id = $1 and deleted_at is null")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    // Their sessions go with them. An account that is gone and a token that
    // still works is the same thing as an account that is not gone.
    sqlx::query("update sessions set ended_at = now() where person_id = $1 and ended_at is null")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// How long a link is good for, as something a letter can say.
#[must_use]
pub fn a_link_lasts() -> Duration {
    Duration::days(A_LINK_LASTS_DAYS)
}
