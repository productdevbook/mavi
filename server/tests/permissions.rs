//! The matrix the engine actually answers, written down. A policy change shows
//! up here as a diff in a pull request, which is the only way "I opened that to
//! everybody by accident" gets noticed before it ships.

use std::collections::HashSet;
use std::fmt::Write as _;

use mavi::kernel::authz::{Access, Capability, Needs, Principal, Resource, check};
use uuid::Uuid;

const SNAPSHOT: &str = "tests/snapshots/permission-matrix.txt";

fn matrix() -> String {
    let site = Uuid::from_u128(1);
    let elsewhere = Uuid::from_u128(2);
    let person = Uuid::from_u128(3);

    let mut out = String::from(
        "principal                 capability access  answer\n\
         ------------------------- ---------- ------- ------\n",
    );

    for capability in Capability::ALL {
        for access in [Access::View, Access::Write, Access::Delete] {
            let needs = Needs::new(capability, access);
            let holding_it: HashSet<String> = [needs.grant()].into_iter().collect();

            let cases: [(&str, Principal, Resource); 4] = [
                (
                    "site user, holds it",
                    Principal::SiteUser {
                        id: person,
                        site,
                        grants: holding_it.clone(),
                    },
                    Resource::Site { id: site },
                ),
                (
                    "site user, holds nothing",
                    Principal::SiteUser {
                        id: person,
                        site,
                        grants: HashSet::new(),
                    },
                    Resource::Site { id: site },
                ),
                (
                    "site user, another site",
                    Principal::SiteUser {
                        id: person,
                        site,
                        grants: holding_it.clone(),
                    },
                    Resource::Site { id: elsewhere },
                ),
                (
                    "operator",
                    Principal::Operator { id: person },
                    Resource::Site { id: site },
                ),
            ];

            for (who, principal, resource) in cases {
                let answer = if check(&principal, needs, resource, None).is_ok() {
                    "allow"
                } else {
                    "deny"
                };

                let _ = writeln!(
                    out,
                    "{who:<25} {:<10} {:<7} {answer}",
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
