//! Whether a request gets in, and whether what it did was written down.
//!
//! One path. In the crate this replaces there were two — a site's and a
//! console's — and the second had no audit gate on it at all, so a write
//! through it could answer before leaving a record. Nothing had gone wrong
//! yet, because only two endpoints were mounted there and both happened to
//! write one. A rule enforced in one of two places is not enforced; it is
//! remembered.

use mavi_api::{Endpoint, Who};
use mavi_core::error::{Code, Error, Result};
use mavi_core::grant::{Needs, may};
use mavi_core::say::Say;

use crate::{Answered, Caller};

pub const YOU_ARE_NOT_SIGNED_IN: &str = "you_are_not_signed_in";
pub const THAT_IS_FOR_SOMEBODY_ENROLLED: &str = "that_is_for_somebody_enrolled";

/// A request that has been let in.
///
/// Carried rather than returned as a `bool`, so that reaching a handler
/// without having been admitted is not something a caller can arrange.
#[derive(Debug)]
pub struct Admitted<'a> {
    pub caller: &'a Caller,
    pub endpoint: &'a Endpoint,
}

/// Whether this caller may reach this endpoint at all.
///
/// `needs` is what the endpoint wants held, where it wants anything. It is a
/// parameter rather than a field on the endpoint because the description
/// belongs to `mavi-api`, which must not know what a capability is — the list
/// of those lives with the domains (#77).
///
/// `owner` is who made the thing being reached, where that is known before the
/// row is read. `None` for a listing, which is the case that matters: an
/// `:own` grant must not answer a question about nobody in particular.
pub fn admit<'a>(
    caller: &'a Caller,
    endpoint: &'a Endpoint,
    needs: Option<Needs>,
    owner: Option<&str>,
) -> Result<Admitted<'a>> {
    // The three that are let in, said as one thing rather than as three empty
    // arms — which reads better and, more to the point, means adding a fourth
    // audience has to be written here rather than falling through.
    let the_right_sort = matches!(
        (endpoint.who, caller),
        (Who::Anybody, _)
            | (Who::AnAccount, Caller::AnAccount { .. })
            | (Who::AStudent, Caller::AStudent { .. })
    );

    if !the_right_sort {
        // A student reaching an account's endpoint is not "signed in as the
        // wrong thing" — from the endpoint's side there is nobody here who
        // could be let in, and saying which would answer a question about who
        // else exists.
        let said = match endpoint.who {
            Who::AStudent => THAT_IS_FOR_SOMEBODY_ENROLLED,
            _ => YOU_ARE_NOT_SIGNED_IN,
        };

        return Err(Error::new(Code::Unauthenticated, Say::of(said)));
    }

    if let Some(needs) = needs {
        may(&caller.grants(), needs, caller.id(), owner)?;
    }

    Ok(Admitted { caller, endpoint })
}

/// What an endpoint answered, held against what it said it was.
///
/// An endpoint that says it changes something and answers without a receipt is
/// a change nobody can find afterwards. That is refused here rather than
/// logged, because a missing audit row is not something to discover later.
///
/// **What counts as a change is what the endpoint said**, never the verb it
/// arrived by. A single `POST` carrying a protocol has reads under it, and
/// asking the verb was how listing an assistant's tools came to be recorded as
/// a change to the site.
pub fn wrote_it_down<T>(endpoint: &Endpoint, answered: &Answered<T>) -> Result<()> {
    if endpoint.changes && answered.wrote().is_none() {
        return Err(Error::internal(std::io::Error::other(format!(
            "{} changed something and left no record of it",
            endpoint.named
        ))));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::{Answers, Method};
    use mavi_core::grant::{Access, Grants};

    use crate::Receipt;

    fn an_endpoint(who: Who, changes: bool) -> Endpoint {
        Endpoint {
            method: Method::Post,
            path: "/api/things",
            named: "things.write",
            about: "Writes one.",
            who,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::Nothing,
            refuses: &[],
            changes,
        }
    }

    fn holding(what: &[&str]) -> Caller {
        Caller::AnAccount {
            id: "me".to_owned(),
            grants: Grants::of(what.iter().map(ToString::to_string)),
        }
    }

    #[test]
    fn anybody_reaches_what_is_for_anybody() {
        let open = an_endpoint(Who::Anybody, false);

        assert!(admit(&Caller::Nobody, &open, None, None).is_ok());
        assert!(admit(&holding(&[]), &open, None, None).is_ok());
    }

    #[test]
    fn nobody_reaches_what_wants_an_account() {
        let shut = an_endpoint(Who::AnAccount, false);
        let refused = admit(&Caller::Nobody, &shut, None, None).expect_err("a refusal");

        assert_eq!(refused.code(), Code::Unauthenticated);
    }

    #[test]
    fn a_student_is_not_an_account() {
        // Two audiences, and holding one is not holding the other. A student
        // reaching an account's endpoint is turned away for the same reason
        // nobody is, and is told the same thing.
        let shut = an_endpoint(Who::AnAccount, false);
        let student = Caller::AStudent {
            id: "them".to_owned(),
        };

        assert_eq!(
            admit(&student, &shut, None, None)
                .expect_err("a refusal")
                .code(),
            Code::Unauthenticated
        );
    }

    #[test]
    fn holding_the_wrong_grant_is_not_holding_it() {
        let shut = an_endpoint(Who::AnAccount, false);
        let needs = Needs::new("content", Access::Write);

        assert!(admit(&holding(&["content:view"]), &shut, Some(needs), None).is_err());
        assert!(admit(&holding(&["content:write"]), &shut, Some(needs), None).is_ok());
    }

    #[test]
    fn a_change_that_left_no_record_does_not_answer() {
        // The rule, and the shape of the hole it closes. It is asked of what
        // the endpoint said about itself, never of the verb: a `POST` that
        // reads is not a change, and this is where that distinction is made
        // once rather than guessed at each mounting.
        let changing = an_endpoint(Who::AnAccount, true);
        let reading = an_endpoint(Who::AnAccount, false);

        let quiet: Answered<()> = Answered::Read(());
        let recorded = Answered::Changed((), Receipt::pretend());

        assert!(wrote_it_down(&changing, &quiet).is_err());
        assert!(wrote_it_down(&changing, &recorded).is_ok());

        // And a read is not held to it, however it arrived.
        assert!(wrote_it_down(&reading, &quiet).is_ok());
    }

    #[test]
    fn a_post_that_only_reads_is_not_a_change() {
        // Named for the failure rather than the mechanism: `/mcp` is one POST
        // that answers `tools/list`, and recording that as a change filled the
        // audit log with handshakes.
        let mut listing = an_endpoint(Who::AnAccount, false);
        listing.method = Method::Post;

        let quiet: Answered<()> = Answered::Read(());

        assert!(wrote_it_down(&listing, &quiet).is_ok());
    }
}
