//! What may be uploaded is decided by the bytes, and what is served is served
//! in a way a browser will not run.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::storage::{LocalDisk, Store};
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

/// The smallest real PNG there is: one pixel, and a header that says so.
const A_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

struct Site {
    router: axum::Router,
    host: String,
    token: String,
    kept_in: std::path::PathBuf,
    db: Db,
    tenant: mavi::kernel::tenant::TenantId,
}

async fn a_site() -> Site {
    let db: Db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let kept_in = std::env::temp_dir().join(format!("mavi-uploads-{}", Uuid::now_v7().simple()));

    let mut state = AppState::new(db.clone());
    state.store = std::sync::Arc::new(Store::Disk(LocalDisk::at(&kept_in)));

    let router = mavi::router(state);

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
        router,
        host,
        token: body["token"].as_str().expect("a token").to_owned(),
        kept_in,
        db,
        tenant,
    }
}

impl Site {
    async fn upload(&self, name: &str, bytes: &'static [u8]) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method("POST")
            .uri(format!("/api/media?name={name}"))
            .header(header::HOST, &self.host)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .body(Body::from(bytes))
            .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (
            status,
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn fetch(&self, path: &str) -> axum::http::Response<Body> {
        self.router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(header::HOST, &self.host)
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response")
    }
}

#[tokio::test]
async fn a_picture_goes_up_and_comes_back() {
    let site = a_site().await;

    let (status, body) = site.upload("holiday.png", A_PNG).await;

    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["mime"], "image/png");
    assert_eq!(body["original_name"], "holiday.png");

    let id = body["id"].as_str().expect("an id");
    let response = site.fetch(&format!("/uploads/{id}")).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/png");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();

    assert_eq!(bytes.as_ref(), A_PNG, "what came back is not what went up");

    let _ = std::fs::remove_dir_all(&site.kept_in);
}

#[tokio::test]
async fn what_a_browser_would_run_does_not_get_in() {
    let site = a_site().await;

    for (name, bytes) in [
        (
            "drawing.svg",
            &b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"[..],
        ),
        (
            "page.html",
            &b"<!doctype html><script>alert(1)</script>"[..],
        ),
        // Named as a picture and not one, which is the whole point of reading
        // the bytes instead of the name.
        (
            "holiday.png",
            &b"<!doctype html><script>alert(1)</script>"[..],
        ),
    ] {
        let leaked: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());
        let (status, _) = site.upload(name, leaked).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{name} was taken");
    }

    let _ = std::fs::remove_dir_all(&site.kept_in);
}

#[tokio::test]
async fn a_file_is_never_kept_under_the_name_somebody_chose() {
    let site = a_site().await;

    let (_, body) = site.upload("../../escape.png", A_PNG).await;
    let id = body["id"].as_str().expect("an id");

    let kept: Vec<_> = std::fs::read_dir(
        site.kept_in.join(
            // One folder per site, and the file inside it named after its id.
            std::fs::read_dir(&site.kept_in)
                .expect("the folder")
                .next()
                .expect("a site's folder")
                .expect("a folder")
                .file_name(),
        ),
    )
    .expect("the site's folder")
    .filter_map(std::result::Result::ok)
    .map(|entry| entry.file_name().to_string_lossy().into_owned())
    .collect();

    assert_eq!(kept.len(), 1);
    assert!(
        kept[0].starts_with(&id.replace('-', "")),
        "kept as {:?} rather than under its id",
        kept[0]
    );

    let _ = std::fs::remove_dir_all(&site.kept_in);
}

#[tokio::test]
async fn a_file_that_was_taken_away_is_not_served() {
    let site = a_site().await;
    let (_, body) = site.upload("holiday.png", A_PNG).await;
    let id = body["id"].as_str().expect("an id").to_owned();

    let response = site
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/media/{id}"))
                .header(header::HOST, &site.host)
                .header(header::AUTHORIZATION, format!("Bearer {}", site.token))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let response = site.fetch(&format!("/uploads/{id}")).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&site.kept_in);
}

#[tokio::test]
async fn another_site_s_file_is_not_served_here() {
    let one = a_site().await;
    let two = a_site().await;

    let (_, body) = one.upload("holiday.png", A_PNG).await;
    let id = body["id"].as_str().expect("an id");

    let response = two.fetch(&format!("/uploads/{id}")).await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let _ = std::fs::remove_dir_all(&one.kept_in);
    let _ = std::fs::remove_dir_all(&two.kept_in);
}

#[tokio::test]
async fn a_site_cannot_fill_the_disk_one_legal_upload_at_a_time() {
    let site = a_site().await;

    // What this site has room for, said by the operator: ten bytes, which the
    // smallest real picture is already past.
    let mut conn = site.db.operator().await.expect("begin");

    // What an operator does, said out loud: `site_settings` belongs to the
    // control plane and a site's own connection sees only its own row.
    conn.across_sites().await.expect("across sites");

    sqlx::query(
        "insert into site_settings (tenant_id, name, storage_limit_bytes)
         values ($1, 'A site', 10)
         on conflict (tenant_id) do update set storage_limit_bytes = 10",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a limit");

    conn.commit().await.expect("commit");

    let (status, refused) = site.upload("a-picture.png", A_PNG).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a site with no room left took another file: {refused}"
    );
    assert_eq!(refused["error"]["key"], "that_site_has_no_room_left");
}
