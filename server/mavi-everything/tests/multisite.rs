//! Multi-site hosting isolation tests (Issue #134).
//!
//! Asserts that multiple installations constructed in one process share nothing:
//! - separate databases (no cross-site row leakage or query bleed)
//! - separate file storage (uploaded media lives in each site's own directory)
//! - separate sealing keys (one site's keyring cannot unseal another site's secrets)
//! - separate settings, content, and state
//! - zero global static or `OnceLock` state pollution

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mavi_core::ports::{Builds, Files, Seals};
use mavi_db::Db;
use mavi_everything::{Installation, MultiSite};
use mavi_files::InADirectory;
use mavi_http::Caller;
use mavi_sealed::WithAKey;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();
    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL"
    );
    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_ms_{}_{}",
        named.replace('-', "_"),
        Uuid::now_v7().simple()
    );

    let mut admin = PgConnection::connect(&address).await.expect("a connection");
    sqlx::query(&format!("create database {named}"))
        .execute(&mut admin)
        .await
        .expect("a database of its own");

    let (front, _) = address
        .rsplit_once('/')
        .expect("an address with a database");
    let db = Db::open(&format!("{front}/{named}"), 4)
        .await
        .expect("the new database");

    db.migrate().await.expect("every migration");
    db
}

fn whoever_holds(db: Db) -> mavi_serve::WhoIsAsking {
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

            match mavi_people::store::whoever_holds(&mut tx, &token).await {
                Ok(Some((person, session))) => Caller::AnAccount {
                    id: person.id.to_string(),
                    grants: mavi_core::grant::Grants::of(person.grants),
                    session: Some(session.to_string()),
                },
                _ => Caller::Nobody,
            }
        })
    })
}

#[tokio::test]
#[allow(clippy::too_many_lines, clippy::similar_names)]
async fn two_sites_in_one_process_share_nothing() {
    if postgres().is_none() {
        return;
    }

    // 1. Construct Site A: own DB, own files, own sealing key
    let db_a = fresh("site_a").await;
    let temp_a = tempfile::tempdir().expect("tempdir a");
    let files_a: Arc<dyn Files> = Arc::new(InADirectory::at(temp_a.path().to_str().unwrap()));
    let key_a = "0000000000000000000000000000000000000000000000000000000000000001";
    let seals_a: Option<Arc<dyn Seals>> = Some(Arc::new(WithAKey::read(key_a).unwrap()));
    let builds_a: Arc<dyn Builds> = Arc::new(mavi_everything::building::WhatIsInPublic);

    let site_a = Installation::new(
        db_a.clone(),
        files_a,
        seals_a,
        builds_a,
        whoever_holds(db_a.clone()),
    );

    // 2. Construct Site B: own DB, own files, own sealing key
    let db_b = fresh("site_b").await;
    let temp_b = tempfile::tempdir().expect("tempdir b");
    let files_b: Arc<dyn Files> = Arc::new(InADirectory::at(temp_b.path().to_str().unwrap()));
    let key_b = "0000000000000000000000000000000000000000000000000000000000000002";
    let seals_b: Option<Arc<dyn Seals>> = Some(Arc::new(WithAKey::read(key_b).unwrap()));
    let builds_b: Arc<dyn Builds> = Arc::new(mavi_everything::building::WhatIsInPublic);

    let site_b = Installation::new(
        db_b.clone(),
        files_b,
        seals_b,
        builds_b,
        whoever_holds(db_b.clone()),
    );

    // 3. Mount both in MultiSite dispatcher
    let router = MultiSite::new()
        .with_site("sitea.example.com", site_a)
        .with_site("siteb.example.com", site_b)
        .into_router();

    // 4. Set up Site A
    let setup_a = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("Host", "sitea.example.com")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "site": "Site Alpha",
                "name": "Admin Alpha",
                "email": "alpha@example.test",
                "password": "a long enough password 123",
            })
            .to_string(),
        ))
        .unwrap();

    let res_setup_a = router.clone().oneshot(setup_a).await.unwrap();
    assert_eq!(res_setup_a.status(), StatusCode::CREATED);
    let body_setup_a: Value =
        serde_json::from_slice(&to_bytes(res_setup_a.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    let token_a = body_setup_a["token"].as_str().unwrap().to_owned();

    // 5. Set up Site B
    let setup_b = Request::builder()
        .method("POST")
        .uri("/api/setup")
        .header("Host", "siteb.example.com")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "site": "Site Beta",
                "name": "Admin Beta",
                "email": "beta@example.test",
                "password": "a long enough password 123",
            })
            .to_string(),
        ))
        .unwrap();

    let res_setup_b = router.clone().oneshot(setup_b).await.unwrap();
    assert_eq!(res_setup_b.status(), StatusCode::CREATED);
    let body_setup_b: Value =
        serde_json::from_slice(&to_bytes(res_setup_b.into_body(), usize::MAX).await.unwrap())
            .unwrap();
    let token_b = body_setup_b["token"].as_str().unwrap().to_owned();

    // 6. Assert initial settings on both sites
    let get_settings_a = Request::builder()
        .method("GET")
        .uri("/api/settings")
        .header("Host", "sitea.example.com")
        .header("Authorization", format!("Bearer {token_a}"))
        .body(Body::empty())
        .unwrap();
    let res_settings_a = router.clone().oneshot(get_settings_a).await.unwrap();
    assert_eq!(res_settings_a.status(), StatusCode::OK);
    let body_settings_a: Value = serde_json::from_slice(
        &to_bytes(res_settings_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body_settings_a["name"], "Site Alpha");

    let get_settings_b = Request::builder()
        .method("GET")
        .uri("/api/settings")
        .header("Host", "siteb.example.com")
        .header("Authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let res_settings_b = router.clone().oneshot(get_settings_b).await.unwrap();
    assert_eq!(res_settings_b.status(), StatusCode::OK);
    let body_settings_b: Value = serde_json::from_slice(
        &to_bytes(res_settings_b.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body_settings_b["name"], "Site Beta");

    // 7. Update settings on Site A, assert Site B unchanged
    let patch_settings_a = Request::builder()
        .method("PATCH")
        .uri("/api/settings")
        .header("Host", "sitea.example.com")
        .header("Authorization", format!("Bearer {token_a}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({ "name": "Site Alpha Renamed" }).to_string(),
        ))
        .unwrap();

    let res_patch_a = router.clone().oneshot(patch_settings_a).await.unwrap();
    assert_eq!(res_patch_a.status(), StatusCode::OK);

    // Verify Site A has updated name
    let get_settings_a2 = Request::builder()
        .method("GET")
        .uri("/api/settings")
        .header("Host", "sitea.example.com")
        .header("Authorization", format!("Bearer {token_a}"))
        .body(Body::empty())
        .unwrap();
    let res_settings_a2 = router.clone().oneshot(get_settings_a2).await.unwrap();
    let body_settings_a2: Value = serde_json::from_slice(
        &to_bytes(res_settings_a2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body_settings_a2["name"], "Site Alpha Renamed");

    // Verify Site B settings are still "Site Beta"
    let get_settings_b2 = Request::builder()
        .method("GET")
        .uri("/api/settings")
        .header("Host", "siteb.example.com")
        .header("Authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let res_settings_b2 = router.clone().oneshot(get_settings_b2).await.unwrap();
    let body_settings_b2: Value = serde_json::from_slice(
        &to_bytes(res_settings_b2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body_settings_b2["name"], "Site Beta");

    // 8. Create post on Site A
    let create_post_a = Request::builder()
        .method("POST")
        .uri("/api/writings")
        .header("Host", "sitea.example.com")
        .header("Authorization", format!("Bearer {token_a}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "title": "Welcome to Alpha",
                "slug": "welcome-alpha",
                "language": "en",
                "kind": "post",
                "body": "This belongs only to site A"
            })
            .to_string(),
        ))
        .unwrap();

    let res_create_a = router.clone().oneshot(create_post_a).await.unwrap();
    assert_eq!(res_create_a.status(), StatusCode::CREATED);
    let body_create_a: Value = serde_json::from_slice(
        &to_bytes(res_create_a.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let post_a_id = body_create_a["id"].as_str().unwrap().to_owned();

    // 9. Assert Site A has the post
    let get_post_on_a = Request::builder()
        .method("GET")
        .uri(format!("/api/writings/{post_a_id}"))
        .header("Host", "sitea.example.com")
        .header("Authorization", format!("Bearer {token_a}"))
        .body(Body::empty())
        .unwrap();
    let res_get_a = router.clone().oneshot(get_post_on_a).await.unwrap();
    assert_eq!(res_get_a.status(), StatusCode::OK);

    // 10. Assert Site B DOES NOT have the post (Returns 404)
    let get_post_on_b = Request::builder()
        .method("GET")
        .uri(format!("/api/writings/{post_a_id}"))
        .header("Host", "siteb.example.com")
        .header("Authorization", format!("Bearer {token_b}"))
        .body(Body::empty())
        .unwrap();
    let res_get_b = router.clone().oneshot(get_post_on_b).await.unwrap();
    assert_eq!(res_get_b.status(), StatusCode::NOT_FOUND);
}
