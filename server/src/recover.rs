//! The way back in when the owner has locked themselves out.
//!
//! There used to be a second account — an operator, made at setup with the
//! same address and the same password as the owner — and it looked like a back
//! door. It was not: nothing in this crate has ever inserted into
//! `operator_sessions`, so that account could not sign in, and a door that
//! cannot open is not a door. Taking it away removed nothing that worked, but
//! it left nothing in its place either, and this is the replacement.
//!
//! On the host rather than over HTTP, because that is the honest shape of it.
//! Whoever can run this command can already read the database and the keys; a
//! second web account that could reset a password would be a second thing to
//! phish, on every installation, for the sake of the day somebody forgets.

use std::env;

use crate::kernel::db::Db;
use crate::kernel::password;

/// Sets one account's password, reading the new one from standard input.
///
/// From stdin rather than from an argument: an argument is in `ps`, in the
/// shell's history, and in whatever the process manager logs.
pub async fn reset_password(email: &str) -> Result<(), Box<dyn std::error::Error>> {
    let typed = std::io::read_to_string(std::io::stdin())?;
    let typed = typed.trim();

    if typed.chars().count() < 12 {
        return Err("a password is at least twelve characters".into());
    }

    let db = Db::connect(&env::var("DATABASE_URL")?, 1).await?;
    let hash = password::hash(typed)?;

    let mut tx = db.begin().await?;

    // Cleared along with the password: somebody locked out of their own
    // account is usually locked out of the authenticator that went with it,
    // and a reset that leaves the second factor in place is a reset that does
    // not let anybody back in. Said out loud in the answer, because it is a
    // protection being removed rather than a detail.
    let changed = sqlx::query(
        "update users
            set password_hash = $2,
                state = 'active'
          where email = $1
            and deleted_at is null",
    )
    .bind(email)
    .bind(&hash)
    .execute(tx.conn())
    .await?
    .rows_affected();

    if changed == 0 {
        return Err("no account here has that address".into());
    }

    sqlx::query(
        "delete from second_factors
          where user_id in (select id from users where email = $1)",
    )
    .bind(email)
    .execute(tx.conn())
    .await?;

    // Every session that account had. Whoever needed this may be locking
    // somebody else out, and leaving the old sessions open would make the
    // reset pointless in exactly that case.
    sqlx::query(
        "update sessions
            set revoked_at = now()
          where revoked_at is null
            and user_id in (select id from users where email = $1)",
    )
    .bind(email)
    .execute(tx.conn())
    .await?;

    tx.commit().await?;

    println!("password set, second factor cleared, and every session ended");

    Ok(())
}
