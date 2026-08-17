use super::helpers::took_it_away;
// Domain route module: boards

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{a_uuid, handling, wrote_about};

/// Boards, and where a card sits on one.
#[must_use]
pub fn what_is_being_worked_on(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_boards::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "boards.list" => Some(handling(db, |db, _| {
                Box::pin(async move { boards(&db).await })
            })),
            "boards.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_board(&db, &asked).await })
            })),
            "boards.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_board(&db, &asked).await })
            })),
            "cards.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { cards(&db, &asked).await })
            })),
            "cards.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_card(&db, &asked).await })
            })),
            "cards.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_card(&db, &asked).await })
            })),
            "cards.move" => Some(handling(db, |db, asked| {
                Box::pin(async move { moved_a_card(&db, &asked).await })
            })),
            "boards.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_a_board_away(&db, &asked).await })
            })),
            "cards.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_card(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_boards::to_write()
            } else {
                mavi_boards::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// Courses, who is on them, and what a student reaches.
async fn took_a_board_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    took_it_away(db, asked, "boards.remove", "board", |tx, id| {
        Box::pin(mavi_boards::store::remove_a_board(tx, id))
    })
    .await
}

async fn boards(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let boards = mavi_boards::store::list(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(boards).map_err(Error::internal)?,
    ))
}

async fn made_a_board(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_boards::store::NewBoard = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_board")))?;

    let mut tx = db.begin().await?;
    let board = mavi_boards::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "boards.make",
        "board",
        Some(&board.id.to_string()),
        &serde_json::json!({ "stages": board.stages.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(board).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_board(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let board = mavi_boards::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(board).map_err(Error::internal)?,
    ))
}

async fn cards(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let stage = asked
        .query
        .get("stage")
        .and_then(|stage| Uuid::parse_str(stage).ok());

    let mut tx = db.begin().await?;
    let cards = mavi_boards::store::cards(&mut tx, a_uuid(asked)?, stage).await?;

    Ok(Answered::Read(
        serde_json::to_value(cards).map_err(Error::internal)?,
    ))
}

async fn made_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_boards::store::NewCard = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_card")))?;

    let board = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::add(&mut tx, board, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.make",
        "card",
        Some(&card.id.to_string()),
        &serde_json::json!({ "board": board }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_boards::store::CardChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_card")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.change",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn moved_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let dropped: mavi_boards::store::Between = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_where_a_card_was_dropped")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::moved(&mut tx, id, &dropped).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.move",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({ "stage": dropped.stage }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_boards::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.remove",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
