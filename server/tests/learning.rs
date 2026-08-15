//! A student is not a panel account, and the test that matters is the one that
//! tries to be both.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
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
    token: String,
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let router = mavi::router(AppState::new(db.clone()));

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    let body: serde_json::Value = serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes(),
    )
    .expect("json");

    Site {
        db,
        router,
        host,
        tenant,
        token: body["token"].as_str().expect("a token").to_owned(),
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        cookie: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value, Option<String>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
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
        let set = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .map(|value| {
                value
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_owned()
            });

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
            set,
        )
    }

    async fn a_course(&self) -> Uuid {
        let (status, body, _) = self
            .send(
                "POST",
                "/api/courses",
                Some(&self.token),
                None,
                Some(serde_json::json!({
                    "slug": format!("course-{}", Uuid::now_v7().simple()),
                    "title": "Learning Something",
                })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        let id: Uuid = body["id"].as_str().expect("an id").parse().expect("a uuid");

        // Open it, and give it something to learn.
        let mut conn = self.db.tenant(self.tenant).await.expect("begin");

        sqlx::query("update courses set state = 'open' where id = $1")
            .bind(id)
            .execute(conn.conn())
            .await
            .expect("open");

        let module = mavi::learning::add_module(&mut conn, self.tenant, id, "The First", 0)
            .await
            .expect("a module");

        mavi::learning::add_lesson(&mut conn, self.tenant, module, "The First Lesson", 0)
            .await
            .expect("a lesson");

        conn.commit().await.expect("commit");

        id
    }

    async fn a_student(&self, course: Uuid) -> (String, String) {
        let email = format!("learner-{}@example.test", Uuid::now_v7().simple());

        let (status, body, _) = self
            .send(
                "POST",
                &format!("/api/courses/{course}/students"),
                Some(&self.token),
                None,
                Some(serde_json::json!({ "email": email, "name": "A Learner" })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        (email, body["token"].as_str().expect("a token").to_owned())
    }

    async fn signed_in_student(&self, course: Uuid) -> String {
        let (email, password) = self.a_student(course).await;

        let (status, body, cookie) = self
            .send(
                "POST",
                "/api/learn/session",
                None,
                None,
                Some(serde_json::json!({ "email": email, "password": password })),
            )
            .await;

        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

        cookie.expect("a cookie")
    }
}

#[tokio::test]
async fn a_student_sees_what_they_are_on_and_finishes_a_lesson() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    let (status, mine, _) = site
        .send("GET", "/api/learn/courses", None, Some(&cookie), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(mine["items"].as_array().expect("a page").len(), 1);

    let (status, curriculum, _) = site
        .send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{curriculum}");

    let lesson = curriculum["modules"][0]["lessons"][0]["id"]
        .as_str()
        .expect("a lesson")
        .to_owned();

    assert_eq!(curriculum["modules"][0]["lessons"][0]["done"], false);

    let (status, _, _) = site
        .send(
            "POST",
            &format!("/api/learn/lessons/{lesson}/done"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, curriculum, _) = site
        .send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(curriculum["modules"][0]["lessons"][0]["done"], true);
}

#[tokio::test]
async fn a_student_reaches_nothing_in_the_panel() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    for path in ["/api/courses", "/api/people", "/api/posts", "/api/orders"] {
        let (status, _, _) = site.send("GET", path, None, Some(&cookie), None).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "a student reached {path} in the panel"
        );
    }
}

#[tokio::test]
async fn a_panel_account_is_not_a_student() {
    let site = a_site().await;
    site.a_course().await;

    let (status, _, _) = site
        .send("GET", "/api/learn/courses", Some(&site.token), None, None)
        .await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a panel token was taken as a student's"
    );
}

#[tokio::test]
async fn a_course_nobody_put_them_on_is_not_there() {
    let site = a_site().await;
    let theirs = site.a_course().await;
    let somebody_elses = site.a_course().await;
    let cookie = site.signed_in_student(theirs).await;

    let (status, _, _) = site
        .send(
            "GET",
            &format!("/api/learn/courses/{somebody_elses}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_lesson_on_a_course_they_are_not_on_cannot_be_finished() {
    let site = a_site().await;
    let theirs = site.a_course().await;
    let somebody_elses = site.a_course().await;
    let cookie = site.signed_in_student(theirs).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let lesson: (Uuid,) = sqlx::query_as(
        "select l.id from lessons l join modules m on m.id = l.module_id
          where m.course_id = $1",
    )
    .bind(somebody_elses)
    .fetch_one(conn.conn())
    .await
    .expect("a lesson");

    let (status, _, _) = site
        .send(
            "POST",
            &format!("/api/learn/lessons/{}/done", lesson.0),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The curriculum is what a student loads on every page, so what it costs is
/// the thing to watch: a lesson per query is how a course with forty of them
/// gets slow.
#[tokio::test]
async fn a_curriculum_costs_the_same_however_many_lessons_there_are() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    site.send(
        "GET",
        &format!("/api/learn/courses/{course}"),
        None,
        Some(&cookie),
        None,
    )
    .await;

    let small = {
        let (counter, _guard) = common::queries::counting();
        site.send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;
        counter.count()
    };

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let module: (Uuid,) = sqlx::query_as("select id from modules where course_id = $1")
        .bind(course)
        .fetch_one(conn.conn())
        .await
        .expect("a module");

    for position in 1..20 {
        mavi::learning::add_lesson(&mut conn, site.tenant, module.0, "Another Lesson", position)
            .await
            .expect("a lesson");
    }

    conn.commit().await.expect("commit");

    site.send(
        "GET",
        &format!("/api/learn/courses/{course}"),
        None,
        Some(&cookie),
        None,
    )
    .await;

    let large = {
        let (counter, _guard) = common::queries::counting();
        site.send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;
        counter.count()
    };

    // Equal, not bounded: the request above warms the route, and
    // `common::queries::counting` no longer counts a connection's one-time
    // cost of learning what `course_state` is (see `SETTING_UP` in
    // `common/queries.rs`) — that cost belongs to whichever connection pays
    // it first, not to whichever read this test happens to measure.
    assert_eq!(
        small, large,
        "a course with twenty lessons cost more to read"
    );
}

#[tokio::test]
async fn everybody_a_site_teaches_is_one_list() {
    let site = a_site().await;
    let course = site.a_course().await;
    let (email, _) = site.a_student(course).await;

    let (status, listed, _) = site
        .send("GET", "/api/students", Some(&site.token), None, None)
        .await;

    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(listed["items"][0]["email"], email);
    assert_eq!(
        listed["items"][0]["courses"], 1,
        "the list does not say what they are on: {listed}"
    );
}

#[tokio::test]
async fn stopping_somebody_leaves_what_they_finished() {
    let site = a_site().await;
    let course = site.a_course().await;
    site.a_student(course).await;

    let (_, listed, _) = site
        .send("GET", "/api/students", Some(&site.token), None, None)
        .await;

    let id = listed["items"][0]["id"].as_str().expect("an id").to_owned();

    let (status, stopped, _) = site
        .send(
            "PATCH",
            &format!("/api/students/{id}"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "suspended": true })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{stopped}");
    assert_eq!(stopped["state"], "suspended");
    assert_eq!(
        stopped["courses"], 1,
        "suspending somebody took away what they were on"
    );

    let (_, back, _) = site
        .send(
            "PATCH",
            &format!("/api/students/{id}"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "suspended": false })),
        )
        .await;

    assert_eq!(back["state"], "active");
}

#[tokio::test]
async fn access_that_was_sold_for_a_while_stops_when_it_is_over() {
    let site = a_site().await;
    let course = site.a_course().await;

    let email = format!("learner-{}@example.test", Uuid::now_v7().simple());

    let (status, enrolled, _) = site
        .send(
            "POST",
            &format!("/api/courses/{course}/students"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "email": email, "name": "A Learner", "days": 30 })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{enrolled}");

    let password = enrolled["token"].as_str().expect("a token").to_owned();
    let student = enrolled["student_id"].as_str().expect("an id").to_owned();

    let (_, listed, _) = site
        .send(
            "GET",
            &format!("/api/students/{student}/enrolments"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    assert_eq!(listed[0]["state"], "open", "{listed}");
    assert!(listed[0]["ends_at"].is_string(), "{listed}");

    let enrolment = listed[0]["id"].as_str().expect("an id").to_owned();

    // The day it runs out, without waiting a month for it.
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query("update enrolments set ends_at = now() - interval '1 day' where id = $1")
        .bind(enrolment.parse::<Uuid>().expect("a uuid"))
        .execute(conn.conn())
        .await
        .expect("an end");

    conn.commit().await.expect("commit");

    let (_, _, cookie) = site
        .send(
            "POST",
            "/api/learn/session",
            None,
            None,
            Some(serde_json::json!({ "email": email, "password": password })),
        )
        .await;

    let cookie = cookie.expect("a cookie");

    let (status, mine, _) = site
        .send("GET", "/api/learn/courses", None, Some(&cookie), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{mine}");
    assert_eq!(
        mine["items"].as_array().expect("a page").len(),
        0,
        "access that had run out still opened the course: {mine}"
    );

    let (status, refused, _) = site
        .send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
}

#[tokio::test]
async fn access_can_be_given_longer_and_taken_back() {
    let site = a_site().await;
    let course = site.a_course().await;
    let (_, _) = site.a_student(course).await;

    let (_, students, _) = site
        .send("GET", "/api/students", Some(&site.token), None, None)
        .await;

    let student = students["items"][0]["id"]
        .as_str()
        .expect("an id")
        .to_owned();

    let (_, listed, _) = site
        .send(
            "GET",
            &format!("/api/students/{student}/enrolments"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    let enrolment = listed[0]["id"].as_str().expect("an id").to_owned();

    let (status, longer, _) = site
        .send(
            "PATCH",
            &format!("/api/enrolments/{enrolment}"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "days": 90 })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{longer}");
    assert!(longer["ends_at"].is_string(), "{longer}");

    let (_, forever, _) = site
        .send(
            "PATCH",
            &format!("/api/enrolments/{enrolment}"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "forever": true })),
        )
        .await;

    assert!(forever["ends_at"].is_null(), "{forever}");

    let (status, _, _) = site
        .send(
            "DELETE",
            &format!("/api/enrolments/{enrolment}"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, after, _) = site
        .send(
            "GET",
            &format!("/api/students/{student}/enrolments"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    assert_eq!(after.as_array().expect("a list").len(), 0);
}

#[tokio::test]
async fn a_curriculum_can_be_built_through_the_api() {
    let site = a_site().await;

    let (status, made, _) = site
        .send(
            "POST",
            "/api/courses",
            Some(&site.token),
            None,
            Some(serde_json::json!({
                "slug": format!("built-{}", Uuid::now_v7().simple()),
                "title": "Built Through The API",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");

    let course = made["id"].as_str().expect("an id").to_owned();

    let (status, module, _) = site
        .send(
            "POST",
            &format!("/api/courses/{course}/modules"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "title": "The First Part" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{module}");
    assert_eq!(module["position"], 0, "the first one goes first");

    let module_id = module["id"].as_str().expect("an id").to_owned();

    for title in ["One", "Two"] {
        let (status, lesson, _) = site
            .send(
                "POST",
                &format!("/api/modules/{module_id}/lessons"),
                Some(&site.token),
                None,
                Some(serde_json::json!({ "title": title })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{lesson}");
    }

    let (status, whole, _) = site
        .send(
            "GET",
            &format!("/api/courses/{course}"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{whole}");
    assert_eq!(
        whole["modules"][0]["lessons"]
            .as_array()
            .expect("lessons")
            .len(),
        2
    );

    // A course being written is not one anybody is on, and opening it is a
    // decision rather than a side effect of adding a lesson.
    assert_eq!(whole["course"]["state"], "draft", "{whole}");

    let (status, opened, _) = site
        .send(
            "PATCH",
            &format!("/api/courses/{course}"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "state": "open" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(opened["state"], "open");
}

#[tokio::test]
async fn two_lessons_cannot_sit_in_one_place() {
    let site = a_site().await;
    let course = site.a_course().await;

    let (_, whole, _) = site
        .send(
            "GET",
            &format!("/api/courses/{course}"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    let module = whole["modules"][0]["id"]
        .as_str()
        .expect("a module")
        .to_owned();

    let (status, refused, _) = site
        .send(
            "POST",
            &format!("/api/modules/{module}/lessons"),
            Some(&site.token),
            None,
            Some(serde_json::json!({ "title": "In the way", "position": 0 })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
}

#[tokio::test]
async fn a_student_reads_a_lesson_and_what_is_either_side_of_it() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    let (_, whole, _) = site
        .send(
            "GET",
            &format!("/api/learn/courses/{course}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    let lesson = whole["modules"][0]["lessons"][0]["id"]
        .as_str()
        .expect("a lesson")
        .to_owned();

    let (status, watching, _) = site
        .send(
            "GET",
            &format!("/api/learn/lessons/{lesson}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{watching}");
    assert_eq!(watching["position"], 1, "{watching}");
    assert_eq!(watching["done"], false);
    assert!(watching["previous"].is_null(), "{watching}");
}

#[tokio::test]
async fn a_lesson_on_a_course_somebody_is_not_on_is_not_there() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    // Another course, which this student is not on.
    let other = site.a_course().await;

    let (_, whole, _) = site
        .send(
            "GET",
            &format!("/api/courses/{other}"),
            Some(&site.token),
            None,
            None,
        )
        .await;

    let lesson = whole["modules"][0]["lessons"][0]["id"]
        .as_str()
        .expect("a lesson")
        .to_owned();

    let (status, refused, _) = site
        .send(
            "GET",
            &format!("/api/learn/lessons/{lesson}"),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND, "{refused}");
}

#[tokio::test]
async fn signing_out_closes_what_was_open() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    let (status, _, _) = site
        .send("DELETE", "/api/learn/session", None, Some(&cookie), None)
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _, _) = site
        .send("GET", "/api/learn/courses", None, Some(&cookie), None)
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_lesson_that_points_at_something_that_is_not_a_video_plays_nothing() {
    let site = a_site().await;
    let course = site.a_course().await;
    let cookie = site.signed_in_student(course).await;

    // A picture, uploaded the way anything is, and a video row pointed at it:
    // what a lesson plays has to be a video whatever a row says.
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let media: (Uuid,) = sqlx::query_as(
        "insert into media (tenant_id, location, mime, bytes, checksum, original_name)
         values ($1, 'nowhere.png', 'image/png', 1, '\\x00'::bytea, 'a.png')
         returning id",
    )
    .bind(site.tenant.0)
    .fetch_one(conn.conn())
    .await
    .expect("a picture");

    let video: (Uuid,) = sqlx::query_as(
        "insert into videos (tenant_id, media_id, title, state)
         values ($1, $2, 'Not a video', 'ready') returning id",
    )
    .bind(site.tenant.0)
    .bind(media.0)
    .fetch_one(conn.conn())
    .await
    .expect("a video row");

    sqlx::query("update lessons set video_id = $1 where tenant_id = $2")
        .bind(video.0)
        .bind(site.tenant.0)
        .execute(conn.conn())
        .await
        .expect("a lesson");

    conn.commit().await.expect("commit");

    let (status, refused, _) = site
        .send(
            "GET",
            &format!("/api/learn/videos/{}", video.0),
            None,
            Some(&cookie),
            None,
        )
        .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "something that is not a video was served to a student: {refused}"
    );
}
