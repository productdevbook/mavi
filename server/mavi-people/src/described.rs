//! Who has an account here, and how they get in.

use mavi_api::{Field, Is, Of, Shape};

const A_TOKEN: &str = "The token that signs them in. Sent as \
                       `Authorization: Bearer`. Handed over once and kept \
                       nowhere: what this installation stores is a hash of it, \
                       so a copy of the database is not a drawer of working \
                       keys.";

#[must_use]
pub fn shapes() -> Vec<Shape> {
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
    }
}
