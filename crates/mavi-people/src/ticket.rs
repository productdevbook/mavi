//! A link somebody was sent, and the one thing it is good for.
//!
//! A ticket is minted **for** something: an invitation, a forgotten password,
//! an address to be proved. Redeeming it asks what it was for, in the `where`
//! clause, so a ticket of the wrong purpose is simply not found.
//!
//! That is not a stylistic preference. In the crate this replaces, minting was
//! careful with the purpose and redemption ignored it entirely — one query,
//! no `purpose` clause — so all three opened the door that sets a password.
//! An account holding `people:write` could change an owner's address to one it
//! controlled, receive the proof link, set the owner's password, and sign in as
//! them. Every session was revoked in the process, including the owner's.
//!
//! Branching in Rust after the row is read would close that hole today and
//! leave it open for whoever adds a fourth purpose. The clause does not.

use std::fmt;

use mavi_core::error::Error;
use mavi_core::say::Say;

pub const THAT_LINK_HAS_BEEN_USED_OR_RUN_OUT: &str = "that_link_has_been_used_or_run_out";

/// What a ticket is good for. Exactly one thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum For {
    /// Somebody new, who has no password yet.
    AnInvitation,
    /// Somebody who has one and cannot remember it.
    AForgottenPassword,
    /// An address that has to prove it is reachable. **Proves the address and
    /// nothing else** — not the password, not the account's state, and it does
    /// not end anybody's sessions. Proving an address is not a credential
    /// change, and ending sessions for one is a way to sign somebody out of
    /// their own account by editing their address.
    AnAddressToProve,
}

impl For {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            For::AnInvitation => "invitation",
            For::AForgottenPassword => "forgotten_password",
            For::AnAddressToProve => "address_to_prove",
        }
    }

    /// Whether redeeming this sets a password.
    ///
    /// Written as a question about the purpose rather than as a branch at the
    /// place it matters, so that adding a fourth purpose is a compile error
    /// here rather than a silent default somewhere else.
    #[must_use]
    pub const fn sets_a_password(self) -> bool {
        match self {
            For::AnInvitation | For::AForgottenPassword => true,
            For::AnAddressToProve => false,
        }
    }
}

impl fmt::Display for For {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One ticket, as it is read back.
#[derive(Clone, Debug)]
pub struct Ticket {
    pub id: uuid::Uuid,
    pub person: uuid::Uuid,
    pub what_for: For,
}

/// What to say when a link is spent, expired, or was never one of ours.
///
/// One refusal for all three on purpose: telling somebody which of the three it
/// was tells them whether a token exists.
#[must_use]
pub fn no_good() -> Error {
    Error::invalid(Say::of(THAT_LINK_HAS_BEEN_USED_OR_RUN_OUT))
}

/// The `where` that finds a ticket of exactly one purpose, and nothing else.
///
/// Given as a fragment rather than a whole query so that a caller cannot
/// accidentally read a ticket without it: there is no function here that finds
/// a ticket by its hash alone.
#[must_use]
pub fn only(what_for: For) -> String {
    format!(
        "token_hash = $1 and what_for = '{}' and spent_at is null and expires_at > now()",
        what_for.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_purpose_is_in_the_clause_rather_than_after_it() {
        // The shape of the fix, asserted as a shape: what the query looks for
        // includes what the ticket was for, so one of the wrong purpose is not
        // found rather than found and then rejected.
        let sql = only(For::AnAddressToProve);

        assert!(sql.contains("what_for = 'address_to_prove'"), "{sql}");
        assert!(sql.contains("spent_at is null"), "{sql}");
        assert!(sql.contains("expires_at > now()"), "{sql}");
    }

    #[test]
    fn proving_an_address_does_not_set_a_password() {
        assert!(!For::AnAddressToProve.sets_a_password());
        assert!(For::AnInvitation.sets_a_password());
        assert!(For::AForgottenPassword.sets_a_password());
    }

    #[test]
    fn every_purpose_is_a_different_clause() {
        let all = [
            For::AnInvitation,
            For::AForgottenPassword,
            For::AnAddressToProve,
        ];

        let mut clauses: Vec<String> = all.iter().map(|what| only(*what)).collect();
        let count = clauses.len();
        clauses.sort();
        clauses.dedup();

        assert_eq!(
            clauses.len(),
            count,
            "two purposes look for the same ticket"
        );
    }

    #[test]
    fn one_refusal_for_spent_expired_and_never_ours() {
        // Three different situations, one answer. Distinguishing them is
        // answering whether a token exists.
        assert_eq!(
            no_good().said().expect("a refusal").key,
            THAT_LINK_HAS_BEEN_USED_OR_RUN_OUT
        );
    }
}
