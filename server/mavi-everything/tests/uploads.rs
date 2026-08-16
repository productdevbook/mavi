//! Uploading something, and where it ends up.
//!
//! The one place in this API where bytes and a row have to agree, and the one
//! endpoint whose body is not JSON. What is being checked is that the two
//! rules the media crate is built on survive the trip through a router: a file
//! is what its bytes say, and it is never kept under the name somebody chose.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_everything::mounted::site;
use mavi_files::InADirectory;
use mavi_http::Caller;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nothing was ever uploaded"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_up_{}_{}",
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

fn somebody() -> mavi_serve::WhoIsAsking {
    Arc::new(|_| {
        Box::pin(async {
            Caller::AnAccount {
                id: "01930000-0000-7000-8000-000000000001".to_owned(),
                grants: Grants::of(["media:view", "media:write"].map(ToOwned::to_owned)),
                session: None,
            }
        })
    })
}

/// A directory of this test's own, kept so the test can look in it.
fn a_directory() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mavi-uploads-{}", Uuid::now_v7()))
}

async fn asked(db: &Db, under: &std::path::Path, request: Request<Body>) -> (StatusCode, Value) {
    let files: Arc<dyn mavi_core::ports::Files> = Arc::new(InADirectory::at(under));

    let answer = site(db, &files, somebody())
        .into_router()
        .oneshot(request)
        .await
        .expect("an answer");

    let status = answer.status();
    let body = axum::body::to_bytes(answer.into_body(), 256 * 1024)
        .await
        .expect("a body");

    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };

    (status, body)
}

const A_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR and some bytes";

fn uploading(name: &str, bytes: &'static [u8]) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(format!("/api/files?name={name}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(bytes))
        .expect("a request")
}

#[tokio::test]
async fn a_file_is_kept_under_its_id_and_never_under_its_name() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("kept").await;
    let under = a_directory();

    let (status, file) = asked(&db, &under, uploading("holiday%20photo.png", A_PNG)).await;

    assert_eq!(status, StatusCode::CREATED, "{file}");

    // What it is came from the bytes, not from the name.
    assert_eq!(file["kind"], "image");
    assert_eq!(file["mime"], "image/png");

    // The name is shown back and used for nothing: where it is kept has the
    // id in it and nothing else.
    assert_eq!(file["name"], "holiday photo.png");

    let kept_at = file["kept_at"].as_str().expect("a place").to_owned();
    let flat = file["id"].as_str().expect("an id").replace('-', "");

    assert!(kept_at.contains(&flat[..2]), "{kept_at}");
    assert!(!kept_at.contains("holiday"), "{kept_at}");

    // And the bytes really are there, under that name and no other.
    let on_disk = under.join(&kept_at);
    assert_eq!(
        tokio::fs::read(&on_disk).await.expect("the bytes"),
        A_PNG,
        "the bytes are not where the row says they are"
    );

    tokio::fs::remove_dir_all(&under).await.ok();
}

#[tokio::test]
async fn a_script_calling_itself_a_picture_is_not_taken() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("lying").await;
    let under = a_directory();

    let (status, refusal) = asked(
        &db,
        &under,
        uploading("holiday.png", b"<!doctype html><script>alert(1)</script>"),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refusal["key"], "that_is_not_a_kind_of_file_this_takes");

    // And nothing was written anywhere: not a row, and not a byte.
    let written: i64 = sqlx::query_scalar("select count(*) from files")
        .fetch_one(db.pool())
        .await
        .expect("a count");

    assert_eq!(written, 0);
    assert!(
        !under.exists(),
        "bytes were kept for a file that was refused"
    );
}

#[tokio::test]
async fn removing_one_takes_the_bytes_with_it() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("gone").await;
    let under = a_directory();

    let (_, file) = asked(&db, &under, uploading("a-picture.png", A_PNG)).await;
    let id = file["id"].as_str().expect("an id").to_owned();
    let kept_at = file["kept_at"].as_str().expect("a place").to_owned();

    let (status, _) = asked(
        &db,
        &under,
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/files/{id}"))
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        !under.join(&kept_at).exists(),
        "the row went and the bytes stayed"
    );

    tokio::fs::remove_dir_all(&under).await.ok();
}
