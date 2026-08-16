//! A site's own kinds of thing, and asking about what they carry.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    router: axum::Router,
    host: String,
    token: String,
    #[expect(dead_code, reason = "kept so the site outlives the leased database")]
    db: Db,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

        let mut conn = db.begin().await.expect("begin");
        sqlx::query(
            "insert into languages (code, name, is_default)
             values ('en', 'English', true)",
        )
        .execute(conn.conn())
        .await
        .expect("a language");
        conn.commit().await.expect("commit");

        let site = Self {
            router: mavi::router(AppState::new(db.clone())),
            host,
            token: String::new(),
            db,
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
                Some(serde_json::json!({ "email": email, "password": PASSWORD })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

    /// A kind of thing with a cooking time on it, which is the whole reason
    /// anybody declares one.
    async fn a_recipe(&self) {
        let (status, made) = self
            .send(
                "POST",
                "/api/content-types",
                Some(serde_json::json!({
                    "key": "recipe",
                    "name": "Recipe",
                    "fields": [
                        { "name": "minutes", "kind": "number", "required": true },
                        {
                            "name": "course",
                            "kind": "choice",
                            "choices": ["starter", "main"],
                        },
                    ],
                })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{made}");
    }

    async fn a_post(
        &self,
        title: &str,
        fields: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        self.send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({
                "language": "en",
                "title": title,
                "type": "recipe",
                "fields": fields,
            })),
        )
        .await
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if !self.token.is_empty() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        }

        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }
}

#[tokio::test]
async fn a_site_asks_for_every_recipe_under_thirty_minutes() {
    let site = Site::new().await;
    site.a_recipe().await;

    for (title, minutes) in [("Quick soup", 20), ("Slow stew", 180)] {
        let (status, written) = site
            .a_post(title, serde_json::json!({ "minutes": minutes }))
            .await;

        assert_eq!(status, StatusCode::CREATED, "{written}");
    }

    let (status, found) = site
        .send(
            "GET",
            "/api/posts?type=recipe&field=minutes&at_most=30",
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{found}");

    let titles: Vec<&str> = found["items"]
        .as_array()
        .expect("a page")
        .iter()
        .filter_map(|post| post["title"].as_str())
        .collect();

    assert_eq!(titles, vec!["Quick soup"], "{found}");
}

#[tokio::test]
async fn a_field_nothing_declared_is_refused_rather_than_matching_nothing() {
    let site = Site::new().await;
    site.a_recipe().await;

    let (status, refused) = site
        .send(
            "GET",
            "/api/posts?type=recipe&field=mintues&at_most=30",
            None,
        )
        .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a typo in a filter quietly showed the wrong posts: {refused}"
    );
}

#[tokio::test]
async fn asking_about_a_field_means_saying_what_it_belongs_to() {
    let site = Site::new().await;
    site.a_recipe().await;

    let (status, _) = site
        .send("GET", "/api/posts?field=minutes&at_most=30", None)
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn what_the_kind_declares_is_what_can_be_written() {
    let site = Site::new().await;
    site.a_recipe().await;

    let (status, refused) = site
        .a_post("Soup", serde_json::json!({ "minutes": 20, "mintues": 30 }))
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    let (status, refused) = site.a_post("Soup", serde_json::json!({})).await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a required field was left out and the post was written anyway: {refused}"
    );

    let (status, refused) = site
        .a_post("Soup", serde_json::json!({ "minutes": "twenty" }))
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    let (status, refused) = site
        .a_post(
            "Soup",
            serde_json::json!({ "minutes": 20, "course": "pudding" }),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn fields_without_a_kind_of_thing_go_nowhere() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/posts",
            Some(serde_json::json!({
                "language": "en",
                "title": "Something",
                "fields": { "minutes": 20 },
            })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a field was stored under nothing, where nothing can ask about it: {refused}"
    );
}

#[tokio::test]
async fn changing_a_post_is_checked_against_what_it_is() {
    let site = Site::new().await;
    site.a_recipe().await;

    let (_, written) = site
        .a_post("Soup", serde_json::json!({ "minutes": 20 }))
        .await;

    let id = written["id"].as_str().expect("an id").to_owned();

    let (status, refused) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(serde_json::json!({ "fields": { "mintues": 20 } })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(serde_json::json!({ "fields": { "minutes": 25, "course": "main" } })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["fields"]["minutes"], 25);
    assert_eq!(changed["type"], "recipe");
}

/// Not found rather than made anyway with the fields dropped, or refused as
/// though the caller had got the fields wrong: what is missing is the kind of
/// thing itself.
#[tokio::test]
async fn a_kind_of_thing_nobody_declared_is_not_one_to_write_under() {
    let site = Site::new().await;

    let (status, _) = site
        .a_post("Soup", serde_json::json!({ "minutes": 20 }))
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn taking_a_kind_of_thing_away_leaves_what_was_written_under_it() {
    let site = Site::new().await;
    site.a_recipe().await;

    let (_, written) = site
        .a_post("Soup", serde_json::json!({ "minutes": 20 }))
        .await;

    let id = written["id"].as_str().expect("an id").to_owned();

    let (status, _) = site.send("DELETE", "/api/content-types/recipe", None).await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, still_there) = site.send("GET", &format!("/api/posts/{id}"), None).await;

    assert_eq!(status, StatusCode::OK, "{still_there}");
    assert_eq!(
        still_there["post"]["fields"]["minutes"], 20,
        "throwing away a declaration threw away what was written"
    );
}

#[tokio::test]
async fn a_kind_of_thing_can_be_called_something_in_each_language() {
    let site = Site::new().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/content-types",
            Some(serde_json::json!({
                "key": "book",
                "name": "Book",
                "plural": "Books",
                "names": { "tr": { "name": "Kitap", "plural": "Kitaplar" } },
                "fields": [],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["plural"], "Books");
    assert_eq!(made["names"]["tr"]["plural"], "Kitaplar", "{made}");

    let (_, listed) = site.send("GET", "/api/content-types", None).await;

    let book = listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|kind| kind["key"] == "book")
        .expect("the kind that was made");

    assert_eq!(book["names"]["tr"]["name"], "Kitap", "{listed}");
}
