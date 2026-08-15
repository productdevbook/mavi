//! What the engine answers, asked directly rather than through a request.

use cedar_policy::{ValidationMode, Validator};

use super::*;

fn site_user(grants: &[&str]) -> Principal {
    Principal::SiteUser {
        id: Uuid::now_v7(),
        grants: grants.iter().map(|grant| (*grant).to_owned()).collect(),
    }
}

#[test]
fn the_policies_validate_against_the_schema() {
    let engine = Engine::load();
    let result =
        Validator::new(engine.schema.clone()).validate(&engine.policies, ValidationMode::Strict);

    assert!(
        result.validation_passed(),
        "{:?}",
        result.validation_errors().collect::<Vec<_>>()
    );
}

#[test]
fn a_grant_that_is_held_is_allowed_and_one_that_is_not_is_refused() {
    let principal = site_user(&["content:write", "content:view"]);

    assert!(
        check(
            &principal,
            Needs::new(Capability::Content, Access::Write),
            None
        )
        .is_ok()
    );

    assert!(
        check(
            &principal,
            Needs::new(Capability::Shop, Access::Write),
            None
        )
        .is_err()
    );
}

#[test]
fn holding_nothing_reaches_nothing() {
    let principal = site_user(&[]);

    for capability in Capability::ALL {
        for access in [Access::View, Access::Write, Access::Delete] {
            assert!(
                check(&principal, Needs::new(capability, access), None).is_err(),
                "{capability}:{access} was allowed with no grants at all"
            );
        }
    }
}

#[test]
fn the_operator_is_not_a_site_role() {
    let operator = Principal::Operator { id: Uuid::now_v7() };

    assert!(
        check(
            &operator,
            Needs::new(Capability::Settings, Access::Write),
            None
        )
        .is_ok()
    );
}

#[test]
fn every_capability_has_six_grants_and_no_more() {
    let grants = every_grant();

    assert_eq!(grants.len(), Capability::ALL.len() * 6);
    assert!(grants.contains(&"content:write".to_owned()));
    assert!(grants.contains(&"content:write:own".to_owned()));
    assert!(!grants.contains(&"content:publish".to_owned()));
}

#[test]
fn an_own_grant_reaches_what_the_person_made_and_nothing_else() {
    let person = Uuid::now_v7();
    let somebody_else = Uuid::now_v7();

    let author = Principal::SiteUser {
        id: person,
        grants: ["content:write:own".to_owned(), "content:view".to_owned()]
            .into_iter()
            .collect(),
    };

    let needs = Needs::new(Capability::Content, Access::Write);

    assert!(check(&author, needs, Some(person)).is_ok());
    assert!(check(&author, needs, Some(somebody_else)).is_err());
    assert!(
        check(&author, needs, None).is_err(),
        "a post with no owner named was reachable by somebody who only has their own"
    );
}
