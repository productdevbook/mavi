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
use crate::role::{self, NewRole, Role, RoleChanges};
use crate::ticket::For;
use crate::token::{self, Minted};

pub const THIS_SITE_IS_ALREADY_SET_UP: &str = "this_site_is_already_set_up";
pub const THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD: &str = "that_is_not_an_address_and_a_password";
pub const SOMEBODY_ALREADY_HAS_THAT_ADDRESS: &str = "somebody_already_has_that_address";
pub const THAT_ACCOUNT_IS_STOPPED: &str = "that_account_is_stopped";
pub const THERE_IS_NO_KEY_LIKE_THAT: &str = "there_is_no_key_like_that";
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
/// What a right password got somebody.
///
/// Two answers rather than one, because an account with a second step is not
/// signed in by a password alone — and a function that answered a session
/// either way would be one where forgetting to ask the second question is a
/// silent way past it.
#[derive(Clone, Debug)]
pub enum WayIn {
    /// In. The token signs them in from now on.
    Signed(Person, String),
    /// Half way. What comes back is a moment to finish with, and it is not a
    /// way in to anything.
    NeedsTheSecondStep(String),
}

pub async fn sign_in(tx: &mut Tx, email: &str, said: &str) -> Result<WayIn> {
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

    // Whether there is a second step, asked here rather than by whoever calls
    // this. A caller that has to remember to ask is a caller that one day does
    // not, and the thing they would have forgotten is the whole feature.
    let has_a_second_step: bool = sqlx::query_scalar(
        "select exists (
            select 1 from second_factors
             where person_id = $1 and confirmed_at is not null
         )",
    )
    .bind(person.id)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    if has_a_second_step {
        return Ok(WayIn::NeedsTheSecondStep(
            mint_a_moment(tx, person.id).await?,
        ));
    }

    let minted = begin(tx, person.id).await?;

    Ok(WayIn::Signed(person, minted.token))
}

/// How long somebody has to finish, in minutes.
///
/// Five: long enough to find a phone that has gone flat and is on a charger,
/// short enough that a moment left on a shared machine is not a way in an hour
/// later.
pub const MINUTES_TO_FINISH: i64 = 5;

/// A moment to finish signing in with.
///
/// Minted in the same table as every other link, so it is expired and used
/// once by the same code — a second mechanism for a short-lived token is a
/// second place for "has this been used" to be got wrong.
async fn mint_a_moment(tx: &mut Tx, person: Uuid) -> Result<String> {
    let minted = token::mint();

    sqlx::query(
        "insert into tickets (id, person_id, token, what_for, expires_at)
         values ($1, $2, $3, $4, now() + make_interval(mins => $5))",
    )
    .bind(Uuid::now_v7())
    .bind(person)
    .bind(minted.hash.as_slice())
    .bind(For::AMomentToFinish.as_str())
    .bind(i32::try_from(MINUTES_TO_FINISH).unwrap_or(5))
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(minted.token)
}

/// The session, once the second step has been got past.
pub async fn finish(tx: &mut Tx, person: Uuid) -> Result<(Person, String)> {
    let minted = begin(tx, person).await?;

    Ok((one(tx, person).await?, minted.token))
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
pub async fn redeem(tx: &mut Tx, token: &str, what_for: For, said: Option<&str>) -> Result<Uuid> {
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

    // An exhaustive match rather than a question with two answers. The `else`
    // that used to be here was "prove an address", so a fourth purpose added
    // to the enum would have quietly proved one — which is the exact shape of
    // the hole this file's own documentation is about.
    match what_for {
        For::AnInvitation | For::AForgottenPassword => {
            let said = said
                .ok_or_else(|| Error::invalid(Say::of(password::A_PASSWORD_IS_AT_LEAST_TWELVE)))?;

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
        }
        For::AnAddressToProve => {
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
        // Nothing. Whoever holds it has already given a right password; what
        // redeeming it does is let the second step be asked for, and that is
        // the caller's next move rather than a change to the account.
        For::AMomentToFinish => {}
    }

    Ok(person)
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

/// Every role. A handful, so no paging: what this is for is a screen that has
/// to show them all at once anyway, and a role picker with a cursor in it is a
/// role picker somebody has to page through to find "Editor".
pub async fn roles(tx: &mut Tx) -> Result<Vec<Role>> {
    let rows = sqlx::query(
        "select id, name, grants, is_the_owner, created_at from roles
          order by is_the_owner desc, name",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_role).collect()
}

fn a_role(row: &sqlx::postgres::PgRow) -> Result<Role> {
    Ok(Role {
        id: row.try_get("id").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        grants: row.try_get("grants").map_err(Error::internal)?,
        is_the_owner: row.try_get("is_the_owner").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// One role, or the refusal that says there is none.
pub async fn a_role_called(tx: &mut Tx, id: Uuid) -> Result<Role> {
    let row =
        sqlx::query("select id, name, grants, is_the_owner, created_at from roles where id = $1")
            .bind(id)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

    row.as_ref()
        .map(a_role)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(role::THERE_IS_NO_ROLE_LIKE_THAT)))
}

/// Makes one.
///
/// Never the owner's. That one is made when the site is, exactly once, and a
/// second thing that can do everything is a second thing to have taken.
pub async fn make_a_role(tx: &mut Tx, new: &NewRole) -> Result<Role> {
    let name = role::a_name(&new.name)?;
    let grants = role::grants(&new.grants)?;

    let row = sqlx::query(
        "insert into roles (id, name, grants) values ($1, $2, $3)
         returning id, name, grants, is_the_owner, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(&name)
    .bind(&grants)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| match &cause {
        sqlx::Error::Database(db) if db.constraint() == Some("roles_name") => {
            Error::conflict(Say::of("something_else_is_called_that"))
        }
        _ => Error::internal(cause),
    })?;

    a_role(&row)
}

/// Changes one.
///
/// The owner's may be renamed and its grants may not be touched: it holds
/// everything by being what it is, and a set of grants written onto it would
/// be a second answer to what it can do — one that could be made smaller.
pub async fn change_a_role(tx: &mut Tx, id: Uuid, changes: &RoleChanges) -> Result<Role> {
    let now = a_role_called(tx, id).await?;

    if now.is_the_owner && changes.grants.is_some() {
        return Err(Error::conflict(Say::of(
            role::THE_OWNERS_ROLE_HOLDS_EVERYTHING,
        )));
    }

    let name = match &changes.name {
        Some(said) => role::a_name(said)?,
        None => now.name.clone(),
    };

    let grants = match &changes.grants {
        Some(asked) => role::grants(asked)?,
        None => now.grants.clone(),
    };

    let row = sqlx::query(
        "update roles set name = $2, grants = $3, updated_at = now() where id = $1
         returning id, name, grants, is_the_owner, created_at",
    )
    .bind(id)
    .bind(&name)
    .bind(&grants)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    a_role(&row)
}

/// Takes one away, unless somebody holds it.
///
/// Refused rather than cascaded. An account whose role has gone is an account
/// that either can do nothing or can do everything depending on how the join
/// is written, and neither is something to discover afterwards.
pub async fn remove_a_role(tx: &mut Tx, id: Uuid) -> Result<()> {
    let role = a_role_called(tx, id).await?;

    if role.is_the_owner {
        return Err(Error::conflict(
            Say::of(owner::SOMEBODY_HAS_TO_BE_ABLE_TO_GET_IN)
                .with("doing", &owner::Doing::TakingTheRole.as_str()),
        ));
    }

    let holding: i64 =
        sqlx::query_scalar("select count(*) from people where role_id = $1 and deleted_at is null")
            .bind(id)
            .fetch_one(tx.conn())
            .await
            .map_err(Error::internal)?;

    if holding > 0 {
        return Err(Error::conflict(
            Say::of(role::SOMEBODY_STILL_HOLDS_THAT_ROLE).with("how_many", &holding),
        ));
    }

    sqlx::query("delete from roles where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// Moves somebody to another role.
///
/// The last owner who can get in cannot be moved off it, for the same reason
/// they cannot be removed: a site nobody can sign into is not a site anybody
/// can fix from the outside.
pub async fn move_them(tx: &mut Tx, id: Uuid, to: Uuid) -> Result<Person> {
    let moving_the_owner: bool = sqlx::query_scalar(
        "select r.is_the_owner from people p join roles r on r.id = p.role_id
          where p.id = $1 and p.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(NOBODY_HERE_HAS_THAT_ADDRESS)))?;

    let to_the_owners = a_role_called(tx, to).await?.is_the_owner;

    if moving_the_owner && !to_the_owners {
        owner::may(
            owner::Doing::MovingThem,
            other_owners_who_can_get_in(tx, id).await?,
        )?;
    }

    sqlx::query("update people set role_id = $2, updated_at = now() where id = $1")
        .bind(id)
        .bind(to)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    one(tx, id).await
}

/// One person, or the refusal that says nobody is there.
pub async fn one(tx: &mut Tx, id: Uuid) -> Result<Person> {
    let row = sqlx::query(&format!(
        "select {COLUMNS} from people p join roles r on r.id = p.role_id
          where p.id = $1 and p.deleted_at is null"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_person)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOBODY_HERE_HAS_THAT_ADDRESS)))
}

/// One key, as somebody managing them sees it.
///
/// Never the key itself. It is handed over once, when it is made, and after
/// that this is all there is — which is what makes losing one mean making a
/// new one rather than looking the old one up.
#[derive(Clone, Debug, Serialize)]
pub struct Key {
    pub id: Uuid,
    pub name: String,
    pub grants: Vec<String>,
    /// Null until it has been used once. What tells somebody which key nobody
    /// uses any more, which is the one worth revoking.
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// What making one asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewKey {
    pub name: String,
    /// A narrowing of what the account may do. Left out means everything the
    /// account can — a key made without thinking about it is exactly its
    /// account, which is what somebody expects.
    #[serde(default)]
    pub grants: Vec<String>,
}

/// The key, once, and the row from now on.
#[derive(Clone, Debug, Serialize)]
pub struct Made {
    pub key: Key,
    /// Handed over here and kept nowhere. What is stored is its hash.
    pub token: String,
}

fn a_key(row: &PgRow) -> Result<Key> {
    Ok(Key {
        id: row.try_get("id").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        grants: row.try_get("grants").map_err(Error::internal)?,
        last_seen_at: row.try_get("last_seen_at").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// The keys one account has, still working.
pub async fn keys(tx: &mut Tx, person: Uuid) -> Result<Vec<Key>> {
    let rows = sqlx::query(
        "select id, name, grants, last_seen_at, created_at from keys
          where person_id = $1 and ended_at is null order by created_at desc",
    )
    .bind(person)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_key).collect()
}

/// Makes one.
///
/// Its grants are checked the same way a role's are, and against the same
/// list — a key asking for something that is not a capability is the same
/// mistake as a role doing it, and it is refused in the same words.
pub async fn make_a_key(tx: &mut Tx, person: Uuid, new: &NewKey) -> Result<Made> {
    let name = role::a_name(&new.name)?;
    let grants = role::grants(&new.grants)?;
    let minted = token::mint();

    let row = sqlx::query(
        "insert into keys (id, person_id, name, token, grants) values ($1, $2, $3, $4, $5)
         returning id, name, grants, last_seen_at, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(person)
    .bind(&name)
    .bind(minted.hash.as_slice())
    .bind(&grants)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Made {
        key: a_key(&row)?,
        token: minted.token,
    })
}

/// Stops one working.
///
/// Ended rather than deleted, like a session: "when did this stop working" is
/// a question somebody asks after the fact, and a row that is gone answers
/// nothing.
pub async fn end_a_key(tx: &mut Tx, person: Uuid, id: Uuid) -> Result<()> {
    let ended = sqlx::query(
        "update keys set ended_at = now()
          where id = $1 and person_id = $2 and ended_at is null",
    )
    .bind(id)
    .bind(person)
    // Held against whose it is, not only against its id. A key somebody else
    // made is not something to end by guessing at an id.
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if ended.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_KEY_LIKE_THAT)));
    }

    Ok(())
}

/// Whoever holds this key, and what it may do.
///
/// **What a key may do is worked out here rather than copied when it was
/// made.** A key's own grants narrow its account's; the account's come from
/// its role as they are now. So a role that loses something loses it for every
/// key made against it, in the same moment, without anything having to go and
/// find them.
///
/// The empty narrowing means the account's own, which is what a key made
/// without thinking about it should be.
pub async fn whoever_holds_a_key(tx: &mut Tx, token: &str) -> Result<Option<Person>> {
    let row = sqlx::query(&format!(
        "select {COLUMNS}, k.id as key, k.grants as narrowed from keys k
           join people p on p.id = k.person_id
           join roles r on r.id = p.role_id
          where k.token = $1
            and k.ended_at is null
            and p.deleted_at is null
            and p.standing = 'here'"
    ))
    .bind(token::hash(token).as_slice())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let key: Uuid = row.try_get("key").map_err(Error::internal)?;
    let narrowed: Vec<String> = row.try_get("narrowed").map_err(Error::internal)?;

    let mut person = a_person(&row)?;

    if !narrowed.is_empty() {
        person.grants.retain(|held| narrowed.contains(held));
    }

    // What tells somebody which key nobody uses any more. Written every time
    // rather than once a day: this is one row by primary key, and a key nobody
    // can tell the age of is a key nobody revokes.
    sqlx::query("update keys set last_seen_at = now() where id = $1")
        .bind(key)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(Some(person))
}
