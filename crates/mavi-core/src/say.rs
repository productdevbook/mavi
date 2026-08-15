//! What a refusal is.
//!
//! A refusal is a **key and its named arguments**, never a sentence. The key
//! is stable and a client may branch on it; the arguments are what the
//! sentence would have interpolated; the English is here so that something
//! with no wording of its own still has something to show.
//!
//! The reason is not tidiness. A refusal built out of a formatted English
//! string can only ever be English, and whoever reads it may not read English.
//! A panel that has its own wording for a key can say it in the reader's
//! language without this crate changing; one that does not falls back to the
//! English below, which beats an empty box.
//!
//! What goes wrong when this is not held: a file whose own doc comment
//! explained all of the above, and then wrote seventeen English string
//! literals — so a panel shipped in two languages refused in one.

use std::collections::BTreeMap;

use serde::Serialize;

/// A refusal somebody will read: which one, and what it needs to name.
///
/// `named` is a sorted map rather than a list so that the same refusal
/// serialises the same way twice — a snapshot of the API's description is
/// compared byte for byte, and a map that reorders itself makes that
/// comparison a coin toss.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Say {
    pub key: &'static str,
    pub named: BTreeMap<&'static str, String>,
}

impl Say {
    /// A refusal with nothing to fill in.
    #[must_use]
    pub fn of(key: &'static str) -> Self {
        Self {
            key,
            named: BTreeMap::new(),
        }
    }

    /// The same, carrying something the sentence needs — a name, a count, a
    /// limit. Chainable, because most refusals carry one and some carry three.
    #[must_use]
    pub fn with(mut self, name: &'static str, value: impl ToString) -> Self {
        self.named.insert(name, value.to_string());
        self
    }

    /// The English for this key, or the key itself where nobody has written
    /// one. Returning the key rather than an empty string is deliberate: a
    /// reader who sees `that_is_the_last_owner` at least knows what was
    /// refused, and whoever left the wording out finds it in a screenshot.
    #[must_use]
    pub fn in_english(&self) -> String {
        let template = ENGLISH
            .iter()
            .find(|(key, _)| *key == self.key)
            .map_or(self.key, |(_, said)| said);

        self.named
            .iter()
            .fold(template.to_owned(), |said, (name, value)| {
                said.replace(&format!("{{{name}}}"), value)
            })
    }
}

/// Every key, and what it says in English.
///
/// Adding a refusal is adding a line here, and that is the point: a sentence
/// written at the place it is refused is a sentence nobody can translate.
///
/// A `{name}` in the English is filled from `named`. A key whose English names
/// something `named` does not carry renders the brace as it stands, which is
/// ugly on purpose — it is meant to be noticed in a screenshot rather than to
/// fail silently.
pub const ENGLISH: &[(&str, &str)] = &[];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_nobody_has_worded_says_the_key() {
        assert_eq!(
            Say::of("nothing_has_worded_this").in_english(),
            "nothing_has_worded_this"
        );
    }

    #[test]
    fn what_a_refusal_carries_is_ordered_however_it_was_given() {
        let one = Say::of("k").with("b", 2).with("a", 1);
        let two = Say::of("k").with("a", 1).with("b", 2);

        assert_eq!(
            serde_json::to_string(&one).expect("json"),
            serde_json::to_string(&two).expect("json"),
            "the same refusal serialised two ways"
        );
    }
}
