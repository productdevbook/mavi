//! What is kept instead of a password.
//!
//! Argon2id, with a salt per password, and what is stored is the whole encoded
//! string — the algorithm, its parameters, the salt and the hash — so that a
//! password hashed by a version of this that ran two years ago can still be
//! checked by the one running now.
//!
//! What is refused is a password too short to be worth hashing. There is no
//! rule here about capital letters or punctuation: those rules produce
//! `Password1!` and nothing else, and length is the only one that measurably
//! helps.

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const A_PASSWORD_IS_AT_LEAST_TWELVE: &str = "a_password_is_at_least_twelve";
pub const A_PASSWORD_IS_AT_MOST_A_HUNDRED_AND_TWENTY_EIGHT: &str =
    "a_password_is_at_most_a_hundred_and_twenty_eight";

/// The shortest a password may be.
///
/// Twelve rather than eight: eight is a number from when the guess was made
/// against a login form rather than against a stolen table.
pub const AT_LEAST: usize = 12;

/// The longest. A limit exists because hashing is deliberately slow, and a
/// megabyte of "password" is a way to make one request cost a whole core.
pub const AT_MOST: usize = 128;

/// What to store for this password.
pub fn kept(password: &str) -> Result<String> {
    let length = password.chars().count();

    if length < AT_LEAST {
        return Err(Error::invalid(
            Say::of(A_PASSWORD_IS_AT_LEAST_TWELVE).with("at_least", &AT_LEAST),
        ));
    }

    if length > AT_MOST {
        return Err(Error::invalid(Say::of(
            A_PASSWORD_IS_AT_MOST_A_HUNDRED_AND_TWENTY_EIGHT,
        )));
    }

    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hashed| hashed.to_string())
        .map_err(|_| Error::internal(std::io::Error::other("a password could not be hashed")))
}

/// Whether this is the password that was kept.
///
/// A row with no password — somebody invited who has not taken it up — answers
/// no, and takes about as long as answering no to a wrong one would. Answering
/// instantly is how somebody learns which addresses have accounts.
#[must_use]
pub fn is_theirs(password: &str, kept: Option<&str>) -> bool {
    let Some(kept) = kept else {
        // Hash something anyway, so that "no account" and "wrong password"
        // take the same time. The value is thrown away.
        let _ =
            Argon2::default().hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng));

        return false;
    };

    let Ok(parsed) = PasswordHash::new(kept) else {
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_kept_is_not_the_password() {
        let kept = kept("a long enough password").expect("a hash");

        assert!(!kept.contains("a long enough password"));
        assert!(kept.starts_with("$argon2"));
    }

    #[test]
    fn the_same_password_twice_is_kept_two_different_ways() {
        // A salt per password. Without one, two people who chose the same
        // password have the same row, and a stolen table sorts itself into
        // "who to try first".
        let one = kept("a long enough password").expect("a hash");
        let other = kept("a long enough password").expect("a hash");

        assert_ne!(one, other);
    }

    #[test]
    fn a_password_is_theirs_or_it_is_not() {
        let kept = kept("a long enough password").expect("a hash");

        assert!(is_theirs("a long enough password", Some(&kept)));
        assert!(!is_theirs("a long enough passworD", Some(&kept)));
        assert!(!is_theirs("", Some(&kept)));
    }

    #[test]
    fn somebody_with_no_password_yet_is_not_signed_in_by_guessing() {
        // An account invited and never taken up has no password. Answering
        // "no" is right; answering it instantly is how somebody learns which
        // addresses have accounts.
        assert!(!is_theirs("anything at all", None));
    }

    #[test]
    fn a_password_too_short_to_be_worth_hashing_is_refused() {
        assert_eq!(
            kept("short")
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            A_PASSWORD_IS_AT_LEAST_TWELVE
        );
    }

    #[test]
    fn a_megabyte_is_not_a_password() {
        // Hashing is deliberately slow, so an unbounded one is a way to make
        // a single request cost a core.
        let vast = "x".repeat(AT_MOST + 1);

        assert_eq!(
            kept(&vast)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            A_PASSWORD_IS_AT_MOST_A_HUNDRED_AND_TWENTY_EIGHT
        );
    }
}
