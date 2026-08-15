//! Ids that know what they are.
//!
//! Every id in the crate this replaces is a bare `Uuid`, so a post's id and an
//! order's id are the same type and the compiler is content to see one passed
//! where the other belongs. Nothing catches it: both are valid, both are
//! present, and the query simply finds nothing — which reads as "it is not
//! there" rather than "you asked the wrong question".
//!
//! A typed id costs one line at the declaration and nothing at the call site.
//!
//! They are v7, so they sort by when they were made. That matters more than it
//! sounds: it makes an id a legitimate last column of a [`Keyset`](crate::page::Keyset),
//! which is what lets a cursor tell two rows apart when everything else about
//! them is equal.

/// Declares an id type: a `Uuid` that will not stand in for another one.
#[macro_export]
macro_rules! id {
    ($(#[$about:meta])* $name:ident) => {
        $(#[$about])*
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash,
            ::serde::Serialize, ::serde::Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub ::uuid::Uuid);

        impl $name {
            /// A new one, ordered by when it was made.
            #[must_use]
            pub fn new() -> Self {
                Self(::uuid::Uuid::now_v7())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<$name> for ::uuid::Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

#[cfg(test)]
mod tests {
    id!(
        /// Something one domain owns.
        OneId
    );
    id!(
        /// Something another owns.
        AnotherId
    );

    #[test]
    fn an_id_is_ordered_by_when_it_was_made() {
        let first = OneId::new();
        let second = OneId::new();

        // Not a formality: this is what makes an id a usable last key in a
        // cursor, and a v4 would make the sort meaningless without warning.
        assert!(
            first < second,
            "two ids made in order did not sort in order"
        );
    }

    #[test]
    fn one_kind_of_id_is_not_another() {
        // The real assertion is that the line below does not compile:
        //
        //     let _: OneId = AnotherId::new();
        //
        // which a test cannot state. What it can state is that they are
        // distinct types carrying the same thing, so the conversion has to be
        // written out and is therefore visible in a diff.
        let one = OneId::new();
        let other = AnotherId(one.0);

        assert_eq!(uuid::Uuid::from(one), uuid::Uuid::from(other));
    }
}
