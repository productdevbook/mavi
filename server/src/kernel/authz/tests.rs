//! What the engine answers, asked directly rather than through a request.

use cedar_policy::{ValidationMode, Validator};

use super::*;

fn site_user(grants: &[&str]) -> (Principal, Uuid) {
    let site = Uuid::now_v7();
    let id = Uuid::now_v7();

    (
        Principal::SiteUser {
            id,
            site,
            grants: grants.iter().map(|grant| (*grant).to_owned()).collect(),
        },
        site,
    )
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
    let (principal, site) = site_user(&["content:write", "content:view"]);
    let resource = Resource::Site {
        id: site,
        frozen: false,
    };

    assert!(
        check(
            &principal,
            Needs::new(Capability::Content, Access::Write),
            resource,
            None
        )
        .is_ok()
    );

    assert!(
        check(
            &principal,
            Needs::new(Capability::Shop, Access::Write),
            resource,
            None
        )
        .is_err()
    );
}

#[test]
fn a_hold_closes_everything_that_changes_and_leaves_reading_open() {
    let (principal, site) = site_user(&["content:view", "content:write", "content:delete"]);
    let frozen = Resource::Site {
        id: site,
        frozen: true,
    };

    assert!(
        check(
            &principal,
            Needs::new(Capability::Content, Access::View),
            frozen,
            None
        )
        .is_ok()
    );

    for access in [Access::Write, Access::Delete] {
        assert!(
            check(
                &principal,
                Needs::new(Capability::Content, access),
                frozen,
                None
            )
            .is_err(),
            "{access} went through on a site that is on hold"
        );
    }
}

#[test]
fn a_site_user_cannot_reach_another_site() {
    let (principal, _) = site_user(&["content:write"]);
    let somewhere_else = Resource::Site {
        id: Uuid::now_v7(),
        frozen: false,
    };

    assert!(
        check(
            &principal,
            Needs::new(Capability::Content, Access::Write),
            somewhere_else,
            None
        )
        .is_err()
    );
}

#[test]
fn holding_nothing_reaches_nothing() {
    let (principal, site) = site_user(&[]);
    let resource = Resource::Site {
        id: site,
        frozen: false,
    };

    for capability in Capability::ALL {
        for access in [Access::View, Access::Write, Access::Delete] {
            assert!(
                check(&principal, Needs::new(capability, access), resource, None).is_err(),
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
            Resource::Platform,
            None
        )
        .is_ok()
    );
}

#[test]
fn a_customer_reads_their_own_charge_and_not_another_site_s() {
    let site = Uuid::now_v7();
    let customer = Principal::Customer {
        id: Uuid::now_v7(),
        site,
    };

    assert!(
        check(
            &customer,
            Needs::new(Capability::Shop, Access::View),
            Resource::Charge {
                id: Uuid::now_v7(),
                site
            },
            None
        )
        .is_ok()
    );

    assert!(
        check(
            &customer,
            Needs::new(Capability::Shop, Access::View),
            Resource::Charge {
                id: Uuid::now_v7(),
                site: Uuid::now_v7()
            },
            None
        )
        .is_err()
    );
}

#[test]
fn a_charge_cannot_be_changed_by_the_person_paying_it() {
    let site = Uuid::now_v7();
    let customer = Principal::Customer {
        id: Uuid::now_v7(),
        site,
    };

    assert!(
        check(
            &customer,
            Needs::new(Capability::Shop, Access::Write),
            Resource::Charge {
                id: Uuid::now_v7(),
                site
            },
            None
        )
        .is_err()
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
    let site = Uuid::now_v7();
    let person = Uuid::now_v7();
    let somebody_else = Uuid::now_v7();

    let author = Principal::SiteUser {
        id: person,
        site,
        grants: ["content:write:own".to_owned(), "content:view".to_owned()]
            .into_iter()
            .collect(),
    };

    let resource = Resource::Site {
        id: site,
        frozen: false,
    };

    let needs = Needs::new(Capability::Content, Access::Write);

    assert!(check(&author, needs, resource, Some(person)).is_ok());
    assert!(check(&author, needs, resource, Some(somebody_else)).is_err());
    assert!(
        check(&author, needs, resource, None).is_err(),
        "a post with no owner named was reachable by somebody who only has their own"
    );
}
