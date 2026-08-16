//! What a form asks for, and what people sent it.
//!
//! Two audiences and one table. Whoever declares a form is signed in and does
//! it rarely; whoever fills it in is anybody at all and may do it a thousand
//! times a minute. Every rule that can be checked at the first is checked
//! there, and what is left on the public endpoint is the smallest set that
//! genuinely cannot be.
//!
//! The endpoints anybody can reach sit under `/api/open/` rather than mixed in
//! with the rest. That is not tidiness — it means "which endpoints are public"
//! is answered by reading a path instead of by trusting that every declaration
//! got its audience right.

pub mod described;
pub mod field;
pub mod filled;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use mavi_core::slug::Slug;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub use field::{Declared, Field};
pub use filled::Filled;

id!(
    /// One form.
    FormId
);

id!(
    /// One thing somebody sent a form.
    FilledId
);

pub const FORMS: &str = "forms";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(FORMS, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(FORMS, Access::Write)
}

/// How long what people send is kept, where the form does not say.
///
/// Every table holding somebody's own words has one of these. A default that
/// is "forever" is a decision nobody made.
pub const KEPT_FOR_DAYS: i32 = 365;

/// One form, as whoever made it sees it.
#[derive(Clone, Debug, Serialize)]
pub struct Form {
    pub id: FormId,
    pub slug: Slug,
    pub name: String,
    pub fields: Declared,
    pub open: bool,
    pub kept_days: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The same form, as a page that is about to draw it sees it.
///
/// A separate type rather than the same one with fields left out. Left out is
/// something somebody has to keep doing; a type is something they cannot
/// forget — and what is missing here is everything about the site rather than
/// about the form: how long what people send is kept, when it was made,
/// whether it is one of many.
#[derive(Clone, Debug, Serialize)]
pub struct OpenForm {
    pub slug: Slug,
    pub name: String,
    pub fields: Declared,
}

/// One thing somebody sent.
///
/// Where it came from is not here. It is written, because "is this one person
/// sending it fifty times" is a question worth being able to answer, and it is
/// not answered back out — an address is about whoever filled the form in
/// rather than about what they said.
#[derive(Clone, Debug, Serialize)]
pub struct Sent {
    pub id: FilledId,
    pub form_id: FormId,
    pub answers: serde_json::Map<String, serde_json::Value>,
    pub seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

/// Everything this domain answers: what the panel reaches, and what anybody
/// does. Two lists rather than one, because the difference between them is the
/// most important thing about this domain.
#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_forms();
    all.extend(what_came_in());
    all.extend(for_anybody());
    all
}

/// The forms themselves, which only somebody signed in touches.
fn the_forms() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/forms",
            named: "forms.list",
            about: "What this site asks people, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("FormPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/forms",
            named: "forms.make",
            about: "Makes one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewForm"),
            answers: Answers::Made("Form"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/forms/{id}",
            named: "forms.read",
            about: "One form, and everything it asks for.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which form.")],
            takes: None,
            answers: Answers::With("Form"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/forms/{id}",
            named: "forms.change",
            about: "Renames one, changes what it asks for, or closes it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which form.")],
            takes: Some("FormChanges"),
            answers: Answers::With("Form"),
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/forms/{id}",
            named: "forms.remove",
            about: "Removes one, and what people sent it with it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which form.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// What people sent them, which only somebody signed in reads.
fn what_came_in() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/forms/{id}/filled",
            named: "forms.filled",
            about: "What people sent this form, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("id", Is::Id, "Which form."),
                Parameter::query("unseen", Is::Bool, "Only what nobody has read yet."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("FilledPage"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/forms/{id}/seen",
            named: "forms.mark-seen",
            about: "Says everything sent to this form up to now has been read.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which form.")],
            takes: None,
            answers: Answers::With("Seen"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/filled/{id}",
            named: "filled.forget",
            about: "Forgets one thing somebody sent.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which one.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// What anybody at all reaches. Every one of these is under `/api/open/`.
fn for_anybody() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/open/forms/{slug}",
            named: "open.form",
            about: "What an open form asks for, so a page can draw it.",
            who: Who::Anybody,
            parameters: vec![Parameter::path("slug", Is::Text, "The form's address.")],
            takes: None,
            // What it asks for, and nothing about the site: not how many
            // people have sent it, not how long what they sent is kept, not
            // when it was made.
            answers: Answers::With("OpenForm"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/open/forms/{slug}",
            named: "open.fill-in",
            about: "Sends a form. Anybody may, which is why every rule is on this side.",
            who: Who::Anybody,
            parameters: vec![Parameter::path("slug", Is::Text, "The form's address.")],
            takes: Some("Filled"),
            answers: Answers::Made("Received"),
            // Never says whether a closed form exists: a form that is not open
            // and a form that was never made answer the same way, or the
            // refusal is a way to ask what this site has.
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn no_two_of_these_are_the_same_route() {
        // This domain is the one that nearly made the mistake: a public
        // endpoint wants the form's address and the panel's wants its id, and
        // the obvious pair — `/api/forms/{slug}/filled` beside
        // `/api/forms/{id}/filled` — is one route with two names for its hole.
        let clashes = Api::of(endpoints()).clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn what_anybody_can_reach_says_so_in_its_path() {
        // "Which endpoints are public" answered by reading rather than by
        // trusting that every declaration got its audience right.
        for endpoint in endpoints() {
            assert_eq!(
                endpoint.who == Who::Anybody,
                endpoint.path.starts_with("/api/open/"),
                "{} is one thing in its path and another in its audience",
                endpoint.named
            );
        }
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(FORMS));
    }

    #[test]
    fn the_order_ends_with_something_unique() {
        assert_eq!(
            BY_RECENT.keys().last().expect("a key").column,
            "id",
            "an order that cannot break a tie"
        );
    }

    #[test]
    fn what_a_page_is_shown_of_a_form_is_the_form_and_nothing_about_the_site() {
        // Serialised rather than read, because "I left that field out" is a
        // claim about a type and this is the type.
        let open = OpenForm {
            slug: Slug::parse("contact").expect("an address"),
            name: "Contact".to_owned(),
            fields: Declared::checked(Vec::new()).expect("a form"),
        };

        let shown = serde_json::to_value(&open).expect("json");
        let mut keys: Vec<&str> = shown
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(keys, ["fields", "name", "slug"]);
    }
}
