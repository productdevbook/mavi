//! Where a page went.
//!
//! Renaming a writing leaves its old address behind, pointing at the new one.
//! Both halves are the point: a rename that writes the row and an edge that
//! never reads it is every link anybody made answering "not here" while the
//! answer sits in a table.
//!
//! What is kept is the slug rather than the whole address, because nothing
//! here knows where a design puts its posts. `/blog/old` becomes `/blog/new`,
//! whatever `/blog` is — and a site that moves its posts under `/writing`
//! keeps working without anybody rewriting rows.

/// The last part of an address — the name a writing answers under.
///
/// One answer to it, because the edge asks the site about this name and then
/// builds the new address around it, and two ways of deciding what "it" is
/// would send somebody to an address assembled out of two different guesses.
#[must_use]
pub fn slug_of(path: &str) -> Option<&str> {
    path.trim_matches('/')
        .rsplit('/')
        .next()
        .filter(|slug| !slug.is_empty())
}

/// Where an address goes now, if it goes anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Went {
    pub to: String,
}

/// The address to send somebody to, given what the site knows about a slug.
///
/// `answers` is what the site has for the last part of the address: the
/// language it was written in, and what it is called now. More than one means
/// the same name was used in two languages, and which one this is depends on
/// the address — a guess between them sends somebody to the wrong page, so
/// nothing is sent at all unless the address says.
#[must_use]
pub fn went(path: &str, answers: &[(String, String)]) -> Option<Went> {
    let asked = path.trim_matches('/');
    let slug = slug_of(path)?;

    let first = asked.split('/').next().unwrap_or_default();

    let now_at = match answers {
        [] => return None,
        [(_, one)] => one,
        many => {
            // Two languages, one name. The address says which only when the
            // design writes the language into it.
            &many
                .iter()
                .find(|(language, _)| language == first)
                .map(|(_, now_at)| now_at.clone())?
        }
    };

    let prefix = &asked[..asked.len() - slug.len()];

    Some(Went {
        to: format!("/{prefix}{now_at}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(now_at: &str) -> Vec<(String, String)> {
        vec![("en".to_owned(), now_at.to_owned())]
    }

    #[test]
    fn a_renamed_page_keeps_its_old_address_working() {
        assert_eq!(
            went("/blog/old", &one("new")),
            Some(Went {
                to: "/blog/new".to_owned()
            })
        );
    }

    #[test]
    fn whatever_the_design_calls_the_folder_is_kept() {
        // Nothing here knows where a design puts its posts, so the prefix is
        // carried rather than rebuilt.
        assert_eq!(
            went("/writing/old", &one("new")).map(|went| went.to),
            Some("/writing/new".to_owned())
        );
        assert_eq!(
            went("/old", &one("new")).map(|went| went.to),
            Some("/new".to_owned())
        );
    }

    #[test]
    fn one_name_in_two_languages_is_not_guessed_between() {
        let both = vec![
            ("en".to_owned(), "new".to_owned()),
            ("tr".to_owned(), "yeni".to_owned()),
        ];

        // The address says which.
        assert_eq!(
            went("/en/old", &both).map(|went| went.to),
            Some("/en/new".to_owned())
        );
        assert_eq!(
            went("/tr/old", &both).map(|went| went.to),
            Some("/tr/yeni".to_owned())
        );

        // And where it does not, nothing is sent: half of the readers would be
        // sent to a page in a language they did not ask for.
        assert_eq!(went("/old", &both), None);
    }

    #[test]
    fn an_address_nothing_was_ever_called_goes_nowhere() {
        assert_eq!(went("/old", &[]), None);
        assert_eq!(went("/", &one("new")), None);
    }
}
