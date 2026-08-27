#![allow(clippy::wildcard_imports)]

use super::super::*;

pub(super) fn board_routes() -> Router<HttpState> {
    Router::new()
        .route("/api/v1/boards", get(list_boards).post(create_board))
        .route(
            "/api/v1/boards/{id}",
            get(read_board).patch(update_board).delete(delete_board),
        )
        .route(
            "/api/v1/boards/{id}/lists",
            get(list_board_lists).post(create_board_list),
        )
        .route("/api/v1/boards/{id}/lists/order", put(reorder_board_lists))
        .route(
            "/api/v1/boards/lists/{id}/cards",
            get(list_board_cards).post(create_board_card),
        )
        .route(
            "/api/v1/boards/cards/{id}",
            get(read_board_card)
                .patch(update_board_card)
                .delete(delete_board_card),
        )
        .route("/api/v1/boards/cards/{id}/move", post(move_board_card))
        .route("/api/v1/boards/cards/{id}/assign", post(assign_board_card))
        .route(
            "/api/v1/boards/cards/{id}/comments",
            get(list_board_comments).post(create_board_comment),
        )
        .route(
            "/api/v1/boards/comments/{id}",
            patch(update_board_comment).delete(delete_board_comment),
        )
        .route("/api/v1/boards/{id}/activity", get(list_board_activity))
}

async fn list_boards(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Query(filter): Query<BoardListFilter>,
) -> Result<Json<Page<Board>>, HttpError> {
    require_boards_grant(&state, &context, Action::View, "Board", "board_collection")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .list_boards(&mut transaction, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_board(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Json(input): Json<CreateBoard>,
) -> Result<(StatusCode, Json<Board>), HttpError> {
    require_boards_grant(&state, &context, Action::Write, "Board", "board_collection")?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let board = state
        .boards
        .create_board(&mut transaction, &context, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(board)))
}

async fn read_board(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardId>,
) -> Result<Json<Board>, HttpError> {
    require_boards_grant(&state, &context, Action::View, "Board", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let board = state
        .boards
        .get_board(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(board))
}

async fn update_board(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardId>,
    Json(input): Json<UpdateBoard>,
) -> Result<Json<Board>, HttpError> {
    require_boards_grant(&state, &context, Action::Write, "Board", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let board = state
        .boards
        .update_board(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(board))
}

async fn delete_board(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardId>,
) -> Result<StatusCode, HttpError> {
    require_boards_grant(&state, &context, Action::Delete, "Board", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .boards
        .delete_board(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_board_lists(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(board_id): Path<BoardId>,
    Query(filter): Query<mavi_boards::ListPageFilter>,
) -> Result<Json<Page<BoardList>>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::View,
        "Board",
        board_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .list_lists(&mut transaction, board_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_board_list(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(board_id): Path<BoardId>,
    Json(input): Json<CreateList>,
) -> Result<(StatusCode, Json<BoardList>), HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Write,
        "Board",
        board_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let list = state
        .boards
        .create_list(&mut transaction, &context, board_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn reorder_board_lists(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(board_id): Path<BoardId>,
    Json(input): Json<ReorderLists>,
) -> Result<Json<Page<BoardList>>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Write,
        "Board",
        board_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .reorder_lists(&mut transaction, &context, board_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn list_board_cards(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<BoardListId>,
    Query(filter): Query<CardPageFilter>,
) -> Result<Json<Page<Card>>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::View,
        "BoardList",
        list_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .list_cards(&mut transaction, list_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(list_id): Path<BoardListId>,
    Json(input): Json<CreateCard>,
) -> Result<(StatusCode, Json<Card>), HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Write,
        "BoardList",
        list_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let card = state
        .boards
        .create_card(&mut transaction, &context, list_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(card)))
}

async fn read_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCardId>,
) -> Result<Json<Card>, HttpError> {
    require_boards_grant(&state, &context, Action::View, "BoardCard", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let card = state
        .boards
        .get_card(&mut transaction, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(card))
}

async fn update_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCardId>,
    Json(input): Json<UpdateCard>,
) -> Result<Json<Card>, HttpError> {
    require_boards_grant(&state, &context, Action::Write, "BoardCard", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let card = state
        .boards
        .update_card(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(card))
}

async fn delete_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCardId>,
) -> Result<StatusCode, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Delete,
        "BoardCard",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .boards
        .delete_card(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn move_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCardId>,
    Json(input): Json<MoveCard>,
) -> Result<Json<Card>, HttpError> {
    require_boards_grant(&state, &context, Action::Write, "BoardCard", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let card = state
        .boards
        .move_card(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(card))
}

async fn assign_board_card(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCardId>,
    Json(input): Json<AssignCard>,
) -> Result<Json<Card>, HttpError> {
    require_boards_grant(&state, &context, Action::Write, "BoardCard", id.to_string())?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let card = state
        .boards
        .assign_card(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(card))
}

async fn list_board_comments(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(card_id): Path<BoardCardId>,
    Query(filter): Query<CommentPageFilter>,
) -> Result<Json<Page<Comment>>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::View,
        "BoardCard",
        card_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .list_comments(&mut transaction, card_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}

async fn create_board_comment(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(card_id): Path<BoardCardId>,
    Json(input): Json<CreateComment>,
) -> Result<(StatusCode, Json<Comment>), HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Write,
        "BoardCard",
        card_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let comment = state
        .boards
        .create_comment(&mut transaction, &context, card_id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok((StatusCode::CREATED, Json(comment)))
}

async fn update_board_comment(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCommentId>,
    Json(input): Json<UpdateComment>,
) -> Result<Json<Comment>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Write,
        "BoardComment",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let comment = state
        .boards
        .update_comment(&mut transaction, &context, id, &input)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(comment))
}

async fn delete_board_comment(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(id): Path<BoardCommentId>,
) -> Result<StatusCode, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::Delete,
        "BoardComment",
        id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    state
        .boards
        .delete_comment(&mut transaction, &context, id)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_board_activity(
    State(state): State<HttpState>,
    Extension(context): Extension<SiteContext>,
    Path(board_id): Path<BoardId>,
    Query(filter): Query<ActivityPageFilter>,
) -> Result<Json<Page<Activity>>, HttpError> {
    require_boards_grant(
        &state,
        &context,
        Action::View,
        "Board",
        board_id.to_string(),
    )?;
    let mut transaction = state.runtime.begin(&context).await.map_err(HttpError)?;
    let page = state
        .boards
        .list_activity(&mut transaction, board_id, &filter)
        .await
        .map_err(HttpError)?;
    transaction.commit().await.map_err(HttpError)?;
    Ok(Json(page))
}
