//! Site-scoped collaboration boards.
//!
//! Boards deliberately use integer positions and transactional reindexing.
//! The old floating-point midpoint trick made ordering silently corrupt after
//! enough drag operations. Every move/reorder validates the complete ID set,
//! locks the board first, and rewrites a contiguous order under a deferrable
//! unique constraint. Comments and activity are separate append-oriented
//! records, so a card read never has to reconstruct history from mutable rows.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, BoardCardId, BoardCommentId, BoardId, BoardListId, Caller, Capability, Cursor,
    ErrorCode, MaviError, Page, PageRequest, PersonId, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

mod relocation;

pub use relocation::{
    BoardActivityRelocation, BoardCardRelocation, BoardCommentRelocation, BoardListRelocation,
    BoardRelocation, BoardsRelocation,
};

pub const MAX_BOARD_NAME: usize = 200;
pub const MAX_LIST_NAME: usize = 120;
pub const MAX_CARD_TITLE: usize = 300;
pub const MAX_CARD_DESCRIPTION: usize = 20_000;
pub const MAX_COMMENT: usize = 10_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBoard {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateBoard {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Board {
    pub id: BoardId,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateList {
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ListPageFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct BoardList {
    pub id: BoardListId,
    pub board_id: BoardId,
    pub name: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReorderLists {
    pub order: Vec<BoardListId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCard {
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<PersonId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardPageFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub assignee_id: Option<PersonId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateCard {
    pub title: Option<String>,
    pub description: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Card {
    pub id: BoardCardId,
    pub board_id: BoardId,
    pub list_id: BoardListId,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<PersonId>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoveCard {
    pub list_id: BoardListId,
    pub before_card_id: Option<BoardCardId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignCard {
    pub assignee_id: Option<PersonId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateComment {
    pub body: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommentPageFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateComment {
    pub body: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Comment {
    pub id: BoardCommentId,
    pub board_id: BoardId,
    pub card_id: BoardCardId,
    pub author_id: Option<PersonId>,
    pub body: String,
    pub edited_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityPageFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub card_id: Option<BoardCardId>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Activity {
    pub id: Uuid,
    pub board_id: BoardId,
    pub card_id: Option<BoardCardId>,
    pub kind: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BoardService;

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Boards,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Boards,
        action: Action::Write,
    };
    let delete = Permission {
        capability: Capability::Boards,
        action: Action::Delete,
    };
    mavi_contract::Api::new(vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/boards",
            "boards.list",
            "List site boards with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("BoardListFilter")
        .returns(200, "BoardPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards",
            "boards.create",
            "Create a collaboration board",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateBoard")
        .returns(201, "Board")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/{id}",
            "boards.read",
            "Read one collaboration board",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Board")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/boards/{id}",
            "boards.update",
            "Update board metadata",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateBoard")
        .returns(200, "Board")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/boards/{id}",
            "boards.delete",
            "Archive a board and its visible work",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/{id}/lists",
            "boards.lists.list",
            "List board columns in order",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("ListPageFilter")
        .returns(200, "BoardListPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards/{id}/lists",
            "boards.lists.create",
            "Create a board column",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateList")
        .returns(201, "BoardList")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/boards/{id}/lists/order",
            "boards.lists.reorder",
            "Reorder all board columns atomically",
        )
        .account_or_assistant()
        .requires(write)
        .takes("ReorderLists")
        .returns(200, "BoardListPage")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/lists/{id}/cards",
            "boards.cards.list",
            "List cards in a column with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("CardPageFilter")
        .returns(200, "CardPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards/lists/{id}/cards",
            "boards.cards.create",
            "Create a card in a column",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateCard")
        .returns(201, "Card")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/cards/{id}",
            "boards.cards.read",
            "Read one card",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Card")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/boards/cards/{id}",
            "boards.cards.update",
            "Update card content",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateCard")
        .returns(200, "Card")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards/cards/{id}/move",
            "boards.cards.move",
            "Move a card atomically between columns",
        )
        .account_or_assistant()
        .requires(write)
        .takes("MoveCard")
        .returns(200, "Card")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards/cards/{id}/assign",
            "boards.cards.assign",
            "Assign or unassign a card",
        )
        .account_or_assistant()
        .requires(write)
        .takes("AssignCard")
        .returns(200, "Card")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/cards/{id}/comments",
            "boards.comments.list",
            "List card comments with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("CommentPageFilter")
        .returns(200, "CommentPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/boards/cards/{id}/comments",
            "boards.comments.create",
            "Add a card comment",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateComment")
        .returns(201, "Comment")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/boards/comments/{id}",
            "boards.comments.update",
            "Edit your card comment",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateComment")
        .returns(200, "Comment")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/boards/comments/{id}",
            "boards.comments.delete",
            "Delete a card comment",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/boards/{id}/activity",
            "boards.activity.list",
            "List immutable board activity",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("ActivityPageFilter")
        .returns(200, "ActivityPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "CreateBoard",
            json!({"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1,"maxLength":200},"description":{"type":["string","null"],"maxLength":10000}}}),
        ),
        Shape::new(
            "UpdateBoard",
            json!({"type":"object","properties":{"name":{"type":["string","null"],"maxLength":200},"description":{"type":["string","null"]},"archived":{"type":["boolean","null"]}}}),
        ),
        Shape::new(
            "BoardListFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"archived":{"type":["boolean","null"]}}}),
        ),
        Shape::new(
            "Board",
            json!({"type":"object","required":["id","name","description","archived","created_at","updated_at"],"properties":{"id":{"type":"string","format":"uuid"},"name":{"type":"string"},"description":{"type":["string","null"]},"archived":{"type":"boolean"},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "BoardPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Board"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "CreateList",
            json!({"type":"object","required":["name"],"properties":{"name":{"type":"string","minLength":1,"maxLength":120}}}),
        ),
        Shape::new(
            "ListPageFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100}}}),
        ),
        Shape::new(
            "BoardList",
            json!({"type":"object","required":["id","board_id","name","position","created_at","updated_at"],"properties":{"id":{"type":"string","format":"uuid"},"board_id":{"type":"string","format":"uuid"},"name":{"type":"string"},"position":{"type":"integer"},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "BoardListPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/BoardList"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "ReorderLists",
            json!({"type":"object","required":["order"],"properties":{"order":{"type":"array","items":{"type":"string","format":"uuid"}}}}),
        ),
        Shape::new(
            "CreateCard",
            json!({"type":"object","required":["title","description","assignee_id"],"properties":{"title":{"type":"string","minLength":1,"maxLength":300},"description":{"type":["string","null"],"maxLength":20000},"assignee_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "CardPageFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"assignee_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "UpdateCard",
            json!({"type":"object","properties":{"title":{"type":["string","null"],"maxLength":300},"description":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "Card",
            json!({"type":"object","required":["id","board_id","list_id","title","description","assignee_id","position","created_at","updated_at"],"properties":{"id":{"type":"string","format":"uuid"},"board_id":{"type":"string","format":"uuid"},"list_id":{"type":"string","format":"uuid"},"title":{"type":"string"},"description":{"type":["string","null"]},"assignee_id":{"type":["string","null"],"format":"uuid"},"position":{"type":"integer"},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "CardPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Card"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "MoveCard",
            json!({"type":"object","required":["list_id","before_card_id"],"properties":{"list_id":{"type":"string","format":"uuid"},"before_card_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "AssignCard",
            json!({"type":"object","required":["assignee_id"],"properties":{"assignee_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "CreateComment",
            json!({"type":"object","required":["body"],"properties":{"body":{"type":"string","minLength":1,"maxLength":10000}}}),
        ),
        Shape::new(
            "CommentPageFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100}}}),
        ),
        Shape::new(
            "UpdateComment",
            json!({"type":"object","required":["body"],"properties":{"body":{"type":"string","minLength":1,"maxLength":10000}}}),
        ),
        Shape::new(
            "Comment",
            json!({"type":"object","required":["id","board_id","card_id","author_id","body","edited_at","created_at"],"properties":{"id":{"type":"string","format":"uuid"},"board_id":{"type":"string","format":"uuid"},"card_id":{"type":"string","format":"uuid"},"author_id":{"type":["string","null"],"format":"uuid"},"body":{"type":"string"},"edited_at":{"type":["string","null"],"format":"date-time"},"created_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "CommentPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Comment"}},"next_cursor":{"type":["string","null"]}}}),
        ),
        Shape::new(
            "ActivityPageFilter",
            json!({"type":"object","properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"card_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "Activity",
            json!({"type":"object","required":["id","board_id","card_id","kind","actor_kind","actor_id","detail","created_at"],"properties":{"id":{"type":"string","format":"uuid"},"board_id":{"type":"string","format":"uuid"},"card_id":{"type":["string","null"],"format":"uuid"},"kind":{"type":"string"},"actor_kind":{"type":"string"},"actor_id":{"type":["string","null"]},"detail":{"type":"object","additionalProperties":true},"created_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "ActivityPage",
            json!({"type":"object","required":["items","next_cursor"],"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/Activity"}},"next_cursor":{"type":["string","null"]}}}),
        ),
    ]
}

impl BoardService {
    pub async fn create_board(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateBoard,
    ) -> Result<Board> {
        let name = bounded_text(&input.name, MAX_BOARD_NAME, "board_name_invalid")?;
        let description = optional_text(
            input.description.as_deref(),
            10_000,
            "board_description_invalid",
        )?;
        let id = BoardId::new();
        sqlx::query("insert into boards (site_id, id, name, description) values ($1, $2, $3, $4)")
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .bind(&name)
            .bind(description)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            id,
            None,
            "board.created",
            json!({"name": name}),
        )
        .await?;
        self.get_board(tx, id).await
    }

    pub async fn get_board(&self, tx: &mut SiteTx, id: BoardId) -> Result<Board> {
        let row = sqlx::query("select id, name, description, archived, created_at, updated_at from boards where id = $1 and deleted_at is null")
            .bind(id.into_uuid()).fetch_optional(tx.conn()).await.map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound { resource: "board" })?;
        board_row(&row, id)
    }

    pub async fn list_boards(
        &self,
        tx: &mut SiteTx,
        filter: &BoardListFilter,
    ) -> Result<Page<Board>> {
        let after = filter.page.after.as_ref().map(decode_recent).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, name, description, archived, created_at, updated_at from boards where deleted_at is null",
        );
        if let Some(archived) = filter.archived {
            query.push(" and archived = ").push_bind(archived);
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(|row| {
                board_row(
                    row,
                    BoardId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_recent(item.created_at, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn update_board(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardId,
        input: &UpdateBoard,
    ) -> Result<Board> {
        if input.name.is_none() && input.description.is_none() && input.archived.is_none() {
            return Err(MaviError::validation("board_update_empty"));
        }
        let name = input
            .name
            .as_deref()
            .map(|value| bounded_text(value, MAX_BOARD_NAME, "board_name_invalid"))
            .transpose()?;
        let description = input
            .description
            .as_ref()
            .map(|value| optional_text(value.as_deref(), 10_000, "board_description_invalid"))
            .transpose()?;
        let rows = sqlx::query("update boards set name = coalesce($2, name), description = case when $3 then $4 else description end, archived = coalesce($5, archived), updated_at = now() where id = $1 and deleted_at is null")
            .bind(id.into_uuid()).bind(name.as_deref()).bind(input.description.is_some()).bind(description.flatten()).bind(input.archived)
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() == 0 {
            return Err(MaviError::NotFound { resource: "board" });
        }
        self.mutate(
            tx,
            context,
            id,
            None,
            "board.updated",
            json!({"archived": input.archived}),
        )
        .await?;
        self.get_board(tx, id).await
    }

    pub async fn delete_board(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardId,
    ) -> Result<()> {
        let rows = sqlx::query("update boards set deleted_at = now(), archived = true, updated_at = now() where id = $1 and deleted_at is null")
            .bind(id.into_uuid()).execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() == 0 {
            return Err(MaviError::NotFound { resource: "board" });
        }
        sqlx::query("update board_lists set deleted_at = now(), updated_at = now() where board_id = $1 and deleted_at is null").bind(id.into_uuid()).execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        sqlx::query("update board_cards set archived_at = now(), updated_at = now() where board_id = $1 and archived_at is null").bind(id.into_uuid()).execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(tx, context, id, None, "board.deleted", json!({}))
            .await
    }

    pub async fn list_lists(
        &self,
        tx: &mut SiteTx,
        board_id: BoardId,
        filter: &ListPageFilter,
    ) -> Result<Page<BoardList>> {
        self.get_board(tx, board_id).await?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_position)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, board_id, name, position, created_at, updated_at from board_lists where board_id = ",
        );
        query
            .push_bind(board_id.into_uuid())
            .push(" and deleted_at is null");
        if let Some(after) = after {
            query
                .push(" and (position, id) > (")
                .push_bind(after.position)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by position asc, id asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(list_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_position(item.position, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        board_id: BoardId,
        input: &CreateList,
    ) -> Result<BoardList> {
        let name = bounded_text(&input.name, MAX_LIST_NAME, "board_list_name_invalid")?;
        let board = self.get_board(tx, board_id).await?;
        if board.archived {
            return Err(MaviError::conflict("board_archived"));
        }
        let position: i32 = sqlx::query_scalar("select coalesce(max(position), -1) + 1 from board_lists where board_id = $1 and deleted_at is null").bind(board_id.into_uuid()).fetch_one(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        let id = BoardListId::new();
        sqlx::query("insert into board_lists (site_id, id, board_id, name, position) values ($1, $2, $3, $4, $5)")
            .bind(context.site_id.into_uuid()).bind(id.into_uuid()).bind(board_id.into_uuid()).bind(&name).bind(position)
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            board_id,
            None,
            "board.list.created",
            json!({"list_id": id, "name": name}),
        )
        .await?;
        self.get_list(tx, id).await
    }

    pub async fn reorder_lists(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        board_id: BoardId,
        input: &ReorderLists,
    ) -> Result<Page<BoardList>> {
        let board = self.get_board(tx, board_id).await?;
        if board.archived {
            return Err(MaviError::conflict("board_archived"));
        }
        sqlx::query("select id from boards where id = $1 for update")
            .bind(board_id.into_uuid())
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let existing: Vec<Uuid> = sqlx::query_scalar("select id from board_lists where board_id = $1 and deleted_at is null order by position, id").bind(board_id.into_uuid()).fetch_all(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        let order = validate_order(
            &existing,
            input
                .order
                .iter()
                .map(|id| id.into_uuid())
                .collect::<Vec<_>>(),
        )?;
        for (position, id) in order.into_iter().enumerate() {
            sqlx::query("update board_lists set position = $2, updated_at = now() where id = $1")
                .bind(id)
                .bind(i32::try_from(position).map_err(|_| MaviError::Internal)?)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        }
        self.mutate(
            tx,
            context,
            board_id,
            None,
            "board.lists.reordered",
            json!({"count": input.order.len()}),
        )
        .await?;
        self.list_lists(tx, board_id, &ListPageFilter::default())
            .await
    }

    pub async fn get_list(&self, tx: &mut SiteTx, id: BoardListId) -> Result<BoardList> {
        let row = sqlx::query("select id, board_id, name, position, created_at, updated_at from board_lists where id = $1 and deleted_at is null").bind(id.into_uuid()).fetch_optional(tx.conn()).await.map_err(|_| MaviError::Internal)?.ok_or(MaviError::NotFound { resource: "board_list" })?;
        list_row(&row)
    }

    pub async fn list_cards(
        &self,
        tx: &mut SiteTx,
        list_id: BoardListId,
        filter: &CardPageFilter,
    ) -> Result<Page<Card>> {
        self.get_list(tx, list_id).await?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_position)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, board_id, list_id, title, description, assignee_id, position, created_at, updated_at from board_cards where list_id = ",
        );
        query
            .push_bind(list_id.into_uuid())
            .push(" and archived_at is null");
        if let Some(assignee) = filter.assignee_id {
            query
                .push(" and assignee_id = ")
                .push_bind(assignee.into_uuid());
        }
        if let Some(after) = after {
            query
                .push(" and (position, id) > (")
                .push_bind(after.position)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by position asc, id asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(card_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_position(item.position, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_card(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        list_id: BoardListId,
        input: &CreateCard,
    ) -> Result<Card> {
        let list = self.get_list(tx, list_id).await?;
        let board = self.get_board(tx, list.board_id).await?;
        if board.archived {
            return Err(MaviError::conflict("board_archived"));
        }
        let title = bounded_text(&input.title, MAX_CARD_TITLE, "card_title_invalid")?;
        let description = optional_text(
            input.description.as_deref(),
            MAX_CARD_DESCRIPTION,
            "card_description_invalid",
        )?;
        validate_assignee(tx, input.assignee_id).await?;
        let position: i32 = sqlx::query_scalar("select coalesce(max(position), -1) + 1 from board_cards where list_id = $1 and archived_at is null").bind(list_id.into_uuid()).fetch_one(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        let id = BoardCardId::new();
        sqlx::query("insert into board_cards (site_id, id, board_id, list_id, title, description, assignee_id, position) values ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(context.site_id.into_uuid()).bind(id.into_uuid()).bind(list.board_id.into_uuid()).bind(list_id.into_uuid()).bind(&title).bind(description).bind(input.assignee_id.map(PersonId::into_uuid)).bind(position)
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            list.board_id,
            Some(id),
            "board.card.created",
            json!({"list_id": list_id, "title": title}),
        )
        .await?;
        self.get_card(tx, id).await
    }

    pub async fn get_card(&self, tx: &mut SiteTx, id: BoardCardId) -> Result<Card> {
        let row = sqlx::query("select id, board_id, list_id, title, description, assignee_id, position, created_at, updated_at from board_cards where id = $1 and archived_at is null").bind(id.into_uuid()).fetch_optional(tx.conn()).await.map_err(|_| MaviError::Internal)?.ok_or(MaviError::NotFound { resource: "board_card" })?;
        card_row(&row)
    }

    pub async fn update_card(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardCardId,
        input: &UpdateCard,
    ) -> Result<Card> {
        if input.title.is_none() && input.description.is_none() {
            return Err(MaviError::validation("card_update_empty"));
        }
        let current = self.get_card(tx, id).await?;
        let title = input
            .title
            .as_deref()
            .map(|value| bounded_text(value, MAX_CARD_TITLE, "card_title_invalid"))
            .transpose()?;
        let description = input
            .description
            .as_ref()
            .map(|value| {
                optional_text(
                    value.as_deref(),
                    MAX_CARD_DESCRIPTION,
                    "card_description_invalid",
                )
            })
            .transpose()?;
        sqlx::query("update board_cards set title = coalesce($2, title), description = case when $3 then $4 else description end, updated_at = now() where id = $1 and archived_at is null")
            .bind(id.into_uuid()).bind(title.as_deref()).bind(input.description.is_some()).bind(description.flatten())
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            current.board_id,
            Some(id),
            "board.card.updated",
            json!({}),
        )
        .await?;
        self.get_card(tx, id).await
    }

    pub async fn move_card(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardCardId,
        input: &MoveCard,
    ) -> Result<Card> {
        let current = self.get_card(tx, id).await?;
        let board = self.get_board(tx, current.board_id).await?;
        if board.archived {
            return Err(MaviError::conflict("board_archived"));
        }
        sqlx::query("select id from boards where id = $1 for update")
            .bind(current.board_id.into_uuid())
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let target = self.get_list(tx, input.list_id).await?;
        if target.board_id != current.board_id {
            return Err(MaviError::validation("card_list_board_mismatch"));
        }
        let mut source_ids: Vec<Uuid> = sqlx::query_scalar("select id from board_cards where list_id = $1 and archived_at is null order by position, id").bind(current.list_id.into_uuid()).fetch_all(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        source_ids.retain(|value| *value != id.into_uuid());
        let mut target_ids = if target.id == current.list_id {
            source_ids.clone()
        } else {
            sqlx::query_scalar("select id from board_cards where list_id = $1 and archived_at is null order by position, id").bind(target.id.into_uuid()).fetch_all(tx.conn()).await.map_err(|_| MaviError::Internal)?
        };
        let insertion = input
            .before_card_id
            .map(|before| {
                target_ids
                    .iter()
                    .position(|value| *value == before.into_uuid())
                    .ok_or(MaviError::validation("card_before_not_in_list"))
            })
            .transpose()?
            .unwrap_or(target_ids.len());
        target_ids.insert(insertion, id.into_uuid());
        rewrite_card_positions(tx, current.list_id, &source_ids).await?;
        sqlx::query("update board_cards set list_id = $2, updated_at = now() where id = $1")
            .bind(id.into_uuid())
            .bind(target.id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        // The card's final list and position are written after the source
        // reindex so the source list never contains a duplicate card.
        rewrite_card_positions(tx, target.id, &target_ids).await?;
        self.mutate(tx, context, current.board_id, Some(id), "board.card.moved", json!({"from_list_id": current.list_id, "to_list_id": target.id, "before_card_id": input.before_card_id})).await?;
        self.get_card(tx, id).await
    }

    pub async fn assign_card(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardCardId,
        input: &AssignCard,
    ) -> Result<Card> {
        let current = self.get_card(tx, id).await?;
        validate_assignee(tx, input.assignee_id).await?;
        sqlx::query("update board_cards set assignee_id = $2, updated_at = now() where id = $1 and archived_at is null").bind(id.into_uuid()).bind(input.assignee_id.map(PersonId::into_uuid)).execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            current.board_id,
            Some(id),
            "board.card.assigned",
            json!({"assignee_id": input.assignee_id}),
        )
        .await?;
        self.get_card(tx, id).await
    }

    pub async fn list_comments(
        &self,
        tx: &mut SiteTx,
        card_id: BoardCardId,
        filter: &CommentPageFilter,
    ) -> Result<Page<Comment>> {
        let card = self.get_card(tx, card_id).await?;
        let after = filter.page.after.as_ref().map(decode_recent).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, board_id, card_id, author_id, body, edited_at, created_at from board_comments where card_id = ",
        );
        query
            .push_bind(card.id.into_uuid())
            .push(" and deleted_at is null");
        if let Some(after) = after {
            query
                .push(" and (created_at, id) > (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at asc, id asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(comment_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_recent(item.created_at, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_comment(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        card_id: BoardCardId,
        input: &CreateComment,
    ) -> Result<Comment> {
        let body = bounded_text(&input.body, MAX_COMMENT, "comment_body_invalid")?;
        let card = self.get_card(tx, card_id).await?;
        let id = BoardCommentId::new();
        let author = actor_person(context);
        sqlx::query("insert into board_comments (site_id, id, board_id, card_id, author_id, body) values ($1, $2, $3, $4, $5, $6)")
            .bind(context.site_id.into_uuid()).bind(id.into_uuid()).bind(card.board_id.into_uuid()).bind(card_id.into_uuid()).bind(author).bind(&body)
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            card.board_id,
            Some(card_id),
            "board.comment.created",
            json!({"comment_id": id}),
        )
        .await?;
        self.get_comment(tx, id).await
    }

    pub async fn get_comment(&self, tx: &mut SiteTx, id: BoardCommentId) -> Result<Comment> {
        let row = sqlx::query("select id, board_id, card_id, author_id, body, edited_at, created_at from board_comments where id = $1 and deleted_at is null").bind(id.into_uuid()).fetch_optional(tx.conn()).await.map_err(|_| MaviError::Internal)?.ok_or(MaviError::NotFound { resource: "board_comment" })?;
        comment_row(&row)
    }

    pub async fn update_comment(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardCommentId,
        input: &UpdateComment,
    ) -> Result<Comment> {
        let body = bounded_text(&input.body, MAX_COMMENT, "comment_body_invalid")?;
        let current = self.get_comment(tx, id).await?;
        if actor_person(context) != current.author_id.map(PersonId::into_uuid) {
            return Err(MaviError::Forbidden);
        }
        sqlx::query("update board_comments set body = $2, edited_at = now() where id = $1 and deleted_at is null").bind(id.into_uuid()).bind(&body).execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        self.mutate(
            tx,
            context,
            current.board_id,
            Some(current.card_id),
            "board.comment.updated",
            json!({"comment_id": id}),
        )
        .await?;
        self.get_comment(tx, id).await
    }

    pub async fn delete_comment(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: BoardCommentId,
    ) -> Result<()> {
        let current = self.get_comment(tx, id).await?;
        let rows = sqlx::query(
            "update board_comments set deleted_at = now() where id = $1 and deleted_at is null",
        )
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if rows.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "board_comment",
            });
        }
        self.mutate(
            tx,
            context,
            current.board_id,
            Some(current.card_id),
            "board.comment.deleted",
            json!({"comment_id": id}),
        )
        .await
    }

    pub async fn list_activity(
        &self,
        tx: &mut SiteTx,
        board_id: BoardId,
        filter: &ActivityPageFilter,
    ) -> Result<Page<Activity>> {
        self.get_board(tx, board_id).await?;
        let after = filter.page.after.as_ref().map(decode_recent).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = QueryBuilder::<Postgres>::new(
            "select id, board_id, card_id, kind, actor_kind, actor_id, detail, created_at from board_activity where board_id = ",
        );
        query.push_bind(board_id.into_uuid());
        if let Some(card_id) = filter.card_id {
            query.push(" and card_id = ").push_bind(card_id.into_uuid());
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(activity_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_recent(item.created_at, item.id)?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    async fn mutate(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        board_id: BoardId,
        card_id: Option<BoardCardId>,
        kind: &str,
        detail: Value,
    ) -> Result<()> {
        let actor = actor(context);
        let activity_id = Uuid::now_v7();
        sqlx::query("insert into board_activity (site_id, id, board_id, card_id, kind, actor_kind, actor_id, detail) values ($1, $2, $3, $4, $5, $6, $7, $8)")
            .bind(context.site_id.into_uuid()).bind(activity_id).bind(board_id.into_uuid()).bind(card_id.map(BoardCardId::into_uuid)).bind(kind).bind(actor.0).bind(actor.1).bind(&detail)
            .execute(tx.conn()).await.map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: kind.to_owned(),
                    resource_type: "Board".to_owned(),
                    resource_id: Some(board_id.into_uuid()),
                    payload: detail,
                },
            )
            .await
    }
}

async fn validate_assignee(tx: &mut SiteTx, assignee: Option<PersonId>) -> Result<()> {
    if let Some(assignee) = assignee {
        let exists: bool = sqlx::query_scalar(
            "select exists(select 1 from people where id = $1 and status = 'active')",
        )
        .bind(assignee.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if !exists {
            return Err(MaviError::NotFound { resource: "person" });
        }
    }
    Ok(())
}

async fn rewrite_card_positions(tx: &mut SiteTx, list_id: BoardListId, ids: &[Uuid]) -> Result<()> {
    for (position, id) in ids.iter().enumerate() {
        sqlx::query("update board_cards set position = $2 where id = $1 and list_id = $3")
            .bind(id)
            .bind(i32::try_from(position).map_err(|_| MaviError::Internal)?)
            .bind(list_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
    }
    Ok(())
}

fn validate_order(existing: &[Uuid], requested: Vec<Uuid>) -> Result<Vec<Uuid>> {
    if existing.len() != requested.len()
        || existing.iter().collect::<BTreeSet<_>>() != requested.iter().collect::<BTreeSet<_>>()
    {
        return Err(MaviError::validation("board_order_must_match_existing_ids"));
    }
    Ok(requested)
}

fn bounded_text(value: &str, max: usize, code: &'static str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(MaviError::validation(code));
    }
    Ok(value.to_owned())
}

fn optional_text(value: Option<&str>, max: usize, code: &'static str) -> Result<Option<String>> {
    value
        .map(|value| {
            if value.chars().count() > max {
                Err(MaviError::validation(code))
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

fn actor(context: &SiteContext) -> (&'static str, Option<String>) {
    match &context.caller {
        Caller::Public => ("public", None),
        Caller::Account { person_id, .. } => ("account", Some(person_id.to_string())),
        Caller::Assistant {
            key_id, person_id, ..
        } => (
            "assistant",
            person_id.map_or_else(|| Some(key_id.to_string()), |id| Some(id.to_string())),
        ),
        Caller::Student { student_id, .. } => ("student", Some(student_id.to_string())),
    }
}

fn actor_person(context: &SiteContext) -> Option<Uuid> {
    match &context.caller {
        Caller::Account { person_id, .. } => Some(person_id.into_uuid()),
        Caller::Assistant { person_id, .. } => person_id.map(PersonId::into_uuid),
        Caller::Public | Caller::Student { .. } => None,
    }
}

fn board_row(row: &sqlx::postgres::PgRow, id: BoardId) -> Result<Board> {
    Ok(Board {
        id,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        description: row
            .try_get("description")
            .map_err(|_| MaviError::Internal)?,
        archived: row.try_get("archived").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn list_row(row: &sqlx::postgres::PgRow) -> Result<BoardList> {
    Ok(BoardList {
        id: BoardListId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        board_id: BoardId::from_uuid(row.try_get("board_id").map_err(|_| MaviError::Internal)?),
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        position: row.try_get("position").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn card_row(row: &sqlx::postgres::PgRow) -> Result<Card> {
    Ok(Card {
        id: BoardCardId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        board_id: BoardId::from_uuid(row.try_get("board_id").map_err(|_| MaviError::Internal)?),
        list_id: BoardListId::from_uuid(row.try_get("list_id").map_err(|_| MaviError::Internal)?),
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        description: row
            .try_get("description")
            .map_err(|_| MaviError::Internal)?,
        assignee_id: row
            .try_get::<Option<Uuid>, _>("assignee_id")
            .map_err(|_| MaviError::Internal)?
            .map(PersonId::from_uuid),
        position: row.try_get("position").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn comment_row(row: &sqlx::postgres::PgRow) -> Result<Comment> {
    Ok(Comment {
        id: BoardCommentId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        board_id: BoardId::from_uuid(row.try_get("board_id").map_err(|_| MaviError::Internal)?),
        card_id: BoardCardId::from_uuid(row.try_get("card_id").map_err(|_| MaviError::Internal)?),
        author_id: row
            .try_get::<Option<Uuid>, _>("author_id")
            .map_err(|_| MaviError::Internal)?
            .map(PersonId::from_uuid),
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        edited_at: row.try_get("edited_at").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

fn activity_row(row: &sqlx::postgres::PgRow) -> Result<Activity> {
    Ok(Activity {
        id: row.try_get("id").map_err(|_| MaviError::Internal)?,
        board_id: BoardId::from_uuid(row.try_get("board_id").map_err(|_| MaviError::Internal)?),
        card_id: row
            .try_get::<Option<Uuid>, _>("card_id")
            .map_err(|_| MaviError::Internal)?
            .map(BoardCardId::from_uuid),
        kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
        actor_kind: row.try_get("actor_kind").map_err(|_| MaviError::Internal)?,
        actor_id: row.try_get("actor_id").map_err(|_| MaviError::Internal)?,
        detail: row.try_get("detail").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RecentCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PositionCursor {
    position: i32,
    id: Uuid,
}

fn encode_recent(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    Cursor::parse(URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?,
    ))
}
fn decode_recent(cursor: &Cursor) -> Result<RecentCursor> {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .map_err(|_| MaviError::validation("invalid_cursor"))?,
    )
    .map_err(|_| MaviError::validation("invalid_cursor"))
}
fn encode_position(position: i32, id: Uuid) -> Result<Cursor> {
    Cursor::parse(URL_SAFE_NO_PAD.encode(
        serde_json::to_vec(&PositionCursor { position, id }).map_err(|_| MaviError::Internal)?,
    ))
}
fn decode_position(cursor: &Cursor) -> Result<PositionCursor> {
    serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(cursor.as_str())
            .map_err(|_| MaviError::validation("invalid_cursor"))?,
    )
    .map_err(|_| MaviError::validation("invalid_cursor"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_requires_exactly_the_existing_ids() {
        let first = Uuid::now_v7();
        let second = Uuid::now_v7();
        assert!(validate_order(&[first, second], vec![second, first]).is_ok());
        assert!(validate_order(&[first, second], vec![first]).is_err());
        assert!(validate_order(&[first, second], vec![first, first]).is_err());
    }

    #[test]
    fn board_contracts_are_cursor_only() {
        let api = api();
        for name in [
            "BoardListFilter",
            "ListPageFilter",
            "CardPageFilter",
            "CommentPageFilter",
            "ActivityPageFilter",
        ] {
            let shape = shapes()
                .into_iter()
                .find(|shape| shape.name == name)
                .expect("filter shape");
            let properties = shape.schema["properties"].as_object().expect("properties");
            assert!(properties.contains_key("after"));
            assert!(properties.contains_key("limit"));
            assert!(!properties.contains_key("offset"));
            assert!(!properties.contains_key("page"));
        }
        api.validate().expect("board API");
    }
}
