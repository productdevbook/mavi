//! Who is asking, worked out from what they sent.
//!
//! One query against one index, on every request that carries a token. What it
//! answers is the whole of what the guard has to work with, and it is asked
//! exactly once per request — the router carries the answer afterwards.

use std::sync::Arc;

use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_http::Caller;
use mavi_serve::WhoIsAsking;

/// Reads the token, finds whoever is holding it.
///
/// Anything that is not a bearer token is nobody. Not a refusal: a request
/// with no token is how every open endpoint is reached, and turning "no token"
/// into an error here would refuse the front page.
#[must_use]
pub fn whoever_holds(db: Db) -> WhoIsAsking {
    Arc::new(move |headers| {
        let db = db.clone();

        Box::pin(async move {
            let Some(token) = headers
                .get("authorization")
                .and_then(|said| said.to_str().ok())
                .and_then(|said| said.strip_prefix("Bearer "))
                .map(ToOwned::to_owned)
            else {
                return Caller::Nobody;
            };

            let Ok(mut tx) = db.begin().await else {
                return Caller::Nobody;
            };

            // A session first, because that is what nearly every request
            // carries: a person at a screen. A key is asked about only when
            // the token was not a session, so the common way in stays one
            // query.
            match mavi_people::store::whoever_holds(&mut tx, &token).await {
                Ok(Some((person, session))) => Caller::AnAccount {
                    id: person.id.to_string(),
                    grants: Grants::of(person.grants),
                    session: Some(session.to_string()),
                },
                // A key a script or an assistant was given. No session, so
                // signing out does not apply to it — what stops one is being
                // ended, which is its own endpoint.
                Ok(None) => match mavi_people::store::whoever_holds_a_key(&mut tx, &token).await {
                    Ok(Some(person)) => Caller::AnAccount {
                        id: person.id.to_string(),
                        grants: Grants::of(person.grants),
                        session: None,
                    },
                    _ => Caller::Nobody,
                },
                // A token that is not one of ours, a session that has ended,
                // an account that was stopped — all of them are nobody, and
                // the endpoint's own rule is what turns that into an answer.
                _ => Caller::Nobody,
            }
        })
    })
}
