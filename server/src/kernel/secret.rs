//! Two kinds of secret, and what each one cannot do.
//!
//! [`Secret`] is one arriving: it can be read by the code that needs it and
//! cannot be serialized, so it never leaves in an answer or a log. [`Shown`]
//! is one going out once — a token being handed over — which serializes and
//! cannot be printed. The type is the rule; nothing has to remember it.
use serde::Deserialize;

/// Holds something that must not reach a log. `Debug` and `Display` mask it, so
/// putting one in a log line prints nothing worth stealing, and `Serialize` is
/// deliberately not implemented: a secret leaves this process only where
/// somebody wrote [`Secret::expose`], which greps.
#[derive(Clone, Deserialize)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl<T> std::fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("…")
    }
}

impl<T> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

/// A secret is a string in the description of the API: what it is made of is
/// this machine's business, and what a client sends is a string either way.
// A generic type has to say how it composes before it can be a schema; there
// is nothing to compose here, because a secret is a string whatever it holds.
impl<T> utoipa::__dev::ComposeSchema for Secret<T> {
    fn compose(
        _: Vec<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>,
    ) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <String as utoipa::PartialSchema>::schema()
    }
}

impl<T> utoipa::ToSchema for Secret<T> {}

/// Something secret on its way *out*: a token that was just made, shown once.
///
/// [`Secret`] deliberately cannot be serialized, which is right for everything
/// coming in and wrong for the handful of answers whose whole purpose is to
/// hand somebody a key. This serializes as the string it is and still prints as
/// nothing, so a `warn!(?answer)` says nothing worth stealing.
#[derive(Clone, serde::Serialize)]
#[serde(transparent)]
pub struct Shown(String);

impl Shown {
    #[must_use]
    pub const fn new(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shown(…)")
    }
}

impl From<String> for Shown {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl utoipa::PartialSchema for Shown {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        String::schema()
    }
}

impl utoipa::ToSchema for Shown {}

/// Whether two secrets are the same, without saying how far along they stopped
/// matching — the timing of a byte-by-byte comparison is enough to guess one.
#[must_use]
pub fn same(one: &[u8], two: &[u8]) -> bool {
    if one.len() != two.len() {
        return false;
    }

    one.iter()
        .zip(two.iter())
        .fold(0_u8, |seen, (a, b)| seen | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_does_not_print_what_it_holds() {
        let secret = Secret::new("hunter2".to_owned());

        assert!(!format!("{secret:?}").contains("hunter2"));
        assert!(!format!("{secret}").contains("hunter2"));
        assert_eq!(secret.expose(), "hunter2");
    }

    #[test]
    fn it_does_not_print_what_it_holds_inside_something_else() {
        #[derive(Debug)]
        #[allow(dead_code, reason = "only the Debug output is being asked about")]
        struct Credentials {
            user: String,
            password: Secret<String>,
        }

        let rendered = format!(
            "{:?}",
            Credentials {
                user: "someone".to_owned(),
                password: Secret::new("hunter2".to_owned()),
            }
        );

        assert!(!rendered.contains("hunter2"), "{rendered}");
    }
}
