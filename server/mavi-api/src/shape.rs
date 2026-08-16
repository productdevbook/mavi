//! What a body is, described.
//!
//! Every endpoint names what it takes and what it answers with —
//! `WritingChanges`, `Writing` — and until now those were names and nothing
//! else. A description whose every reference points at a shape that does not
//! exist is one nothing can be generated from, which made "the description is
//! the contract" a sentence rather than a fact.
//!
//! A shape is a **value**, the way an endpoint is. Declared beside the type it
//! describes, so the two are read together, and held against the type by a
//! test that serialises a real one and compares the fields — because a
//! hand-written description that nothing checks is a description that drifts,
//! and a drifted one is worse than none.

use serde_json::{Map, Value, json};

use crate::Is;

/// One body, by the name endpoints refer to it by.
#[derive(Clone, Debug)]
pub struct Shape {
    /// What endpoints call it. Part of the API: a generated client names a
    /// type from this.
    pub named: &'static str,
    pub about: &'static str,
    pub what: What,
}

/// What a body is, at the top.
///
/// Two, because this API has two: a thing, and a bare list of a thing. The
/// second is what an endpoint answers where the answer is all of them and
/// there is nothing to page through — the languages a site writes in, what one
/// writing is filed under. Wrapping those in an object to make one shape do
/// would be changing the API to suit the description.
#[derive(Clone, Debug)]
pub enum What {
    Fields(Vec<Field>),
    Every(&'static str),
    /// An object this software does not decide the shape of. What a site put
    /// in its own fields, what the thing that set a flow off was carrying —
    /// describing those would be inventing them, and a generator acts on the
    /// invention.
    Anything,
}

/// One field of one shape.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: &'static str,
    pub about: &'static str,
    pub of: Of,
    /// Whether it must be there. A field that is optional in what a caller
    /// **sends** and always present in what comes **back** is two fields in
    /// two shapes, not one field described twice.
    pub required: bool,
    /// Whether it may be absent or explicitly nothing. Different from
    /// `required`: `excerpt` is always in what comes back and is sometimes
    /// null, and a client that cannot tell those apart crashes on the null.
    pub null: bool,
}

/// What a field holds.
#[derive(Clone, Debug)]
pub enum Of {
    /// One of the kinds a parameter can be, and the same list — so a uuid is
    /// described the same way whether it arrives in a path or in a body.
    One(Is),
    /// A list of them.
    Many(Is),
    /// Another shape, by name.
    Another(&'static str),
    /// A list of another shape.
    ManyOf(&'static str),
    /// Whatever a site put there. Custom fields on a writing, a form's
    /// answers: shapes this software does not decide and must not pretend to.
    Whatever,
    /// One of these, exactly. What a `state` or a `kind` is — said out loud,
    /// so a client can offer them rather than guess.
    OneOf(&'static [&'static str]),
}

impl Field {
    #[must_use]
    pub const fn new(name: &'static str, of: Of, about: &'static str) -> Self {
        Self {
            name,
            about,
            of,
            required: true,
            null: false,
        }
    }

    /// Something a caller may leave out.
    #[must_use]
    pub const fn maybe(mut self) -> Self {
        self.required = false;
        self
    }

    /// Something that may be there and be nothing.
    #[must_use]
    pub const fn or_null(mut self) -> Self {
        self.null = true;
        self
    }
}

impl Shape {
    #[must_use]
    pub fn new(named: &'static str, about: &'static str, fields: Vec<Field>) -> Self {
        Self {
            named,
            about,
            what: What::Fields(fields),
        }
    }

    /// A bare list of something, which is what an endpoint answers where there
    /// is nothing to page through.
    #[must_use]
    pub fn list_of(named: &'static str, of: &'static str, about: &'static str) -> Self {
        Self {
            named,
            about,
            what: What::Every(of),
        }
    }

    /// An object whose shape is not this software's to say.
    #[must_use]
    pub fn anything(named: &'static str, about: &'static str) -> Self {
        Self {
            named,
            about,
            what: What::Anything,
        }
    }

    /// Its fields, where it has any.
    #[must_use]
    pub fn fields(&self) -> &[Field] {
        match &self.what {
            What::Fields(fields) => fields,
            What::Every(_) | What::Anything => &[],
        }
    }

    /// A page of something, which is the same shape for every listing here.
    ///
    /// Written once rather than per domain, because `next` means one thing
    /// everywhere and a listing that described it differently would be a
    /// listing a client pages through differently.
    #[must_use]
    pub fn page_of(named: &'static str, of: &'static str, about: &'static str) -> Self {
        Self::new(
            named,
            about,
            vec![
                Field::new("items", Of::ManyOf(of), "What is on this page."),
                Field::new(
                    "next",
                    Of::One(Is::Text),
                    "Where the next page starts. Absent means this is the last \
                     one — never a cursor that answers an empty page.",
                )
                .maybe(),
            ],
        )
    }

    /// The names of every other shape this one refers to.
    #[must_use]
    pub fn refers_to(&self) -> Vec<&'static str> {
        match &self.what {
            What::Anything => Vec::new(),
            What::Every(of) => vec![*of],
            What::Fields(fields) => fields
                .iter()
                .filter_map(|field| match field.of {
                    Of::Another(named) | Of::ManyOf(named) => Some(named),
                    _ => None,
                })
                .collect(),
        }
    }

    /// This shape as a client is generated from it.
    #[must_use]
    pub fn described(&self) -> Value {
        let fields = match &self.what {
            What::Anything => {
                // No `properties`, which is how a schema says "an object, and
                // whatever is in it". Saying `properties: {}` instead is how a
                // generator comes to refuse every field a site invented.
                return json!({ "type": "object", "description": self.about });
            }
            What::Every(of) => {
                return json!({
                    "type": "array",
                    "description": self.about,
                    "items": { "$ref": format!("#/components/schemas/{of}") },
                });
            }
            What::Fields(fields) => fields,
        };

        let mut properties = Map::new();
        let mut required = Vec::new();

        for field in fields {
            properties.insert(field.name.to_owned(), described(field));

            if field.required {
                required.push(field.name);
            }
        }

        json!({
            "type": "object",
            "description": self.about,
            "properties": properties,
            "required": required,
        })
    }
}

fn one(is: Is) -> Value {
    let mut said = json!({ "type": is.json() });

    if let Some(format) = is.format() {
        said["format"] = json!(format);
    }

    said
}

fn described(field: &Field) -> Value {
    let mut said = match &field.of {
        Of::One(is) => one(*is),
        Of::Many(is) => json!({ "type": "array", "items": one(*is) }),
        Of::Another(named) => json!({ "$ref": format!("#/components/schemas/{named}") }),
        Of::ManyOf(named) => json!({
            "type": "array",
            "items": { "$ref": format!("#/components/schemas/{named}") },
        }),
        // Deliberately unconstrained. What a site decided its own fields are
        // is not this software's to describe, and a schema claiming otherwise
        // would be a lie a generator acts on.
        Of::Whatever => json!({}),
        Of::OneOf(these) => json!({ "type": "string", "enum": these }),
    };

    // A `$ref` beside anything else is ignored by most generators, so a field
    // that is another shape and may be nothing is said as a choice between the
    // two rather than as a reference with a note on it.
    if field.null {
        if said.get("$ref").is_some() {
            said = json!({ "oneOf": [said, { "type": "null" }] });
        } else {
            said["nullable"] = json!(true);
        }
    }

    if let Some(object) = said.as_object_mut()
        && !object.contains_key("oneOf")
    {
        object.insert("description".to_owned(), json!(field.about));
    }

    said
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_shape() -> Shape {
        Shape::new(
            "Writing",
            "Something a site wrote.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("title", Of::One(Is::Text), "What it is called."),
                Field::new("excerpt", Of::One(Is::Text), "A line about it.").or_null(),
                Field::new("fields", Of::Whatever, "Whatever this site added."),
                Field::new(
                    "state",
                    Of::OneOf(&["draft", "published"]),
                    "Whether it is out.",
                ),
                Field::new("tags", Of::ManyOf("Term"), "What it is filed under.").maybe(),
            ],
        )
    }

    #[test]
    fn a_shape_says_what_a_generator_needs_to_make_a_type() {
        let described = a_shape().described();

        assert_eq!(described["properties"]["id"]["format"], "uuid");
        assert_eq!(described["properties"]["excerpt"]["nullable"], true);
        assert_eq!(described["properties"]["state"]["enum"][1], "published");
        assert_eq!(
            described["properties"]["tags"]["items"]["$ref"],
            "#/components/schemas/Term"
        );

        // What a site decided its own fields are is not described, because
        // describing it would be inventing it.
        assert_eq!(described["properties"]["fields"]["type"], Value::Null);
    }

    #[test]
    fn what_may_be_left_out_is_not_what_may_be_nothing() {
        let described = a_shape().described();

        let required: Vec<&str> = described["required"]
            .as_array()
            .expect("a list")
            .iter()
            .filter_map(Value::as_str)
            .collect();

        // `excerpt` is always there and is sometimes null. `tags` may be
        // absent entirely. A client that cannot tell those apart crashes on
        // one of them.
        assert!(required.contains(&"excerpt"));
        assert!(!required.contains(&"tags"));
    }

    #[test]
    fn what_a_shape_refers_to_is_asked_rather_than_read() {
        assert_eq!(a_shape().refers_to(), vec!["Term"]);
        assert_eq!(
            Shape::list_of("TermList", "Term", "All of them.").refers_to(),
            vec!["Term"]
        );
    }

    #[test]
    fn a_bare_list_is_described_as_one() {
        // Some endpoints answer all of something, with nothing to page
        // through. Wrapping those in an object so one shape could do would be
        // changing the API to suit the description.
        let described = Shape::list_of("TermList", "Term", "All of them.").described();

        assert_eq!(described["type"], "array");
        assert_eq!(described["items"]["$ref"], "#/components/schemas/Term");
    }
}
