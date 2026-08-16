//! Reading and writing boards.
//!
//! The only thing here that is not a plain insert is where a card lands, and
//! that is the whole of [`crate::place`]: a number between its neighbours,
//! with a refusal when there is no room left rather than two cards in one
//! place.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Tx;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::place;

pub const THERE_IS_NO_BOARD_LIKE_THAT: &str = "there_is_no_board_like_that";
pub const THERE_IS_NO_CARD_LIKE_THAT: &str = "there_is_no_card_like_that";
pub const A_BOARD_HAS_SOMEWHERE_TO_PUT_THINGS: &str = "a_board_has_somewhere_to_put_things";

/// One board, with its columns.
#[derive(Clone, Debug, Serialize)]
pub struct Board {
    pub id: Uuid,
    pub name: String,
    pub stages: Vec<Stage>,
    pub created_at: DateTime<Utc>,
}

/// One column.
#[derive(Clone, Debug, Serialize)]
pub struct Stage {
    pub id: Uuid,
    pub name: String,
    pub place: i32,
}

/// One card.
#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: Uuid,
    pub board_id: Uuid,
    pub stage_id: Uuid,
    pub title: String,
    pub detail: Option<String>,
    pub owner: Option<String>,
    pub place: f64,
    pub created_at: DateTime<Utc>,
}

fn a_card(row: &PgRow) -> Result<Card> {
    Ok(Card {
        id: row.try_get("id").map_err(Error::internal)?,
        board_id: row.try_get("board_id").map_err(Error::internal)?,
        stage_id: row.try_get("stage_id").map_err(Error::internal)?,
        title: row.try_get("title").map_err(Error::internal)?,
        detail: row.try_get("detail").map_err(Error::internal)?,
        owner: row.try_get("owner").map_err(Error::internal)?,
        place: row.try_get("place").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// The boards this site keeps.
pub async fn list(tx: &mut Tx) -> Result<Vec<Board>> {
    let rows = sqlx::query(
        "select id, name, created_at from boards
          where deleted_at is null order by created_at desc, id desc",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    let mut boards = Vec::with_capacity(rows.len());

    for row in &rows {
        let id: Uuid = row.try_get("id").map_err(Error::internal)?;

        boards.push(Board {
            id,
            name: row.try_get("name").map_err(Error::internal)?,
            stages: stages(tx, id).await?,
            created_at: row.try_get("created_at").map_err(Error::internal)?,
        });
    }

    Ok(boards)
}

async fn stages(tx: &mut Tx, board: Uuid) -> Result<Vec<Stage>> {
    let rows = sqlx::query("select id, name, place from stages where board_id = $1 order by place")
        .bind(board)
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Stage {
                id: row.try_get("id").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                place: row.try_get("place").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// What making one asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewBoard {
    pub name: String,
    /// The columns it starts with. A board with none is a board nothing can be
    /// put on, so it is refused here rather than made and then wondered about.
    pub stages: Vec<String>,
}

/// Makes one, with the columns it starts with.
pub async fn make(tx: &mut Tx, new: &NewBoard) -> Result<Board> {
    if new.stages.is_empty() {
        return Err(Error::invalid(Say::of(A_BOARD_HAS_SOMEWHERE_TO_PUT_THINGS)));
    }

    let id = Uuid::now_v7();

    sqlx::query("insert into boards (id, name) values ($1, $2)")
        .bind(id)
        .bind(new.name.trim())
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    for (at, name) in new.stages.iter().enumerate() {
        sqlx::query("insert into stages (id, board_id, name, place) values ($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(id)
            .bind(name.trim())
            .bind(i32::try_from(at).unwrap_or(i32::MAX))
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    read(tx, id).await
}

/// One board.
pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Board> {
    let row =
        sqlx::query("select id, name, created_at from boards where id = $1 and deleted_at is null")
            .bind(id)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?
            .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_BOARD_LIKE_THAT)))?;

    Ok(Board {
        id,
        name: row.try_get("name").map_err(Error::internal)?,
        stages: stages(tx, id).await?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// The cards on one board, in the order somebody put them in.
pub async fn cards(tx: &mut Tx, board: Uuid, stage: Option<Uuid>) -> Result<Vec<Card>> {
    let rows = sqlx::query(
        "select id, board_id, stage_id, title, detail, owner, place, created_at from cards
          where board_id = $1 and ($2::uuid is null or stage_id = $2)
            and deleted_at is null
          order by stage_id, place, id",
    )
    .bind(board)
    .bind(stage)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_card).collect()
}

/// What putting one on asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCard {
    pub stage: Uuid,
    pub title: String,
    pub detail: Option<String>,
    pub owner: Option<String>,
}

/// Puts a card at the bottom of its column.
pub async fn add(tx: &mut Tx, board: Uuid, new: &NewCard) -> Result<Card> {
    let below: Option<f64> = sqlx::query_scalar(
        "select max(place) from cards where stage_id = $1 and deleted_at is null",
    )
    .bind(new.stage)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let place = place::between(below, None)?;

    let row = sqlx::query(
        "insert into cards (id, board_id, stage_id, title, detail, owner, place)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning id, board_id, stage_id, title, detail, owner, place, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(board)
    .bind(new.stage)
    .bind(new.title.trim())
    .bind(new.detail.as_deref())
    .bind(new.owner.as_deref())
    .bind(place)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    a_card(&row)
}

/// What may be changed about a card.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CardChanges {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub owner: Option<String>,
}

/// Changes what a card says.
pub async fn change(tx: &mut Tx, id: Uuid, changes: &CardChanges) -> Result<Card> {
    let row = sqlx::query(
        "update cards
            set title = coalesce($2, title),
                detail = coalesce($3, detail),
                owner = coalesce($4, owner),
                updated_at = now()
          where id = $1 and deleted_at is null
         returning id, board_id, stage_id, title, detail, owner, place, created_at",
    )
    .bind(id)
    .bind(changes.title.as_deref())
    .bind(changes.detail.as_deref())
    .bind(changes.owner.as_deref())
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_card)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_CARD_LIKE_THAT)))
}

/// Where a card was dropped: which column, and between which two cards.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Between {
    pub stage: Uuid,
    pub after: Option<Uuid>,
    pub before: Option<Uuid>,
}

/// Drags a card.
///
/// Given its neighbours rather than a number, because what a person did is drop
/// it between two cards. When there is no room left between those two, the
/// column is spread out and the drop is tried again — once, because a spread
/// column has room by construction and a second failure is a bug rather than a
/// crowded column.
pub async fn moved(tx: &mut Tx, id: Uuid, dropped: &Between) -> Result<Card> {
    let place = if let Ok(place) = somewhere_between(tx, dropped).await {
        place
    } else {
        // No room left between those two. The column is given room again,
        // keeping the order it is already in, and the drop is tried once more
        // — a spread column has room by construction, so a second failure is a
        // bug rather than a crowded column.
        spread_out(tx, dropped.stage).await?;

        somewhere_between(tx, dropped).await?
    };

    let row = sqlx::query(
        "update cards set stage_id = $2, place = $3, updated_at = now()
          where id = $1 and deleted_at is null
         returning id, board_id, stage_id, title, detail, owner, place, created_at",
    )
    .bind(id)
    .bind(dropped.stage)
    .bind(place)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_card)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_CARD_LIKE_THAT)))
}

async fn somewhere_between(tx: &mut Tx, dropped: &Between) -> Result<f64> {
    let after = match dropped.after {
        Some(card) => place_of(tx, card).await?,
        None => None,
    };

    let before = match dropped.before {
        Some(card) => place_of(tx, card).await?,
        None => None,
    };

    place::between(after, before)
}

async fn place_of(tx: &mut Tx, card: Uuid) -> Result<Option<f64>> {
    sqlx::query_scalar("select place from cards where id = $1 and deleted_at is null")
        .bind(card)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)
}

/// Gives a column room again, keeping the order it is already in.
async fn spread_out(tx: &mut Tx, stage: Uuid) -> Result<()> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "select id from cards where stage_id = $1 and deleted_at is null order by place, id",
    )
    .bind(stage)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    for (id, place) in ids.iter().zip(place::spread(ids.len())) {
        sqlx::query("update cards set place = $2 where id = $1")
            .bind(id)
            .bind(place)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
    }

    Ok(())
}

/// Takes a card off.
pub async fn remove(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone =
        sqlx::query("update cards set deleted_at = now() where id = $1 and deleted_at is null")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_CARD_LIKE_THAT)));
    }

    Ok(())
}
