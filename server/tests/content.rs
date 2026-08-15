//! Posts, pages and what they are filed under. The seven, and the two things
//! this domain has that forms does not: a state machine, and a record whose
//! owner decides who may change it.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::{Access, Capability, Needs, every_grant};
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
}

struct Who {
    id: Uuid,
    token: String,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;

        let mut conn = db.tenant(tenant).await.expect("begin");
        sqlx::query(
            "insert into languages (tenant_id, code, name, is_default)
             values ($1, 'en', 'English', true)",
        )
        .bind(tenant.0)
        .execute(conn.conn())
        .await
        .expect("a language");
        conn.commit().await.expect("commit");

        Self {
            router: mavi::router(AppState::new(db.clone())),
            db,
            host,
            tenant,
        }
    }

    async fn somebody(&self, role: &str, grants: &[String]) -> Who {
        let role_id = a_role(&self.db, self.tenant, role, grants).await;
        let password = "a long enough password";
        let (id, email) = a_user(&self.db, self.tenant, role_id, password).await;

        let (status, body) = self
            .send(
                "POST",
                "/api/auth/session",
                None,
                Some(serde_json::json!({ "email": email, "password": password })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Who {
            id,
            token: body["token"].as_str().expect("a token").to_owned(),
        }
    }

    async fn everyone(&self) -> Who {
        self.somebody("owner", &every_grant()).await
    }

    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
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

    async fn a_post(&self, who: &Who, title: &str) -> Uuid {
        let (status, body) = self
            .send(
                "POST",
                "/api/posts",
                Some(&who.token),
                Some(serde_json::json!({ "language": "en", "title": title })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        body["id"].as_str().expect("an id").parse().expect("a uuid")
    }
}

#[tokio::test]
async fn a_post_is_written_read_changed_and_taken_away() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let id = site.a_post(&who, "The First One").await;

    let (status, body) = site
        .send("GET", &format!("/api/posts/{id}"), Some(&who.token), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["post"]["slug"], "the-first-one",
        "a title becomes an address"
    );
    assert_eq!(body["post"]["state"], "draft");

    let (status, body) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "state": "published" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "published");
    assert!(body["published_at"].is_string(), "published and not dated");

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site
        .send("GET", &format!("/api/posts/{id}"), Some(&who.token), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_post_moves_only_the_ways_the_machine_allows() {
    let site = Site::new().await;
    let who = site.everyone().await;
    let id = site.a_post(&who, "Draft").await;

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "state": "archived" })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a draft went straight to archived"
    );

    for next in ["published", "archived", "draft"] {
        let (status, body) = site
            .send(
                "PATCH",
                &format!("/api/posts/{id}"),
                Some(&who.token),
                Some(serde_json::json!({ "state": next })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{next}: {body}");
    }
}

#[tokio::test]
async fn an_author_reaches_their_own_and_no_further() {
    let site = Site::new().await;

    let editor = site.everyone().await;
    let author = site
        .somebody(
            "author",
            &[
                Needs::new(Capability::Content, Access::View).grant(),
                Needs::new(Capability::Content, Access::Write).own_grant(),
            ],
        )
        .await;

    let theirs = site.a_post(&author, "Mine").await;
    let somebody_elses = site.a_post(&editor, "Not Mine").await;

    let (status, body) = site
        .send(
            "PATCH",
            &format!("/api/posts/{theirs}"),
            Some(&author.token),
            Some(serde_json::json!({ "title": "Still Mine" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/posts/{somebody_elses}"),
            Some(&author.token),
            Some(serde_json::json!({ "title": "Mine Now" })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "an author edited somebody else's post"
    );

    assert_ne!(author.id, editor.id);
}

#[tokio::test]
async fn a_changed_address_leaves_the_old_one_pointing_at_it() {
    let site = Site::new().await;
    let who = site.everyone().await;
    let id = site.a_post(&who, "Where It Was").await;

    site.send(
        "PATCH",
        &format!("/api/posts/{id}"),
        Some(&who.token),
        Some(serde_json::json!({ "slug": "where-it-is" })),
    )
    .await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let redirect: (String, String) =
        sqlx::query_as("select was, now_at from redirects where post_id = $1")
            .bind(id)
            .fetch_one(conn.conn())
            .await
            .expect("a redirect");

    assert_eq!(
        redirect,
        ("where-it-was".to_owned(), "where-it-is".to_owned())
    );
}

#[tokio::test]
async fn two_posts_cannot_answer_on_one_address() {
    let site = Site::new().await;
    let who = site.everyone().await;

    site.a_post(&who, "Twice").await;

    let (status, _) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "en", "title": "Twice" })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_language_the_site_does_not_write_in_is_refused() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "fr", "title": "Bonjour" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_category_and_a_tag_are_one_thing_a_post_is_filed_under() {
    let site = Site::new().await;
    let who = site.everyone().await;
    let id = site.a_post(&who, "Filed").await;

    let mut terms = Vec::new();

    for (kind, name) in [("category", "Recipes"), ("tag", "Quick")] {
        let (status, body) = site
            .send(
                "POST",
                "/api/terms",
                Some(&who.token),
                Some(serde_json::json!({
                    "kind": kind, "language": "en", "name": name
                })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");
        terms.push(body["id"].as_str().expect("an id").to_owned());
    }

    let (status, body) = site
        .send(
            "PUT",
            &format!("/api/posts/{id}/terms"),
            Some(&who.token),
            Some(serde_json::json!({ "term_ids": terms })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let attached = body.as_array().expect("a list");
    assert_eq!(attached.len(), 2);
    let mut got: Vec<String> = attached
        .iter()
        .map(|term| term["id"].as_str().expect("an id").to_owned())
        .collect();
    got.sort();
    let mut wanted = terms.clone();
    wanted.sort();
    assert_eq!(got, wanted, "every term filed under the post, and no other");

    // Filing it under one thing rather than two takes the other off, because
    // what a post is filed under is one decision.
    let (status, body) = site
        .send(
            "PUT",
            &format!("/api/posts/{id}/terms"),
            Some(&who.token),
            Some(serde_json::json!({ "term_ids": [terms[0]] })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let attached = body.as_array().expect("a list");
    assert_eq!(attached.len(), 1);
    assert_eq!(attached[0]["id"].as_str(), Some(terms[0].as_str()));

    let (status, body) = site
        .send(
            "GET",
            &format!("/api/posts?term={}", terms[0]),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().expect("a list").len(), 1);
}

#[tokio::test]
async fn a_tag_does_not_sit_under_another() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (_, parent) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({ "kind": "category", "language": "en", "name": "Food" })),
        )
        .await;

    let (status, _) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({
                "kind": "tag", "language": "en", "name": "Quick",
                "parent_id": parent["id"]
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// What #160 was about: a custom field is something to ask a question about,
/// not only something to keep.
#[tokio::test]
async fn a_site_s_own_field_can_be_asked_about() {
    let site = Site::new().await;
    let who = site.everyone().await;

    site.send(
        "POST",
        "/api/content-types",
        Some(&who.token),
        Some(serde_json::json!({
            "key": "recipe",
            "name": "Recipe",
            "fields": [
                { "name": "minutes", "kind": "number" },
                { "name": "vegetarian", "kind": "boolean" },
            ],
        })),
    )
    .await;

    site.send(
        "POST",
        "/api/posts",
        Some(&who.token),
        Some(serde_json::json!({
            "language": "en",
            "title": "A Quick One",
            "type": "recipe",
            "fields": { "minutes": 20, "vegetarian": true }
        })),
    )
    .await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let quick: (i64,) = sqlx::query_as(
        "select count(*) from posts
          where (fields ->> 'minutes')::int < 30 and (fields ->> 'vegetarian')::boolean",
    )
    .fetch_one(conn.conn())
    .await
    .expect("a count");

    assert_eq!(quick.0, 1);
}

#[tokio::test]
async fn a_post_scheduled_for_later_goes_when_later_arrives() {
    let site = Site::new().await;
    let who = site.everyone().await;
    let id = site.a_post(&who, "Later Today").await;

    let (status, body) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({
                "state": "scheduled",
                "publish_at": (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339(),
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "scheduled");

    let state = AppState::new(site.db.clone());

    // Nothing yet: the moment has not arrived.
    let went = mavi::content::publish_due(&state, site.tenant)
        .await
        .expect("a pass");

    assert_eq!(went, 0);

    // The moment arrives.
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");
    sqlx::query("update posts set publish_at = now() - interval '1 minute' where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let went = mavi::content::publish_due(&state, site.tenant)
        .await
        .expect("a pass");

    assert_eq!(went, 1);

    let (_, post) = site
        .send("GET", &format!("/api/posts/{id}"), Some(&who.token), None)
        .await;

    assert_eq!(post["post"]["state"], "published");
    assert!(post["post"]["published_at"].is_string());

    // And whatever was waiting for it heard.
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let announced: (i64,) =
        sqlx::query_as("select count(*) from outbox where event = 'post.published'")
            .fetch_one(conn.conn())
            .await
            .expect("the outbox");

    assert_eq!(announced.0, 1);
}

#[tokio::test]
async fn scheduling_without_a_moment_is_refused() {
    let site = Site::new().await;
    let who = site.everyone().await;
    let id = site.a_post(&who, "When Though").await;

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "state": "scheduled" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({
                "state": "scheduled",
                "publish_at": (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339(),
            })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a post was scheduled for a moment that has passed"
    );
}

#[tokio::test]
async fn a_page_says_what_is_wrong_with_it_before_anybody_reads_it() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (status, post) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "en", "title": "Hi" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{post}");

    let id = post["id"].as_str().expect("an id").to_owned();

    let (status, issues) = site
        .send(
            "GET",
            &format!("/api/posts/{id}/issues"),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK);

    let named: Vec<&str> = issues
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|issue| issue["kind"].as_str())
        .collect();

    assert!(named.contains(&"title.short"), "{issues}");
    assert!(named.contains(&"excerpt.missing"));
    assert!(named.contains(&"body.thin"));

    // Fixing it stops it saying so.
    site.send(
        "PATCH",
        &format!("/api/posts/{id}"),
        Some(&who.token),
        Some(serde_json::json!({
            "title": "A Title That Is About The Right Length",
            "excerpt": "A summary long enough to be worth showing and short enough to show.",
            "body": "x".repeat(600),
        })),
    )
    .await;

    let (_, issues) = site
        .send(
            "GET",
            &format!("/api/posts/{id}/issues"),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(
        issues.as_array().expect("a list").len(),
        0,
        "a page that was fixed still says it is broken: {issues}"
    );
}

#[tokio::test]
async fn what_is_wrong_across_a_site_is_one_question() {
    let site = Site::new().await;
    let who = site.everyone().await;

    for title in ["Hi", "Yo"] {
        site.send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "en", "title": title })),
        )
        .await;
    }

    let (counter, _guard) = common::queries::counting();

    let (status, issues) = site
        .send("GET", "/api/pages/issues", Some(&who.token), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        issues["items"].as_array().expect("a page").len() >= 6,
        "two thin pages should have something to say"
    );

    assert!(
        counter.count() <= 6,
        "asking what is wrong with a site cost {} queries",
        counter.count()
    );
}

#[tokio::test]
async fn many_posts_are_published_in_one_go() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let mut ids = Vec::new();

    for which in 0..3 {
        let (_, made) = site
            .send(
                "POST",
                "/api/posts",
                Some(&who.token),
                Some(serde_json::json!({
                    "language": "en",
                    "title": format!("The {which} Of Them"),
                })),
            )
            .await;

        ids.push(made["id"].as_str().expect("an id").to_owned());
    }

    let (status, acted) = site
        .send(
            "POST",
            "/api/posts/actions",
            Some(&who.token),
            Some(serde_json::json!({ "act": "publish", "ids": ids })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(acted["acted_on"], 3);

    // Again: they are already published, so nothing happens twice.
    let (_, again) = site
        .send(
            "POST",
            "/api/posts/actions",
            Some(&who.token),
            Some(serde_json::json!({ "act": "publish", "ids": ids })),
        )
        .await;

    assert_eq!(again["acted_on"], 0, "publishing twice published again");
}

#[tokio::test]
async fn a_batch_bigger_than_the_limit_is_refused_rather_than_cut_short() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let ids: Vec<String> = (0..201).map(|_| Uuid::now_v7().to_string()).collect();

    let (status, refused) = site
        .send(
            "POST",
            "/api/posts/actions",
            Some(&who.token),
            Some(serde_json::json!({ "act": "trash", "ids": ids })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["named"]["most"], "200");
}

#[tokio::test]
async fn a_post_this_site_does_not_have_is_left_alone_and_said_so() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let nobody_s = Uuid::now_v7().to_string();

    let (status, acted) = site
        .send(
            "POST",
            "/api/posts/actions",
            Some(&who.token),
            Some(serde_json::json!({ "act": "trash", "ids": [nobody_s] })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(acted["acted_on"], 0);
    assert_eq!(acted["left_alone"][0], nobody_s, "{acted}");
}

#[tokio::test]
async fn a_batch_does_not_reach_past_what_one_person_may_change() {
    let site = Site::new().await;

    let editor = site.everyone().await;
    let author = site
        .somebody(
            "author",
            &[
                Needs::new(Capability::Content, Access::View).grant(),
                Needs::new(Capability::Content, Access::Write).own_grant(),
            ],
        )
        .await;

    let theirs = site.a_post(&author, "Mine").await;
    let somebody_elses = site.a_post(&editor, "Not Mine").await;

    let (status, acted) = site
        .send(
            "POST",
            "/api/posts/actions",
            Some(&author.token),
            Some(serde_json::json!({
                "act": "publish",
                "ids": [theirs.to_string(), somebody_elses.to_string()],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{acted}");
    assert_eq!(acted["acted_on"], 1, "{acted}");
    assert_eq!(
        acted["left_alone"][0],
        somebody_elses.to_string(),
        "a batch reached a post its caller may not change: {acted}"
    );
}

#[tokio::test]
async fn a_category_can_be_renamed_without_moving_its_address() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({
                "kind": "category",
                "language": "en",
                "name": "Wrting",
            })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();
    let slug = made["slug"].as_str().expect("a slug").to_owned();

    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/terms/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "name": "Writing" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["name"], "Writing");
    assert_eq!(
        changed["slug"], slug,
        "renaming moved the address people have linked to"
    );
}

#[tokio::test]
async fn nothing_sits_under_itself() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({
                "kind": "category",
                "language": "en",
                "name": "Everything",
            })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();

    let (status, refused) = site
        .send(
            "PATCH",
            &format!("/api/terms/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "parent_id": id })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_tag_still_does_not_sit_under_another_when_it_is_changed() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (_, one) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({
                "kind": "tag", "language": "en", "name": "Rust",
            })),
        )
        .await;

    let (_, other) = site
        .send(
            "POST",
            "/api/terms",
            Some(&who.token),
            Some(serde_json::json!({
                "kind": "tag", "language": "en", "name": "Postgres",
            })),
        )
        .await;

    let id = one["id"].as_str().expect("an id");

    let (status, refused) = site
        .send(
            "PATCH",
            &format!("/api/terms/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "parent_id": other["id"] })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn how_many_there_are_is_counted_by_the_database() {
    let site = Site::new().await;
    let who = site.everyone().await;

    for which in 0..3 {
        site.send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({
                "language": "en",
                "title": format!("Number {which}"),
            })),
        )
        .await;
    }

    let (status, counted) = site
        .send(
            "GET",
            "/api/posts/counts?language=en&kind=post",
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{counted}");
    assert_eq!(counted["draft"], 3, "{counted}");
    assert_eq!(counted["published"], 0, "{counted}");

    // Another language is another count, not this one.
    let (_, elsewhere) = site
        .send(
            "GET",
            "/api/posts/counts?language=tr",
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(elsewhere["draft"], 0, "{elsewhere}");
}

#[tokio::test]
async fn a_post_says_how_it_should_appear_elsewhere() {
    let site = Site::new().await;
    let who = site.everyone().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({
                "language": "en",
                "title": "What We Do",
                "seo_title": "What we do — and who we do it for",
                "seo_description": "A sentence a search engine shows.",
                "canonical": "https://example.test/what-we-do",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["seo_title"], "What we do — and who we do it for");
    assert_eq!(made["canonical"], "https://example.test/what-we-do");

    let id = made["id"].as_str().expect("an id").to_owned();

    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/posts/{id}"),
            Some(&who.token),
            Some(serde_json::json!({ "seo_description": "A better sentence." })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["seo_description"], "A better sentence.");
    assert_eq!(
        changed["seo_title"], "What we do — and who we do it for",
        "changing one of them cleared the others"
    );
}

#[tokio::test]
async fn the_same_writing_in_two_languages_is_one_group() {
    let site = Site::new().await;
    let who = site.everyone().await;

    site.send(
        "POST",
        "/api/languages",
        Some(&who.token),
        Some(serde_json::json!({ "code": "tr", "name": "Türkçe" })),
    )
    .await;

    let (_, english) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "en", "title": "About us" })),
        )
        .await;

    let original = english["id"].as_str().expect("an id").to_owned();

    let (status, turkish) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({
                "language": "tr",
                "title": "Hakkımızda",
                "translation_of": original,
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{turkish}");

    // Asked from either end, the other one is there.
    let (_, read) = site
        .send(
            "GET",
            &format!("/api/posts/{original}"),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(read["translations"][0]["language"], "tr", "{read}");

    let other = turkish["id"].as_str().expect("an id");

    let (_, back) = site
        .send(
            "GET",
            &format!("/api/posts/{other}"),
            Some(&who.token),
            None,
        )
        .await;

    assert_eq!(back["translations"][0]["language"], "en", "{back}");
}

#[tokio::test]
async fn one_group_holds_one_post_per_language() {
    let site = Site::new().await;
    let who = site.everyone().await;

    site.send(
        "POST",
        "/api/languages",
        Some(&who.token),
        Some(serde_json::json!({ "code": "tr", "name": "Türkçe" })),
    )
    .await;

    let (_, english) = site
        .send(
            "POST",
            "/api/posts",
            Some(&who.token),
            Some(serde_json::json!({ "language": "en", "title": "Prices" })),
        )
        .await;

    let original = english["id"].as_str().expect("an id").to_owned();

    for title in ["Fiyatlar", "Ücretler"] {
        let (status, written) = site
            .send(
                "POST",
                "/api/posts",
                Some(&who.token),
                Some(serde_json::json!({
                    "language": "tr",
                    "title": title,
                    "translation_of": original,
                })),
            )
            .await;

        if title == "Ücretler" {
            assert_eq!(
                status,
                StatusCode::CONFLICT,
                "one group was given two Turkish versions: {written}"
            );
        }
    }
}
