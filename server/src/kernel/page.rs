//! One pagination shape, for everything.
//!
//! Cursor rather than offset: a page taken by counting rows moves under
//! whoever is reading it, and the count itself gets slower the further in they
//! go. Every listing in the API returns this, and no listing returns a set with
//! no limit on it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_LIMIT: u16 = 25;
pub const MAX_LIMIT: u16 = 100;

/// Where the last page stopped, for the listings whose cursor is a moment —
/// which is most of them, because most are newest first.
#[must_use]
pub fn older_than(after: Option<&str>) -> Option<DateTime<Utc>> {
    after
        .and_then(|after| DateTime::parse_from_rfc3339(after).ok())
        .map(|at| at.with_timezone(&Utc))
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Query {
    /// Where the last page stopped. Opaque to whoever holds it.
    pub after: Option<String>,
    pub limit: Option<u16>,
}

impl Query {
    /// Clamped rather than refused: an over-large limit is somebody hoping,
    /// not somebody wrong, and refusing it teaches nothing a smaller page does
    /// not.
    #[must_use]
    pub fn limit(&self) -> u16 {
        self.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
    }

    /// One more than asked for: whether there is another page is answered by
    /// whether the extra row came back, without a second query.
    #[must_use]
    pub fn fetch(&self) -> i64 {
        i64::from(self.limit()) + 1
    }
}

/// Described as itself, so the API says what it actually answers: a listing
/// that claims to give back the thing rather than a page of them is a client
/// generated against a shape that does not exist.
#[derive(Clone, Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

/// Said by hand rather than derived, for one reason: the derive describes the
/// page and forgets what is in it, so a client generated from the description
/// referred to a `Post` the description never mentioned. This names the thing
/// and registers it beside the page of them.
///
/// `PartialSchema` comes free from this, which is why it is not written twice.
impl<T: utoipa::ToSchema> utoipa::__dev::ComposeSchema for Page<T> {
    fn compose(
        _: Vec<utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>>,
    ) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{ArrayBuilder, ObjectBuilder, Ref, SchemaType, Type};

        ObjectBuilder::new()
            .property(
                "items",
                ArrayBuilder::new().items(Ref::from_schema_name(T::name())),
            )
            .required("items")
            .property(
                "next",
                ObjectBuilder::new().schema_type(SchemaType::from_iter([Type::String, Type::Null])),
            )
            .into()
    }
}

impl<T: utoipa::ToSchema> utoipa::ToSchema for Page<T> {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Page")
    }

    fn schemas(
        all: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        T::schemas(all);
        all.push((
            T::name().into_owned(),
            <T as utoipa::PartialSchema>::schema(),
        ));
    }
}

impl<T> Page<T> {
    /// Takes the rows a [`Query::fetch`] returned and splits the extra one off.
    pub fn build(query: &Query, mut rows: Vec<T>, cursor: impl Fn(&T) -> String) -> Self {
        let limit = query.limit() as usize;

        let next = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(&cursor)
        } else {
            None
        };

        Self { items: rows, next }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            next: None,
        }
    }
}
