//! What a browser is told, and what is believed when a browser asks.

use axum::http::header::{HeaderName, HeaderValue, ORIGIN};
use axum::http::{HeaderMap, Method, Request};
use axum::middleware::Next;
use axum::response::Response;

use super::error::{AppError, Result};
use super::say;

/// This answers JSON, so nothing here is allowed to load anything, be framed,
/// or be the target of a form. A stricter policy than a page would take, which
/// is the point: none of it is ever wanted.
const POLICY: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; \
                      form-action 'none'; sandbox";

const HEADERS: [(HeaderName, &str); 5] = [
    (HeaderName::from_static("content-security-policy"), POLICY),
    (HeaderName::from_static("x-content-type-options"), "nosniff"),
    (HeaderName::from_static("x-frame-options"), "DENY"),
    (HeaderName::from_static("referrer-policy"), "no-referrer"),
    (
        HeaderName::from_static("cross-origin-resource-policy"),
        "same-origin",
    ),
];

/// A year, and every name under it. Said only where the request arrived over
/// TLS: promising it on a plain connection is how somebody locks themselves
/// out of a machine that has no certificate yet.
const HSTS: &str = "max-age=31536000; includeSubDomains";

pub async fn told(request: Request<axum::body::Body>, next: Next) -> Response {
    let secure = arrived_securely(request.headers());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    for (name, value) in HEADERS {
        // Whatever answered may have said it better — a page served from here
        // needs a policy of its own, and this is the floor rather than the law.
        if let (false, Ok(value)) = (headers.contains_key(&name), HeaderValue::from_str(value)) {
            headers.insert(name, value);
        }
    }

    if secure {
        headers.insert(
            HeaderName::from_static("strict-transport-security"),
            HeaderValue::from_static(HSTS),
        );
    }

    response
}

fn arrived_securely(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .next()
                .is_some_and(|first| first.trim() == "https")
        })
}

/// Whether a change asked for with a cookie was asked for by this site.
///
/// A cookie is sent by the browser whichever page asked, so a form on somebody
/// else's page can make a change here in a signed-in person's name. What tells
/// the two apart is where the request says it came from. A token in a header is
/// not sent by anybody's page, so nothing carrying one is asked.
pub fn asked_by_this_site(method: &Method, headers: &HeaderMap, host: &str) -> Result<()> {
    if !matches!(
        *method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return Ok(());
    }

    // What a browser says about where it came from, where it says anything.
    // Something that is not a browser — a script, a phone, curl — sends none of
    // these, and none of them are what a cross-site request looks like.
    if let Some(site) = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
    {
        return match site {
            "same-origin" | "same-site" | "none" => Ok(()),
            _ => Err(AppError::Refused(
                say::CHANGE_ASKED_FOR_FROM_SOMEWHERE_ELSE.into(),
            )),
        };
    }

    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };

    let named = origin
        .split("//")
        .nth(1)
        .unwrap_or(origin)
        .split('/')
        .next()
        .unwrap_or_default();

    if same_host(named, host) {
        Ok(())
    } else {
        Err(AppError::Refused(
            say::CHANGE_ASKED_FOR_FROM_SOMEWHERE_ELSE.into(),
        ))
    }
}

/// The same name, whatever port either of them named it on.
fn same_host(one: &str, two: &str) -> bool {
    let bare = |host: &str| {
        host.rsplit_once(':')
            // An address with two colons in it is IPv6, which keeps its colons.
            .filter(|(before, _)| !before.contains(':'))
            .map_or_else(
                || host.to_ascii_lowercase(),
                |(before, _)| before.to_ascii_lowercase(),
            )
    };

    bare(one) == bare(two)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saying(name: &str, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
            HeaderValue::from_str(value).expect("a header value"),
        );
        headers
    }

    #[test]
    fn a_form_on_somebody_else_s_page_does_not_change_anything_here() {
        let elsewhere = saying("origin", "https://example.invalid");

        assert!(
            asked_by_this_site(&Method::POST, &elsewhere, "shop.example").is_err(),
            "a page somewhere else made a change in somebody's name"
        );

        let ours = saying("origin", "https://shop.example");

        assert!(asked_by_this_site(&Method::POST, &ours, "shop.example").is_ok());
    }

    #[test]
    fn a_port_is_not_a_different_site() {
        let ours = saying("origin", "http://shop.example:5173");

        assert!(asked_by_this_site(&Method::POST, &ours, "shop.example:8080").is_ok());
    }

    #[test]
    fn reading_is_not_asked_where_it_came_from() {
        let elsewhere = saying("origin", "https://example.invalid");

        assert!(asked_by_this_site(&Method::GET, &elsewhere, "shop.example").is_ok());
    }

    #[test]
    fn what_a_browser_says_about_itself_is_taken_first() {
        let mut headers = saying("sec-fetch-site", "cross-site");
        headers.insert(ORIGIN, HeaderValue::from_static("https://shop.example"));

        assert!(
            asked_by_this_site(&Method::POST, &headers, "shop.example").is_err(),
            "a browser said the request came from another site and was believed anyway"
        );
    }

    #[test]
    fn something_that_is_not_a_browser_is_not_asked() {
        assert!(asked_by_this_site(&Method::POST, &HeaderMap::new(), "shop.example").is_ok());
    }

    #[test]
    fn a_promise_of_https_is_only_made_over_https() {
        assert!(arrived_securely(&saying("x-forwarded-proto", "https")));
        assert!(!arrived_securely(&saying("x-forwarded-proto", "http")));
        assert!(!arrived_securely(&HeaderMap::new()));
    }
}
