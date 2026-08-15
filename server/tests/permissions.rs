//! The matrix the engine actually answers, written down. A policy change shows
//! up here as a diff in a pull request, which is the only way "I opened that to
//! everybody by accident" gets noticed before it ships.

use std::collections::HashSet;
use std::fmt::Write as _;

use mavi::kernel::authz::{Access, Capability, Needs, Principal, check};
use uuid::Uuid;

const SNAPSHOT: &str = "tests/snapshots/permission-matrix.txt";

/// The engine used to be asked which site a person was reaching, and which of
/// four kinds of person they were. Both questions are gone: there is one site
/// and one kind of person, and "holds the grant" is a `HashSet::contains` that
/// writing down eighty-four times proves nothing about.
///
/// What the engine still decides that a `contains` cannot is the `:own`
/// qualifier: a grant ending in `:own` reaches what this person made and
/// nothing else, and whether it reaches anything at all depends on an owner
/// the handler passes in. The three ways that goes — theirs, somebody else's,
/// and none named — are what this matrix is for now. The last of those is the
/// one worth having: with no owner passed, the policy compares the person's id
/// against an empty string, and that it comes out `deny` is a fact about the
/// default rather than something anybody chose.
fn matrix() -> String {
    let person = Uuid::from_u128(3);
    let somebody_else = Uuid::from_u128(4);

    let mut out = String::from(
        "principal                          capability access  owner        answer\n\
         ---------------------------------- ---------- ------- ------------ ------\n",
    );

    for capability in Capability::ALL {
        for access in [Access::View, Access::Write, Access::Delete] {
            let needs = Needs::new(capability, access);
            let holds_it: HashSet<String> = [needs.grant()].into_iter().collect();
            let holds_own: HashSet<String> = [needs.own_grant()].into_iter().collect();

            let cases: [(&str, Principal, Option<Uuid>, &str); 4] = [
                (
                    "holds the grant",
                    Principal {
                        id: person,
                        grants: holds_it.clone(),
                    },
                    None,
                    "none",
                ),
                (
                    "holds only their own, theirs",
                    Principal {
                        id: person,
                        grants: holds_own.clone(),
                    },
                    Some(person),
                    "them",
                ),
                (
                    "holds only their own, another's",
                    Principal {
                        id: person,
                        grants: holds_own.clone(),
                    },
                    Some(somebody_else),
                    "somebody",
                ),
                (
                    "holds only their own, none named",
                    Principal {
                        id: person,
                        grants: holds_own.clone(),
                    },
                    None,
                    "none",
                ),
            ];

            for (who, principal, owner, said) in cases {
                let answer = if check(&principal, needs, owner).is_ok() {
                    "allow"
                } else {
                    "deny"
                };

                let _ = writeln!(
                    out,
                    "{who:<34} {:<10} {:<7} {said:<12} {answer}",
                    capability.as_str(),
                    access.as_str(),
                );
            }
        }
    }

    out
}

#[test]
fn the_matrix_is_what_it_was() {
    let now = matrix();

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all("tests/snapshots").expect("a place to write");
        std::fs::write(SNAPSHOT, &now).expect("write");
        return;
    }

    let before = std::fs::read_to_string(SNAPSHOT).unwrap_or_else(|_| {
        panic!("{SNAPSHOT} is missing; run the tests with UPDATE_SNAPSHOTS=1 to write it")
    });

    assert_eq!(
        before, now,
        "the answers the engine gives have changed. Read the difference, and if \
         it is what was meant, run with UPDATE_SNAPSHOTS=1."
    );
}
