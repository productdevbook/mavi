//! What a role is, and the rules about changing one.
//!
//! A role is a name and a set of grants, and an account holds exactly one.
//! That is the whole of the permission system — which is why the absence of
//! any way to list, make or change one was not a missing screen but a missing
//! half: `people.invite` takes a role's id and **nothing could tell anybody
//! one**, so an installation had the role it was set up with and no other, and
//! every capability but the owner's was a switch nothing could turn on.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const A_NAME_IS_BETWEEN_ONE_AND_A_HUNDRED: &str = "a_name_is_between_one_and_a_hundred";
pub const THAT_IS_NOT_SOMETHING_A_ROLE_CAN_HOLD: &str = "that_is_not_something_a_role_can_hold";
pub const THE_OWNERS_ROLE_HOLDS_EVERYTHING: &str = "the_owners_role_holds_everything";
pub const SOMEBODY_STILL_HOLDS_THAT_ROLE: &str = "somebody_still_holds_that_role";
pub const THERE_IS_NO_ROLE_LIKE_THAT: &str = "there_is_no_role_like_that";

/// One role.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    /// What it holds, as `content:write` and the like.
    pub grants: Vec<String>,
    /// The one that can do everything, including the things nothing else may.
    /// Exactly one exists, and it is not made or removed here.
    pub is_the_owner: bool,
    pub created_at: DateTime<Utc>,
}

/// What making one asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewRole {
    pub name: String,
    #[serde(default)]
    pub grants: Vec<String>,
}

/// What changing one asks for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RoleChanges {
    pub name: Option<String>,
    /// The whole set, replaced. A role's grants are one thing rather than a
    /// collection to add to: what somebody is editing is which switches are
    /// on, and sending only the ones they turned on would never turn one off.
    pub grants: Option<Vec<String>>,
}

/// What moving somebody asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WhichRole {
    pub role: Uuid,
}

/// A name, checked.
pub fn a_name(said: &str) -> Result<String> {
    let name = said.trim();

    if !(1..=100).contains(&name.chars().count()) {
        return Err(Error::invalid(Say::of(A_NAME_IS_BETWEEN_ONE_AND_A_HUNDRED)));
    }

    Ok(name.to_owned())
}

/// Grants, checked against what a capability actually is.
///
/// Every one, by name, against the one list — because a grant nobody spelled
/// right is a switch in a panel that looks on and does nothing, and the
/// account holding it finds out by being refused something they were told
/// they could do.
///
/// Sorted and deduplicated on the way through, so that two roles holding the
/// same things hold them in the same order and a screen comparing them is
/// comparing what they hold rather than what order somebody typed.
pub fn grants(asked: &[String]) -> Result<Vec<String>> {
    let mut checked = Vec::with_capacity(asked.len());

    for grant in asked {
        let (of, access) = grant.split_once(':').ok_or_else(|| refused_grant(grant))?;

        if !crate::is_a_capability(of) || !["view", "write"].contains(&access) {
            return Err(refused_grant(grant));
        }

        checked.push(grant.clone());
    }

    checked.sort();
    checked.dedup();

    Ok(checked)
}

fn refused_grant(grant: &str) -> Error {
    Error::invalid(Say::of(THAT_IS_NOT_SOMETHING_A_ROLE_CAN_HOLD).with("grant", &grant))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_is_a_capability_and_something_to_do_with_it() {
        assert_eq!(
            grants(&["content:view".to_owned(), "shop:write".to_owned()]).expect("two"),
            vec!["content:view".to_owned(), "shop:write".to_owned()]
        );
    }

    #[test]
    fn a_grant_nobody_spelled_right_is_refused_where_it_is_written() {
        // The one that matters. A misspelled grant kept as it was typed is a
        // switch in a panel that looks on and does nothing, and the account
        // holding it finds out by being refused something they were told they
        // could do.
        for wrong in [
            "content",
            "content:",
            "content:read",
            "contnet:view",
            "everything:write",
            ":view",
            "",
        ] {
            assert!(
                grants(&[wrong.to_owned()]).is_err(),
                "{wrong} was taken for a grant"
            );
        }
    }

    #[test]
    fn what_two_roles_hold_is_comparable() {
        // Sorted and deduplicated, so a screen comparing two roles is
        // comparing what they hold rather than the order somebody typed.
        let one = grants(&[
            "shop:write".to_owned(),
            "content:view".to_owned(),
            "shop:write".to_owned(),
        ])
        .expect("checked");

        let other = grants(&["content:view".to_owned(), "shop:write".to_owned()]).expect("checked");

        assert_eq!(one, other);
    }

    #[test]
    fn a_name_is_a_name() {
        assert_eq!(a_name("  Editor  ").expect("a name"), "Editor");
        assert!(a_name("").is_err());
        assert!(a_name(&"x".repeat(101)).is_err());
    }
}
