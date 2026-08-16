//! Which file answers an address, and what it is.

/// Which file answers this address, where one does.
///
/// A folder is its index: `/about` and `/about/` are the same page, and a
/// design that wrote `about/index.html` answers on both. Anything with a dot
/// in it is asking for a file by name.
///
/// **Nothing that climbs is a page.** The store refuses a path that climbs as
/// well, and this refuses it anyway: what `/../../etc/passwd` should get is
/// the site's own "not here" rather than a different answer saying which guard
/// stopped it — and a store somewhere else may not have the same guard.
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

/// What a browser should be told a file is.
///
/// A list rather than a guess, and everything not on it is bytes. A file whose
/// kind is guessed from its contents is a file a browser can be talked into
/// running: what a design put in `public/` is a site's own, and what it is
/// called is the only thing this decides from.
#[must_use]
pub fn kind_of(path: &str) -> &'static str {
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

/// What sort of answer an address is asking for, where that changes what
/// happens when it is not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A page. Missing, it is worth showing the site's own "not here" — which
    /// is a page.
    Page,
    /// Something a page asked for: a stylesheet, a picture, a font. Missing,
    /// answering with a page of HTML is how a stylesheet becomes a parse error
    /// in somebody's console rather than a four-oh-four.
    Something,
}

impl Kind {
    #[must_use]
    pub fn of(file: &str) -> Self {
        if file.ends_with(".html") {
            Kind::Page
        } else {
            Kind::Something
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_its_index() {
        assert_eq!(file_for("/"), Some("index.html".to_owned()));
        assert_eq!(file_for("/about"), Some("about/index.html".to_owned()));
        assert_eq!(file_for("/about/"), Some("about/index.html".to_owned()));
        assert_eq!(
            file_for("/blog/hello"),
            Some("blog/hello/index.html".to_owned())
        );
    }

    #[test]
    fn something_with_a_dot_in_it_is_asked_for_by_name() {
        assert_eq!(
            file_for("/styles/site.css"),
            Some("styles/site.css".to_owned())
        );
        assert_eq!(file_for("/logo.svg"), Some("logo.svg".to_owned()));
    }

    #[test]
    fn nothing_that_climbs_is_a_page() {
        // Written out because each of these is a different way of writing the
        // same request, and a check that catches three of the four is a check
        // that catches none.
        for wrong in [
            "/../etc/passwd",
            "/about/../../etc/passwd",
            "/%2e%2e/etc/passwd",
            "/%2E%2E/etc/passwd",
            "/about%2f..%2fetc",
            "/about/..\\etc",
            "/.",
        ] {
            assert_eq!(file_for(wrong), None, "{wrong} was taken for a page");
        }
    }

    #[test]
    fn what_a_file_is_comes_from_its_name_and_a_list() {
        assert_eq!(kind_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(kind_of("site.css"), "text/css; charset=utf-8");
        assert_eq!(kind_of("logo.svg"), "image/svg+xml");

        // Everything else is bytes. A kind guessed from what is inside a file
        // is a file a browser can be talked into running.
        assert_eq!(kind_of("something.wat"), "application/octet-stream");
        assert_eq!(kind_of("no-extension"), "application/octet-stream");
    }

    #[test]
    fn a_missing_stylesheet_is_not_a_page() {
        // Answering a missing `.css` with the site's own not-found page is how
        // a stylesheet becomes a parse error in a console instead of a plain
        // four-oh-four.
        assert_eq!(Kind::of("about/index.html"), Kind::Page);
        assert_eq!(Kind::of("styles/site.css"), Kind::Something);
    }
}
