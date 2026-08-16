//! Setting a second step up, and getting past it.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::ports::Seals;
use mavi_core::say::Say;
use mavi_db::Tx;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::digits;

pub const THERE_IS_NO_SECOND_STEP_HERE: &str = "there_is_no_second_step_here";
pub const THAT_IS_NOT_THE_RIGHT_CODE: &str = "that_is_not_the_right_code";
pub const THERE_IS_ALREADY_A_SECOND_STEP: &str = "there_is_already_a_second_step";

/// How many ways back in somebody is given.
///
/// Ten, shown once. Enough that losing a few is not losing the account, few
/// enough that the list is one a person actually keeps.
pub const HOW_MANY_WAYS_BACK: usize = 10;

/// What somebody is shown once, when they set one up.
#[derive(Clone, Debug, Serialize)]
pub struct ToSetUp {
    /// What an app reads out of a picture.
    pub what_an_app_reads: String,
    /// The same secret written out, for somebody typing it in by hand.
    pub typed_in: String,
}

/// What is shown once it has been confirmed.
#[derive(Clone, Debug, Serialize)]
pub struct WaysBackIn {
    /// Shown here and never again. What is kept is their hashes.
    pub codes: Vec<String>,
}

/// Whether an account has one, and whether it is confirmed.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Standing {
    pub set_up: bool,
    pub confirmed: bool,
    /// How many ways back in are left. Somebody down to their last one should
    /// be told before the phone goes, not after.
    pub ways_back_in: i64,
}

fn hashed(code: &str) -> Vec<u8> {
    let mut hash = Sha256::new();
    hash.update(code.replace([' ', '-'], "").to_uppercase().as_bytes());

    hash.finalize().to_vec()
}

/// Where an account stands.
pub async fn standing(tx: &mut Tx, person: Uuid) -> Result<Standing> {
    let row: Option<(Option<DateTime<Utc>>,)> =
        sqlx::query_as("select confirmed_at from second_factors where person_id = $1")
            .bind(person)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

    let left: i64 = sqlx::query_scalar(
        "select count(*) from ways_back_in where person_id = $1 and used_at is null",
    )
    .bind(person)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Standing {
        set_up: row.is_some(),
        confirmed: row.is_some_and(|(at,)| at.is_some()),
        ways_back_in: left,
    })
}

/// Starts one.
///
/// Refused where one is already confirmed: replacing a working second step
/// with a new one is the same move as taking it off, and it goes through the
/// same door with the same code asked for.
pub async fn set_up(
    tx: &mut Tx,
    seals: &dyn Seals,
    person: Uuid,
    site: &str,
    account: &str,
) -> Result<ToSetUp> {
    if standing(tx, person).await?.confirmed {
        return Err(Error::conflict(Say::of(THERE_IS_ALREADY_A_SECOND_STEP)));
    }

    let secret = digits::invent();
    let sealed = seals.seal(&secret).await?;

    // Written over whatever was there. An unconfirmed row is somebody who
    // started and stopped, and starting again should not need a step to undo
    // the last attempt.
    sqlx::query(
        "insert into second_factors (person_id, sealed) values ($1, $2)
         on conflict (person_id) do update
            set sealed = excluded.sealed, confirmed_at = null, last_step = null",
    )
    .bind(person)
    .bind(&sealed)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(ToSetUp {
        what_an_app_reads: digits::what_an_app_reads(&secret, site, account),
        typed_in: digits::to_base32(&secret),
    })
}

/// The secret for one account, opened.
async fn secret_for(
    tx: &mut Tx,
    seals: &dyn Seals,
    person: Uuid,
) -> Result<(Vec<u8>, Option<i64>)> {
    let row: Option<(Vec<u8>, Option<i64>)> =
        sqlx::query_as("select sealed, last_step from second_factors where person_id = $1")
            .bind(person)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

    let (sealed, last_step) =
        row.ok_or_else(|| Error::invalid(Say::of(THERE_IS_NO_SECOND_STEP_HERE)))?;

    Ok((seals.open(&sealed).await?, last_step))
}

/// Shows the six digits work, and hands over the ways back in.
///
/// The ways back in are made **here** rather than when it was set up, so that
/// somebody who scanned a picture and never confirmed is not carrying ten
/// codes that would get them past a step they do not have.
pub async fn confirm(
    tx: &mut Tx,
    seals: &dyn Seals,
    person: Uuid,
    code: &str,
    now: DateTime<Utc>,
) -> Result<WaysBackIn> {
    let (secret, last_step) = secret_for(tx, seals, person).await?;

    let Some(step) = digits::check(&secret, code, now, last_step) else {
        return Err(Error::invalid(Say::of(THAT_IS_NOT_THE_RIGHT_CODE)));
    };

    sqlx::query(
        "update second_factors set confirmed_at = coalesce(confirmed_at, now()), last_step = $2
          where person_id = $1",
    )
    .bind(person)
    .bind(step)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    // Whatever was there goes. Confirming again is a new set of codes, and the
    // old ones stop working the moment the new ones are shown.
    sqlx::query("delete from ways_back_in where person_id = $1")
        .bind(person)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    let mut codes = Vec::with_capacity(HOW_MANY_WAYS_BACK);

    for _ in 0..HOW_MANY_WAYS_BACK {
        let code = a_way_back();

        sqlx::query("insert into ways_back_in (person_id, code) values ($1, $2)")
            .bind(person)
            .bind(hashed(&code))
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

        codes.push(code);
    }

    Ok(WaysBackIn { codes })
}

/// One way back in: ten characters somebody can read off paper.
///
/// No letters a handwritten note confuses — no `O`, `I`, `L`, `U` against `0`,
/// `1` and `V` — because the moment these are read is the moment somebody is
/// already locked out and typing from a piece of paper.
fn a_way_back() -> String {
    use rand::Rng;

    const READABLE: &[u8] = b"ABCDEFGHJKMNPQRSTWXYZ23456789";

    let mut rng = rand::rng();
    let mut code = String::with_capacity(11);

    for at in 0..10 {
        if at == 5 {
            code.push('-');
        }

        code.push(char::from(READABLE[rng.random_range(0..READABLE.len())]));
    }

    code
}

/// Whether these six digits get past the step.
///
/// Writes down the step it took, in the same statement that checks nothing
/// else took it — so two sign-ins racing with one code is one of them getting
/// in.
pub async fn gets_past(
    tx: &mut Tx,
    seals: &dyn Seals,
    person: Uuid,
    code: &str,
    now: DateTime<Utc>,
) -> Result<bool> {
    let (secret, last_step) = secret_for(tx, seals, person).await?;

    let Some(step) = digits::check(&secret, code, now, last_step) else {
        return used_a_way_back(tx, person, code).await;
    };

    let took = sqlx::query(
        "update second_factors set last_step = $2
          where person_id = $1 and (last_step is null or last_step < $2)",
    )
    .bind(person)
    .bind(step)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(took.rows_affected() > 0)
}

/// One of the codes from the piece of paper.
///
/// Marked used in the statement that finds it, so the same code sent twice is
/// one way in rather than two.
async fn used_a_way_back(tx: &mut Tx, person: Uuid, code: &str) -> Result<bool> {
    let used = sqlx::query(
        "update ways_back_in set used_at = now()
          where person_id = $1 and code = $2 and used_at is null",
    )
    .bind(person)
    .bind(hashed(code))
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(used.rows_affected() > 0)
}

/// Takes it off.
///
/// The ways back in go with it: a code that gets somebody past a step that no
/// longer exists is a piece of paper that is still a way into the account.
pub async fn take_it_off(tx: &mut Tx, person: Uuid) -> Result<()> {
    sqlx::query("delete from second_factors where person_id = $1")
        .bind(person)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    sqlx::query("delete from ways_back_in where person_id = $1")
        .bind(person)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_way_back_is_readable_off_paper() {
        let code = a_way_back();

        assert_eq!(code.len(), 11);
        assert_eq!(&code[5..6], "-");

        // The moment these are read is the moment somebody is locked out and
        // typing from a piece of paper, so nothing a handwritten note
        // confuses.
        for wrong in ['O', 'I', 'L', 'U', '0', '1'] {
            assert!(!code.contains(wrong), "{code} has a {wrong} in it");
        }
    }

    #[test]
    fn a_code_is_read_the_way_somebody_types_it() {
        // Off paper, with the dash, in whatever case they hit.
        assert_eq!(hashed("abcde-fghjk"), hashed("ABCDEFGHJK"));
        assert_eq!(hashed(" ABCDE FGHJK "), hashed("ABCDEFGHJK"));
    }
}
