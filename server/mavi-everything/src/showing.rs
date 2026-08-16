//! Answering a visitor.
//!
//! Everything that is not the API, on a site's own address. The rules are
//! [`mavi_edge`]'s and are tested without any of this; what is here is where
//! the bytes come from and what a browser is told.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use mavi_core::ports::Files;
use mavi_db::Db;
use mavi_edge::page::Kind;
use std::sync::Arc;
use uuid::Uuid;

/// What the edge needs to answer anything.
#[derive(Clone)]
pub struct Site {
    pub db: Db,
    pub files: Arc<dyn Files>,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Site")
    }
}

/// Anything that is not the API.
pub async fn serve(State(site): State<Site>, request: Request<Body>) -> Response {
    let path = request.uri().path().to_owned();

    // Under `/api`, the API's own shape holds all the way down: a client that
    // mistypes a path gets the refusal every other refusal looks like, not a
    // site's not-found page. One fallback, and this is the line that keeps the
    // two halves from answering for each other.
    if path.starts_with("/api/") {
        return mavi_serve::refusal::nothing_answers_there();
    }

    // Asked once. Everything below either serves it or has to say what a site
    // shows when something is not there, and two reads of it in one request
    // could disagree across a publish.
    //
    // A database that is not answering is not a site with no pages. Four-oh-four
    // is a fact about the site that caches and search engines act on, and
    // saying it because a connection failed is how an outage becomes a site
    // that has been deindexed.
    let Ok(live) = published(&site).await else {
        return (StatusCode::SERVICE_UNAVAILABLE, "not answering just now").into_response();
    };

    // Somebody looking at a build. Under the build's own id, which nothing
    // links to — so this comes before what is published, and works for a
    // design nobody has put out yet.
    if let Some((build, rest)) = mavi_edge::looking(&path) {
        return match file(&site, build, &rest).await {
            Some(answer) => answer,
            None => nothing_here(&site, live).await,
        };
    }

    let Some(live) = live else {
        // Nothing has been published, so there is no site yet — which is what
        // an installation looks like on its first day, and is not an error.
        return (StatusCode::NOT_FOUND, "nothing is published yet").into_response();
    };

    if let Some(answer) = file(&site, live, &path).await {
        return answer;
    }

    // No page there. Before saying so: a page that was renamed keeps its old
    // address working, which is the other half of writing the row.
    if let Some(went) = moved(&site, &path).await {
        return went;
    }

    // A missing stylesheet is not a missing page. Answering one with a page of
    // HTML is how a stylesheet becomes a parse error in somebody's console
    // instead of a plain four-oh-four.
    match mavi_edge::file_for(&path).map(|file| Kind::of(&file)) {
        Some(Kind::Something) => (StatusCode::NOT_FOUND, "not found").into_response(),
        _ => nothing_here(&site, Some(live)).await,
    }
}

/// Which set of changes is live, where the database says.
async fn published(site: &Site) -> mavi_core::error::Result<Option<Uuid>> {
    let mut tx = site.db.begin().await?;

    mavi_design::store::published(&mut tx).await
}

/// One file out of one build, if it is there.
async fn file(site: &Site, build: Uuid, path: &str) -> Option<Response> {
    let wanted = mavi_edge::file_for(path)?;
    let bytes = site.files.get(&mavi_edge::at(build, &wanted)).await.ok()?;

    Some(answered(
        bytes,
        mavi_edge::kind_of(&wanted),
        StatusCode::OK,
        build,
    ))
}

/// Where a page went, where it went anywhere.
async fn moved(site: &Site, path: &str) -> Option<Response> {
    let slug = mavi_edge::slug_of(path)?;

    let mut tx = site.db.begin().await.ok()?;
    let answers = mavi_content::store::moved(&mut tx, slug).await.ok()?;

    let went = mavi_edge::went(path, &answers)?;

    let location = HeaderValue::from_str(&went.to).ok()?;
    let mut response = StatusCode::MOVED_PERMANENTLY.into_response();
    response.headers_mut().insert(header::LOCATION, location);

    Some(response)
}

/// The page a site says to show for something that is not there — and failing
/// that, the plainest answer rather than nothing.
async fn nothing_here(site: &Site, live: Option<Uuid>) -> Response {
    let Some(live) = live else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };

    match site.files.get(&mavi_edge::at(live, "404.html")).await {
        Ok(bytes) => answered(
            bytes,
            "text/html; charset=utf-8",
            StatusCode::NOT_FOUND,
            live,
        ),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn answered(bytes: Vec<u8>, kind: &'static str, status: StatusCode, build: Uuid) -> Response {
    // The build and the size of what is going back. The build alone would tag
    // every file in it the same; the size alone would tag two builds of one
    // unchanged file the same, which is right for a cache and wrong for
    // telling them apart.
    let tag = format!("\"{}-{}\"", build.simple(), bytes.len());

    let mut response = (status, bytes).into_response();
    let headers = response.headers_mut();

    if let Ok(value) = HeaderValue::from_str(kind) {
        headers.insert(header::CONTENT_TYPE, value);
    }

    if let Ok(value) = HeaderValue::from_str(&tag) {
        headers.insert(header::ETAG, value);
    }

    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(mavi_edge::HELD_FOR),
    );

    // What a build produced is a site's own, and nothing here decides what it
    // may do. A policy this machine puts on its own answers would break a page
    // that loads its own stylesheet.
    headers.remove(header::CONTENT_SECURITY_POLICY);

    response
}
