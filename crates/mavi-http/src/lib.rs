//! What lets a request in, and what it has to leave behind.
//!
//! Two rules, both of which were held in the crate this replaces and both of
//! which had a hole in them.
//!
//! **Nothing is reached without being admitted.** An endpoint says who may
//! reach it and what they must hold; this asks, and a refusal is an error
//! rather than a `false` somebody can forget to read.
//!
//! **Nothing that changes anything answers without a receipt.** There, that
//! rule was enforced on one of two admission paths — the console's had no gate
//! at all (#16) — and it decided what a change was by reading the **HTTP
//! verb**, which is wrong for any endpoint carrying a protocol underneath it.
//! A single `POST` answering `tools/list` was recorded as a change to the site
//! (#54).
//!
//! Here there is one admission path, and what counts as a change is what the
//! endpoint said.

pub mod admit;

use mavi_core::grant::Grants;

pub use admit::{Admitted, admit};

/// Who is asking.
///
/// `Nobody` is a real answer rather than an absence: a public endpoint is
/// reached by somebody, and giving them no name is different from failing to
/// look one up.
#[derive(Clone, Debug)]
pub enum Caller {
    Nobody,
    /// Somebody with an account here, and what they hold.
    AnAccount {
        id: String,
        grants: Grants,
    },
    /// Somebody enrolled on a course. Not an account: a student holds no
    /// grants and reaches only what is theirs.
    AStudent {
        id: String,
    },
}

impl Caller {
    /// Who this is, where that is a thing at all. What an `:own` grant is
    /// compared against.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Caller::Nobody => None,
            Caller::AnAccount { id, .. } | Caller::AStudent { id } => Some(id),
        }
    }

    #[must_use]
    pub fn grants(&self) -> Grants {
        match self {
            Caller::AnAccount { grants, .. } => grants.clone(),
            _ => Grants::default(),
        }
    }
}

/// Proof that a change was written down before it answered.
///
/// Held rather than checked: a handler that changes something returns one of
/// these, and one cannot be made without writing the row. The alternative —
/// a rule everybody remembers — is the version that had a hole in it for as
/// long as there were two ways in.
#[derive(Debug)]
pub struct Receipt {
    /// The audit row this change wrote.
    pub wrote: uuid::Uuid,
}

impl Receipt {
    /// Only callable by whatever writes the row, which is the point.
    #[must_use]
    pub const fn of(wrote: uuid::Uuid) -> Self {
        Self { wrote }
    }
}

/// What a handler answers with.
///
/// An endpoint that changes something answers `Changed`, which carries the
/// receipt. One that does not answers `Read`. The two are different types, so
/// "a change that left no record" is not a thing anybody can write.
#[derive(Debug)]
pub enum Answered<T> {
    Read(T),
    Changed(T, Receipt),
}

impl<T> Answered<T> {
    #[must_use]
    pub const fn wrote(&self) -> Option<&Receipt> {
        match self {
            Answered::Read(_) => None,
            Answered::Changed(_, receipt) => Some(receipt),
        }
    }

    #[must_use]
    pub fn into_inner(self) -> T {
        match self {
            Answered::Read(what) | Answered::Changed(what, _) => what,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_student_holds_nothing() {
        let student = Caller::AStudent {
            id: "a-student".to_owned(),
        };

        assert_eq!(student.grants().all().count(), 0);
        assert_eq!(student.id(), Some("a-student"));
    }

    #[test]
    fn nobody_is_somebody_with_no_name() {
        // Not a null check waiting to be forgotten: a public endpoint is
        // reached by `Nobody`, which is a value, so nothing has to remember
        // that the absence of a caller is allowed there.
        assert!(Caller::Nobody.id().is_none());
        assert_eq!(Caller::Nobody.grants().all().count(), 0);
    }

    #[test]
    fn a_change_carries_its_receipt_in_its_type() {
        let read: Answered<u8> = Answered::Read(1);
        let changed = Answered::Changed(1, Receipt::of(uuid::Uuid::now_v7()));

        assert!(read.wrote().is_none());
        assert!(changed.wrote().is_some());
    }
}
