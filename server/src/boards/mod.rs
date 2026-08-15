//! What a site works through in order.
//!
//! Enquiries, repairs, applications: a board is stages a site names itself and
//! cards that move between them. The first version of this had six stages
//! written into the software, named after one agency's sales process, and
//! every site on the machine got them.
use axum::Json;
use axum::extract::{Path, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::money::{Currency, Money};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::say;
use crate::kernel::types::Title;

fn boards(access: Access) -> Needs {
    Needs::new(Capability::Boards, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = board_endpoints();
    all.extend(card_endpoints());
    all
}

fn board_endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/boards",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Board>>(),
        Endpoint::post(
            "/api/boards",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewBoard>()
        .gives::<Board>(),
        Endpoint::get(
            "/api/boards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::View)),
                rate: RatePolicy::None,
            },
            read,
        )
        .gives::<Full>(),
        Endpoint::patch(
            "/api/boards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Write)),
                rate: RatePolicy::None,
            },
            rename,
        )
        .takes::<BoardChanges>()
        .gives::<Board>(),
        Endpoint::delete(
            "/api/boards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::get(
            "/api/boards/{id}/cards",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::View)),
                rate: RatePolicy::None,
            },
            cards,
        )
        .gives::<Page<Card>>(),
    ]
}

/// One card, on its own: read, moved, noted on, thrown away. Its own list
/// because a board's endpoints and a card's are two screens.
fn card_endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/cards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::View)),
                rate: RatePolicy::None,
            },
            read_card,
        )
        .gives::<Card>(),
        Endpoint::delete(
            "/api/cards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove_card,
        ),
        Endpoint::get(
            "/api/cards/{id}/notes",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::View)),
                rate: RatePolicy::None,
            },
            notes,
        )
        .gives::<Page<Note>>(),
        Endpoint::post(
            "/api/boards/{id}/cards",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Write)),
                rate: RatePolicy::None,
            },
            add_card,
        )
        .takes::<NewCard>()
        .gives::<Card>(),
        Endpoint::patch(
            "/api/cards/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Write)),
                rate: RatePolicy::None,
            },
            move_card,
        )
        .takes::<CardChanges>()
        .gives::<Card>(),
        Endpoint::post(
            "/api/cards/{id}/notes",
            Guard {
                audience: Audience::User,
                needs: Some(boards(Access::Write)),
                rate: RatePolicy::None,
            },
            add_note,
        )
        .takes::<NewNote>(),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Board {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Full {
    pub board: Board,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Stage {
    pub id: Uuid,
    pub name: String,
    pub position: i32,
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Card {
    pub id: Uuid,
    pub stage_id: Uuid,
    pub title: String,
    pub detail: Option<String>,
    pub owner_id: Option<Uuid>,
    pub value: Option<Money>,
    pub position: f64,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Card {
    const SUBJECT: &'static str = "card";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "title": self.title,
            "stage_id": self.stage_id,
            "value": self.value.map(|money| money.minor),
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewBoard {
    pub name: Title,
    /// The columns it starts with. A board with no stages is a board nothing
    /// can be put on, so there is a default rather than an empty one.
    #[serde(default)]
    pub stages: Vec<Title>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewCard {
    pub stage_id: Uuid,
    pub title: Title,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub owner_id: Option<Uuid>,
    #[serde(default)]
    pub value_minor: Option<i64>,
    #[serde(default)]
    pub currency: Option<Currency>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CardChanges {
    pub title: Option<Title>,
    pub detail: Option<String>,
    pub stage_id: Option<Uuid>,
    pub position: Option<f64>,
    pub owner_id: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Note {
    pub id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BoardChanges {
    pub name: Title,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewNote {
    pub body: String,
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Board>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Board> = sqlx::query_as(
        "select id, name, created_at from boards
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |board| {
        board.created_at.to_rfc3339()
    })))
}

/// The cards on one board, without its stages. What a screen wants when it is
/// showing one column, or looking for a card by name.
async fn cards(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Card>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows = sqlx::query(
        "select id, stage_id, title, detail, owner_id, value_minor, currency, position,
                created_at
           from cards
          where board_id = $1 and deleted_at is null
            and ($2::timestamptz is null or created_at < $2)
          order by created_at desc
          limit $3",
    )
    .bind(id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let cards: Vec<Card> = rows.iter().map(card_from).collect();

    Ok(Json(Page::build(&page, cards, |card| {
        card.created_at.to_rfc3339()
    })))
}

/// One card, and what has been said about it.
async fn read_card(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Card>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let card = one(&mut conn, id).await?;
    conn.commit().await?;

    Ok(Json(card))
}

/// What has been said about a card, newest first.
async fn notes(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Note>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    one(&mut conn, id).await?;

    let rows: Vec<Note> = sqlx::query_as(
        "select id, author_id, body, created_at from card_notes
          where card_id = $1
            and ($2::timestamptz is null or created_at < $2)
          order by created_at desc
          limit $3",
    )
    .bind(id)
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |note| {
        note.created_at.to_rfc3339()
    })))
}

/// A card thrown away goes to the bin like everything else, so it can be put
/// back by somebody who moved the wrong one.
async fn remove_card(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let card = one(&mut conn, id).await?;

    sqlx::query("update cards set deleted_at = now() where id = $1 and deleted_at is null")
        .bind(id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "threw away",
        Some(&card),
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn rename(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(body): Json<BoardChanges>,
) -> Result<Audited<Json<Board>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let board: Option<Board> = sqlx::query_as(
        "update boards set name = $2
          where id = $1 and deleted_at is null
         returning id, name, created_at",
    )
    .bind(id)
    .bind(body.name.as_str())
    .fetch_optional(conn.conn())
    .await?;

    let board = board.ok_or(AppError::NotFound("board"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "renamed a board",
        "board",
        Some(&board.id.to_string()),
        &serde_json::json!({ "name": board.name }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(board)))
}

/// The whole board, and everything on it. A card is not thrown away one at a
/// time on the way out: what goes to the bin is the board, and putting it back
/// is what brings its cards back with it.
async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone =
        sqlx::query("update boards set deleted_at = now() where id = $1 and deleted_at is null")
            .bind(id)
            .execute(conn.conn())
            .await?
            .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("board"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "threw away a board",
        "board",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

fn card_from(row: &sqlx::postgres::PgRow) -> Card {
    Card {
        id: row.get("id"),
        stage_id: row.get("stage_id"),
        title: row.get("title"),
        detail: row.get("detail"),
        owner_id: row.get("owner_id"),
        value: row
            .get::<Option<i64>, _>("value_minor")
            .zip(row.get::<Option<Currency>, _>("currency"))
            .map(|(minor, currency)| Money::new(minor, currency)),
        position: row.get("position"),
        created_at: row.get("created_at"),
    }
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewBoard>,
) -> Result<Audited<(StatusCode, Json<Board>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let board: Board = sqlx::query_as(
        "insert into boards (tenant_id, name) values ($1, $2) returning id, name, created_at",
    )
    .bind(caller.tenant().0)
    .bind(body.name.as_str())
    .fetch_one(conn.conn())
    .await?;

    let stages: Vec<String> = if body.stages.is_empty() {
        ["To do", "Doing", "Done"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect()
    } else {
        body.stages.iter().map(ToString::to_string).collect()
    };

    for (position, name) in stages.iter().enumerate() {
        sqlx::query(
            "insert into board_stages (tenant_id, board_id, name, position)
             values ($1, $2, $3, $4)",
        )
        .bind(caller.tenant().0)
        .bind(board.id)
        .bind(name)
        .bind(i32::try_from(position).unwrap_or(i32::MAX))
        .execute(conn.conn())
        .await?;
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a board",
        "board",
        Some(&board.id.to_string()),
        &serde_json::json!({ "name": board.name, "stages": stages }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(board))))
}

/// A board with everything on it, in two queries: one for the board and one
/// join for every stage and card. A card per query is how a pipeline with two
/// hundred deals stops opening.
async fn read(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Full>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let board: Board = sqlx::query_as(
        "select id, name, created_at from boards where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("board"))?;

    let rows = sqlx::query(
        "select s.id as stage_id, s.name as stage_name, s.position as stage_position,
                c.id as card_id, c.title, c.detail, c.owner_id, c.value_minor,
                c.currency, c.position as card_position, c.created_at as card_created_at
           from board_stages s
           left join cards c on c.stage_id = s.id and c.deleted_at is null
          where s.board_id = $1
          order by s.position, c.position",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut stages: Vec<Stage> = Vec::new();

    for row in rows {
        let stage_id: Uuid = row.get("stage_id");

        if stages.last().map(|stage| stage.id) != Some(stage_id) {
            stages.push(Stage {
                id: stage_id,
                name: row.get("stage_name"),
                position: row.get("stage_position"),
                cards: Vec::new(),
            });
        }

        if let Some(card_id) = row.get::<Option<Uuid>, _>("card_id")
            && let Some(stage) = stages.last_mut()
        {
            stage.cards.push(Card {
                id: card_id,
                stage_id,
                title: row.get("title"),
                detail: row.get("detail"),
                owner_id: row.get("owner_id"),
                value: row
                    .get::<Option<i64>, _>("value_minor")
                    .zip(row.get::<Option<Currency>, _>("currency"))
                    .map(|(minor, currency)| Money::new(minor, currency)),
                position: row.get("card_position"),
                created_at: row.get("card_created_at"),
            });
        }
    }

    Ok(Json(Full { board, stages }))
}

async fn add_card(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(board_id): Path<Uuid>,
    Json(body): Json<NewCard>,
) -> Result<Audited<(StatusCode, Json<Card>)>> {
    if body.value_minor.is_some() != body.currency.is_some() {
        return Err(AppError::Invalid(
            say::AMOUNT_CURRENCY_ARRIVE_TOGETHER_OR_NOT.into(),
        ));
    }

    let mut conn = state.db.tenant(caller.tenant()).await?;

    // At the end of its stage, wherever that is now.
    let last: (Option<f64>,) =
        sqlx::query_as("select max(position) from cards where stage_id = $1")
            .bind(body.stage_id)
            .fetch_one(conn.conn())
            .await?;

    let card = insert_card(
        &mut conn,
        &caller,
        board_id,
        &body,
        last.0.unwrap_or(0.0) + 1.0,
    )
    .await?;

    let receipt = audit::record(&mut conn, Actor::of(&caller), "made", None, Some(&card)).await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(card))))
}

async fn insert_card(
    conn: &mut TenantConn,
    caller: &Caller,
    board_id: Uuid,
    body: &NewCard,
    position: f64,
) -> Result<Card> {
    let row = sqlx::query(
        "insert into cards
             (tenant_id, board_id, stage_id, title, detail, owner_id, value_minor, currency,
              position)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         returning id, stage_id, title, detail, owner_id, value_minor, currency, position,
                   created_at",
    )
    .bind(caller.tenant().0)
    .bind(board_id)
    .bind(body.stage_id)
    .bind(body.title.as_str())
    .bind(body.detail.as_deref())
    .bind(body.owner_id)
    .bind(body.value_minor)
    .bind(body.currency)
    .bind(position)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => AppError::NotFound("stage"),
            _ => AppError::Database(error),
        }
    })?;

    Ok(Card {
        id: row.get("id"),
        stage_id: row.get("stage_id"),
        title: row.get("title"),
        detail: row.get("detail"),
        owner_id: row.get("owner_id"),
        value: row
            .get::<Option<i64>, _>("value_minor")
            .zip(row.get::<Option<Currency>, _>("currency"))
            .map(|(minor, currency)| Money::new(minor, currency)),
        position: row.get("position"),
        created_at: row.get("created_at"),
    })
}

/// Moving a card is one row changed, whichever column it lands in and wherever
/// between two others it goes.
async fn move_card(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(changes): Json<CardChanges>,
) -> Result<Audited<Json<Card>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let before = one(&mut conn, id).await?;

    let row = sqlx::query(
        "update cards
            set title = coalesce($2, title),
                detail = coalesce($3, detail),
                stage_id = coalesce($4, stage_id),
                position = coalesce($5, position),
                owner_id = coalesce($6, owner_id)
          where id = $1 and deleted_at is null
         returning id, stage_id, title, detail, owner_id, value_minor, currency, position,
                   created_at",
    )
    .bind(id)
    .bind(changes.title.as_ref().map(Title::as_str))
    .bind(changes.detail.as_deref())
    .bind(changes.stage_id)
    .bind(changes.position)
    .bind(changes.owner_id)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => AppError::NotFound("stage"),
            _ => AppError::Database(error),
        }
    })?;

    let after = card_from(&row);

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "moved",
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn one(conn: &mut TenantConn, id: Uuid) -> Result<Card> {
    let row = sqlx::query(
        "select id, stage_id, title, detail, owner_id, value_minor, currency, position,
                created_at
           from cards where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("card"))?;

    Ok(Card {
        id: row.get("id"),
        stage_id: row.get("stage_id"),
        title: row.get("title"),
        detail: row.get("detail"),
        owner_id: row.get("owner_id"),
        value: row
            .get::<Option<i64>, _>("value_minor")
            .zip(row.get::<Option<Currency>, _>("currency"))
            .map(|(minor, currency)| Money::new(minor, currency)),
        position: row.get("position"),
        created_at: row.get("created_at"),
    })
}

async fn add_note(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(body): Json<NewNote>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    one(&mut conn, id).await?;

    sqlx::query(
        "insert into card_notes (tenant_id, card_id, author_id, body) values ($1, $2, $3, $4)",
    )
    .bind(caller.tenant().0)
    .bind(id)
    .bind(caller.user.as_ref().map(|user| user.user_id))
    .bind(&body.body)
    .execute(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23514" => {
                AppError::Invalid(say::NOTE_BETWEEN_ONE_TEN_THOUSAND_CHARACTERS.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "noted",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::CREATED))
}
