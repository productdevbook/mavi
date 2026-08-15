//! Serving the pages a build published.
//!
//! Every address a site answers on reaches this process, so a site's pages are
//! served by the same thing that serves its panel: one deployment, and a site
//! that has just been made appears the moment it is published rather than when
//! somebody adds a container for it.
//!
//! What is served is a build, named by the publish that made it. Nothing here
//! reads a site's source: what a build produced is in the store, under the id
//! of the publish that produced it, and going live is that id changing.

use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Caller};

/// Where a build's files live in the store. The publish id is the whole of the
/// name: a new build is a new folder, and going live is one row changing.
#[must_use]
pub fn at(publish: uuid::Uuid, path: &str) -> String {
    format!(
        "builds/{}/{}",
        publish.simple(),
        path.trim_start_matches('/')
    )
}

/// Where a build made to look at answers, on the site's own address.
///
/// Under the build's own id, which nobody guesses and nothing links to: a
/// preview is for whoever asked for it, and a site that is not published yet
/// should not have its next design found by walking its addresses.
#[must_use]
pub fn preview_path(publish: uuid::Uuid) -> String {
    format!("/_preview/{}/", publish.simple())
}

/// What a page is cached for.
///
/// A minute, because an address does not carry the id of the build that
/// answered it: a longer one would serve yesterday's page from somebody's
/// browser after a publish, and the fix for that is fingerprinted names in the
/// theme rather than a number here.
const HELD_FOR: &str = "public, max-age=60";

/// Anything that is not the API, on a site's own address.
pub async fn serve(State(state): State<AppState>, _caller: Caller, request: Request) -> Response {
    let path = request.uri().path().to_owned();

    match page(&state, &path).await {
        Ok(answer) => answer,
        Err(AppError::NotFound(_)) => match moved(&state, &path).await {
            Ok(Some(answer)) => answer,
            _ => not_found(&state).await,
        },
        Err(why) => why.into_response(),
    }
}

/// Where a page went, for an address that used to be one.
///
/// Renaming a post leaves the old slug behind pointing at the new one, and
/// until this read it the row was written and never looked at: every link
/// anybody had made to that page answered 404. The prefix is kept because
/// nothing here knows where a theme puts its posts — `/blog/old` becomes
/// `/blog/new`, whatever `/blog` is.
async fn moved(state: &AppState, path: &str) -> Result<Option<Response>> {
    let asked = path.trim_matches('/');

    let Some(slug) = asked.rsplit('/').next().filter(|slug| !slug.is_empty()) else {
        return Ok(None);
    };

    let mut conn = state.db.begin().await?;

    let rows: Vec<(String, String)> =
        sqlx::query_as("select language, now_at from redirects where was = $1")
            .bind(slug)
            .fetch_all(conn.conn())
            .await?;

    conn.commit().await?;

    // The same name in two languages went to two different places, and the
    // address says which only when the theme writes the language into it.
    // Guessing between them would send somebody to the wrong page.
    let first = asked.split('/').next().unwrap_or_default();
    let now_at = match rows.as_slice() {
        [(_, now_at)] => now_at,
        many => match many.iter().find(|(language, _)| language == first) {
            Some((_, now_at)) => now_at,
            None => return Ok(None),
        },
    };

    let prefix = &asked[..asked.len() - slug.len()];
    let to = format!("/{prefix}{now_at}");

    let Ok(location) = HeaderValue::from_str(&to) else {
        return Ok(None);
    };

    let mut response = StatusCode::MOVED_PERMANENTLY.into_response();
    response.headers_mut().insert(header::LOCATION, location);

    Ok(Some(response))
}

async fn page(state: &AppState, path: &str) -> Result<Response> {
    if let Some(rest) = path.strip_prefix("/_preview/") {
        return previewed(state, rest).await;
    }

    let publish = live(state).await?;

    let wanted = file_for(path).ok_or(AppError::NotFound("page"))?;

    let bytes = state.store.get(&at(publish, &wanted)).await?;
    let kind = kind_of(&wanted);

    Ok(answered(bytes, kind, StatusCode::OK, publish, &wanted))
}

/// Which file answers an address, where one does.
///
/// A folder is its index: `/about` and `/about/` are the same page, and a theme
/// that wrote `about/index.html` should answer on both. Anything with a dot in
/// it is asking for a file by name.
///
/// Nothing that climbs is a page. The store refuses a path that climbs as well,
/// and this is here anyway: what `/../../etc/passwd` should get is the site's
/// own "not here" rather than a different error saying which guard stopped it,
/// and a store somewhere else may not have the same guard.
#[must_use]
pub fn file_for(path: &str) -> Option<String> {
    let asked = path.trim_matches('/');

    let climbs = asked.split('/').any(|part| {
        let part = part.to_ascii_lowercase();

        part == "."
            || part == ".."
            || part.contains('\\')
            // Looked for as written rather than decoded: decoding would turn
            // `%2f` into a separator this has already finished splitting on.
            || part.contains("%2e")
            || part.contains("%2f")
            || part.contains("%5c")
    });

    if climbs {
        return None;
    }

    if asked.is_empty() {
        Some("index.html".to_owned())
    } else if asked.contains('.') {
        Some(asked.to_owned())
    } else {
        Some(format!("{asked}/index.html"))
    }
}

/// The page a site says to show for something that is not there — and, failing
/// that, the plainest possible answer rather than nothing.
async fn not_found(state: &AppState) -> Response {
    let Ok(publish) = live(state).await else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    match state.store.get(&at(publish, "404.html")).await {
        Ok(bytes) => answered(
            bytes,
            "text/html; charset=utf-8",
            StatusCode::NOT_FOUND,
            publish,
            "404.html",
        ),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn answered(
    bytes: Vec<u8>,
    kind: &'static str,
    status: StatusCode,
    publish: uuid::Uuid,
    path: &str,
) -> Response {
    let tag = format!("\"{}-{}\"", publish.simple(), path.len());

    let mut response = (status, bytes).into_response();
    let headers = response.headers_mut();

    if let Ok(value) = HeaderValue::from_str(kind) {
        headers.insert(header::CONTENT_TYPE, value);
    }

    if let Ok(value) = HeaderValue::from_str(&tag) {
        headers.insert(header::ETAG, value);
    }

    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(HELD_FOR));

    // What a build produced is a site's own, and nothing here decides what it
    // may do. The policy this machine puts on its own answers would break a
    // page that loads its own stylesheet.
    headers.remove(header::CONTENT_SECURITY_POLICY);

    response
}

/// A build somebody asked to look at, served under its own id.
///
/// The id has to be one of this site's own previews: it comes from the address
/// and the store is asked for exactly that build, so a build id taken
/// from another site answers nothing here.
async fn previewed(state: &AppState, rest: &str) -> Result<Response> {
    let (id, path) = rest.split_once('/').unwrap_or((rest, ""));

    let publish: uuid::Uuid = id.parse().map_err(|_| AppError::NotFound("preview"))?;

    let mut conn = state.db.begin().await?;

    let mine: Option<(uuid::Uuid,)> =
        sqlx::query_as("select id from publishes where id = $1 and preview")
            .bind(publish)
            .fetch_optional(conn.conn())
            .await?;

    conn.commit().await?;

    if mine.is_none() {
        return Err(AppError::NotFound("preview"));
    }

    let wanted = file_for(path).ok_or(AppError::NotFound("page"))?;
    let bytes = state.store.get(&at(publish, &wanted)).await?;

    Ok(answered(
        bytes,
        kind_of(&wanted),
        StatusCode::OK,
        publish,
        &wanted,
    ))
}

/// Which build is live. One query, and a site that has never published has
/// nothing to serve rather than an error.
async fn live(state: &AppState) -> Result<uuid::Uuid> {
    let mut conn = state.db.begin().await?;

    let found: Option<(uuid::Uuid,)> = sqlx::query_as(
        "select id from publishes
          where state = 'live'
          order by finished_at desc nulls last
          limit 1",
    )
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    found
        .map(|(id,)| id)
        .ok_or(AppError::NotFound("published site"))
}

/// What a file is, from its name.
///
/// From the name here rather than from the bytes, unlike an upload: what is in
/// a build was written by the site's own theme, not sent by a stranger, and a
/// stylesheet is a stylesheet because the theme called it one.
fn kind_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_its_index_and_a_file_is_itself() {
        assert_eq!(file_for("/").as_deref(), Some("index.html"));
        assert_eq!(file_for("/about").as_deref(), Some("about/index.html"));
        assert_eq!(file_for("/about/").as_deref(), Some("about/index.html"));
        assert_eq!(file_for("/style.css").as_deref(), Some("style.css"));
        assert_eq!(file_for("/assets/app.js").as_deref(), Some("assets/app.js"));
    }

    #[test]
    fn nothing_that_climbs_is_a_page() {
        for asked in [
            "/../../etc/passwd",
            "/assets/../../secrets",
            "/./index.html",
            "/%2e%2e/%2e%2e/etc/passwd",
            "/assets/..%2fsecret",
        ] {
            assert!(file_for(asked).is_none(), "{asked} was taken as a page");
        }
    }

    #[test]
    fn a_build_is_named_by_the_publish_that_made_it() {
        let publish = uuid::Uuid::nil();

        assert_eq!(
            at(publish, "/index.html"),
            "builds/00000000000000000000000000000000/index.html"
        );
    }

    #[test]
    fn what_a_file_is_comes_from_what_the_theme_called_it() {
        assert_eq!(kind_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(kind_of("assets/app.js"), "text/javascript; charset=utf-8");
        assert_eq!(kind_of("something-else"), "application/octet-stream");
    }
}
