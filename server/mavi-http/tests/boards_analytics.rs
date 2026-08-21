mod support;

use axum::http::{Method, StatusCode};
use mavi_http::api;
use serde_json::json;
use support::{bootstrap, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn boards_and_analytics_use_canonical_cursor_contracts() {
    let app = support::build_app().await;
    let token = bootstrap(&app, "Boards analytics HTTP test").await;

    let board = send(
        &app,
        Method::POST,
        "/api/v1/boards",
        Some(&token),
        Some(json!({"name": "Content board", "description": "Editorial work"})),
    )
    .await;
    assert_eq!(board.status(), StatusCode::CREATED);
    let board_id = response_json(board).await["id"]
        .as_str()
        .expect("board id")
        .to_owned();

    let first_list = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/{board_id}/lists"),
        Some(&token),
        Some(json!({"name": "Backlog"})),
    )
    .await;
    assert_eq!(first_list.status(), StatusCode::CREATED);
    let first_list_id = response_json(first_list).await["id"]
        .as_str()
        .expect("first list id")
        .to_owned();
    let second_list = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/{board_id}/lists"),
        Some(&token),
        Some(json!({"name": "Published"})),
    )
    .await;
    assert_eq!(second_list.status(), StatusCode::CREATED);
    let second_list_id = response_json(second_list).await["id"]
        .as_str()
        .expect("second list id")
        .to_owned();

    let first_card = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/lists/{first_list_id}/cards"),
        Some(&token),
        Some(json!({"title": "First post", "description": null, "assignee_id": null})),
    )
    .await;
    assert_eq!(first_card.status(), StatusCode::CREATED);
    let first_card_id = response_json(first_card).await["id"]
        .as_str()
        .expect("first card id")
        .to_owned();
    let second_card = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/lists/{first_list_id}/cards"),
        Some(&token),
        Some(json!({"title": "Second post", "description": null, "assignee_id": null})),
    )
    .await;
    assert_eq!(second_card.status(), StatusCode::CREATED);

    let cards = send(
        &app,
        Method::GET,
        &format!("/api/v1/boards/lists/{first_list_id}/cards?limit=1"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(cards.status(), StatusCode::OK);
    let cards = response_json(cards).await;
    assert_eq!(cards["items"].as_array().expect("cards").len(), 1);
    assert!(cards["next_cursor"].as_str().is_some());
    assert!(cards.get("offset").is_none());
    assert!(cards.get("page").is_none());

    let reordered = send(
        &app,
        Method::PUT,
        &format!("/api/v1/boards/{board_id}/lists/order"),
        Some(&token),
        Some(json!({"order": [second_list_id, first_list_id]})),
    )
    .await;
    assert_eq!(reordered.status(), StatusCode::OK);
    assert_eq!(
        response_json(reordered).await["items"][0]["id"],
        second_list_id
    );

    let moved = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/cards/{first_card_id}/move"),
        Some(&token),
        Some(json!({"list_id": second_list_id, "before_card_id": null})),
    )
    .await;
    assert_eq!(moved.status(), StatusCode::OK);
    assert_eq!(response_json(moved).await["list_id"], second_list_id);

    let comment = send(
        &app,
        Method::POST,
        &format!("/api/v1/boards/cards/{first_card_id}/comments"),
        Some(&token),
        Some(json!({"body": "Ready for review."})),
    )
    .await;
    assert_eq!(comment.status(), StatusCode::CREATED);

    let activity = send(
        &app,
        Method::GET,
        &format!("/api/v1/boards/{board_id}/activity?limit=2"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(activity.status(), StatusCode::OK);
    assert!(
        !response_json(activity).await["items"]
            .as_array()
            .expect("activity")
            .is_empty()
    );

    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/v1/boards/{board_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(trashed.status(), StatusCode::NO_CONTENT);
    let board_trash = send(
        &app,
        Method::GET,
        "/api/v1/trash?kind=board",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(board_trash.status(), StatusCode::OK);
    assert_eq!(response_json(board_trash).await["items"][0]["id"], board_id);
    let restored = send(
        &app,
        Method::POST,
        &format!("/api/v1/trash/board/{board_id}/restore"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);
    let restored_board = send(
        &app,
        Method::GET,
        &format!("/api/v1/boards/{board_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(restored_board.status(), StatusCode::OK);

    let anonymous_board_read = send(&app, Method::GET, "/api/v1/boards", None, None).await;
    assert_eq!(anonymous_board_read.status(), StatusCode::UNAUTHORIZED);

    let ingested = send(
        &app,
        Method::POST,
        "/public/v1/analytics/events",
        None,
        Some(json!({
            "events": [
                {"event_name": "page_view", "path": "/home", "value": null, "occurred_at": null},
                {"event_name": "page_view", "path": "/home", "value": 2, "occurred_at": null}
            ]
        })),
    )
    .await;
    assert_eq!(ingested.status(), StatusCode::ACCEPTED);
    assert_eq!(response_json(ingested).await["accepted"], 2);

    let daily = send(
        &app,
        Method::GET,
        "/api/v1/analytics/daily?limit=1",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(daily.status(), StatusCode::OK);
    let daily = response_json(daily).await;
    assert_eq!(daily["items"][0]["event_count"], 2);
    assert!(daily.get("offset").is_none());
    assert!(daily.get("page").is_none());

    let events = send(
        &app,
        Method::GET,
        "/api/v1/analytics/events?limit=1",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(events.status(), StatusCode::OK);
    assert_eq!(
        response_json(events).await["items"]
            .as_array()
            .expect("events")
            .len(),
        1
    );

    let pruned = send(
        &app,
        Method::POST,
        "/api/v1/analytics/prune",
        Some(&token),
        Some(json!({"raw_days": 1, "aggregate_days": 1})),
    )
    .await;
    assert_eq!(pruned.status(), StatusCode::OK);

    let catalog = api();
    for operation_id in [
        "boards.cards.move",
        "boards.comments.create",
        "boards.delete",
        "analytics.events.ingest",
        "analytics.daily.list",
    ] {
        assert!(
            catalog
                .endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == operation_id)
        );
    }
}
