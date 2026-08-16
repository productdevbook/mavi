//! Who has an account here, and how they get in.

use mavi_api::{Field, Is, Of, Shape};

const A_TOKEN: &str = "The token that signs them in. Sent as \
                       `Authorization: Bearer`. Handed over once and kept \
                       nowhere: what this installation stores is a hash of it, \
                       so a copy of the database is not a drawer of working \
                       keys.";

#[must_use]
pub fn shapes() -> Vec<Shape> {
    let mut all = the_accounts();
    all.extend(the_roles());

    all
}

fn the_accounts() -> Vec<Shape> {
    vec![
        a_person(),
        Shape::page_of("PersonPage", "Person", "Who has an account here."),
        Shape::new(
            "Setup",
            "What making the site asks for. Answers once — an installation that \
             has been set up refuses this, which is the same thing a visitor \
             learns by looking at the front page.",
            vec![
                Field::new("site", Of::One(Is::Text), "What the site is called."),
                Field::new("name", Of::One(Is::Text), "What the owner is called."),
                Field::new("email", Of::One(Is::Text), "Where to reach them."),
                Field::new(
                    "password",
                    Of::One(Is::Text),
                    "What they will sign in with.",
                ),
            ],
        ),
        Shape::new(
            "Ready",
            "The site, its owner, and the way in — in one answer, because the \
             alternative is telling somebody to sign in with the password they \
             typed ten seconds ago and hoping nothing went wrong in between.",
            vec![
                Field::new("person", Of::Another("Person"), "The owner's account."),
                Field::new("token", Of::One(Is::Text), A_TOKEN),
            ],
        ),
        Shape::new(
            "Credentials",
            "Signing in. An address with no account and an address with the \
             wrong password are refused the same way, so this is not a way to \
             ask which addresses have accounts.",
            vec![
                Field::new("email", Of::One(Is::Text), "Where they are reached."),
                Field::new("password", Of::One(Is::Text), "What they typed."),
            ],
        ),
        Shape::new(
            "Session",
            "Signed in.",
            vec![
                Field::new("person", Of::Another("Person"), "Who."),
                Field::new("token", Of::One(Is::Text), A_TOKEN),
            ],
        ),
        Shape::new(
            "Invitation",
            "Somebody to invite. The account exists immediately and has no \
             password — which is the difference between an invitation and a \
             promise: whoever invited them can see them in the list, and the \
             link is the only way the account becomes usable.",
            vec![
                Field::new("email", Of::One(Is::Text), "Where to send the link."),
                Field::new("name", Of::One(Is::Text), "What to call them."),
                Field::new("role", Of::One(Is::Id), "Which role they hold."),
            ],
        ),
        Shape::new(
            "ChosenPassword",
            "Choosing a password with a link somebody was sent. The link says \
             what it was minted for, and one minted to prove an address will \
             not do this.",
            vec![
                Field::new("token", Of::One(Is::Text), "What was in the link."),
                Field::new("password", Of::One(Is::Text), "What they chose."),
            ],
        ),
        Shape::new(
            "Proof",
            "Proving an address with a link sent to it. Touches nothing else — \
             a link that proves an address cannot set a password, which is the \
             hole this shape exists to keep closed.",
            vec![Field::new(
                "token",
                Of::One(Is::Text),
                "What was in the link.",
            )],
        ),
    ]
}

fn the_roles() -> Vec<Shape> {
    vec![
        a_role(),
        Shape::list_of(
            "RoleList",
            "Role",
            "Every role. A handful, with nothing to page through — a role \
             picker with a cursor in it is one somebody has to page through to \
             find \"Editor\".",
        ),
        Shape::new(
            "NewRole",
            "One to make. Never the owner's: that one is made when the site is, \
             exactly once, and a second thing that can do everything is a \
             second thing to have taken.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new("grants", Of::Many(Is::Text), WHAT_A_ROLE_HOLDS).maybe(),
            ],
        ),
        Shape::new(
            "RoleChanges",
            "What may be changed. The owner's may be renamed and what it holds \
             may not be touched — it holds everything by being what it is, and \
             a set of grants written onto it would be a second answer to what \
             it can do, one that could be made smaller.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it is called.").maybe(),
                Field::new(
                    "grants",
                    Of::Many(Is::Text),
                    "The whole set, replaced. What somebody is editing is which \
                     switches are on, and sending only the ones they turned on \
                     would never turn one off.",
                )
                .maybe()
                .or_null(),
            ],
        ),
        Shape::new(
            "WhichRole",
            "Which role to move somebody to.",
            vec![Field::new("role", Of::One(Is::Id), "Which one.")],
        ),
    ]
}

const WHAT_A_ROLE_HOLDS: &str = "What it holds, as `content:write` and the \
                                 like. Each is checked against the one list of \
                                 capabilities, because a grant nobody spelled \
                                 right is a switch in a panel that looks on and \
                                 does nothing.";

fn a_role() -> Shape {
    Shape::new(
        "Role",
        "A name and a set of grants. An account holds exactly one, and that is \
         the whole of the permission system.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new("grants", Of::Many(Is::Text), WHAT_A_ROLE_HOLDS),
            Field::new(
                "is_the_owner",
                Of::One(Is::Bool),
                "The one that can do everything, including the things nothing \
                 else may. Exactly one exists; it is never made and never \
                 removed.",
            ),
            Field::new("created_at", Of::One(Is::Moment), "When it was made."),
        ],
    )
}

fn a_person() -> Shape {
    Shape::new(
        "Person",
        "Somebody with an account here.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("email", Of::One(Is::Text), "Where they are reached."),
            Field::new("name", Of::One(Is::Text), "What they are called."),
            Field::new(
                "role",
                Of::One(Is::Id),
                "Which role they hold. What that role grants is the role's own \
                 answer — repeating it on every person would be two answers to \
                 one question.",
            ),
            Field::new(
                "standing",
                Of::One(Is::Text),
                "Whether the account may be used.",
            ),
            Field::new(
                "proved_at",
                Of::One(Is::Moment),
                "When they proved the address is theirs.",
            )
            .or_null(),
            Field::new("created_at", Of::One(Is::Moment), "When they were invited."),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::{NewRole, Role, RoleChanges, WhichRole};
    use crate::store::{Invitation, Person, Setup};
    use std::collections::BTreeSet;

    fn fields_of(named: &str) -> BTreeSet<&'static str> {
        shapes()
            .iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect()
    }

    fn keys(what: &serde_json::Value) -> BTreeSet<&str> {
        what.as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        let person = Person {
            id: uuid::Uuid::nil(),
            email: "somebody@example.test".to_owned(),
            name: "Somebody".to_owned(),
            role: uuid::Uuid::nil(),
            standing: "here".to_owned(),
            proved_at: None,
            created_at: chrono::Utc::now(),
            grants: vec!["content:view".to_owned()],
        };

        let sent = serde_json::to_value(&person).expect("a person");

        // `grants` is deliberately not sent, and this is where that stays
        // true: it is read so the guard has it, and what a role grants is the
        // role's own answer.
        assert!(sent.get("grants").is_none());
        assert_eq!(keys(&sent), fields_of("Person"));
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let setup = serde_json::to_value(Setup {
            site: "A Site".to_owned(),
            name: "Somebody".to_owned(),
            email: "somebody@example.test".to_owned(),
            password: "not a real one".to_owned(),
        })
        .expect("setting up");

        assert_eq!(keys(&setup), fields_of("Setup"));

        let invitation = serde_json::to_value(Invitation {
            email: "somebody-else@example.test".to_owned(),
            name: "Somebody Else".to_owned(),
            role: uuid::Uuid::nil(),
        })
        .expect("an invitation");

        assert_eq!(keys(&invitation), fields_of("Invitation"));

        let role = serde_json::to_value(NewRole {
            name: "Editor".to_owned(),
            grants: vec!["content:view".to_owned()],
        })
        .expect("a new role");

        assert_eq!(keys(&role), fields_of("NewRole"));

        assert_eq!(
            keys(&serde_json::to_value(RoleChanges::default()).expect("changes")),
            fields_of("RoleChanges")
        );

        assert_eq!(
            keys(
                &serde_json::to_value(WhichRole {
                    role: uuid::Uuid::nil()
                })
                .expect("which role")
            ),
            fields_of("WhichRole")
        );
    }

    #[test]
    fn what_a_role_is_is_what_is_described() {
        let role = Role {
            id: uuid::Uuid::nil(),
            name: "Editor".to_owned(),
            grants: vec!["content:view".to_owned()],
            is_the_owner: false,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&role).expect("a role")),
            fields_of("Role")
        );
    }
}
