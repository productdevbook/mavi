use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use super::BoardService;

pub const BOARDS_RELOCATION_FORMAT: &str = "mavi.boards.relocation";
pub const BOARDS_RELOCATION_VERSION: u16 = 1;
pub const MAX_BOARDS_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_BOARDS_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardsRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub boards: Vec<BoardRelocation>,
    pub lists: Vec<BoardListRelocation>,
    pub cards: Vec<BoardCardRelocation>,
    pub comments: Vec<BoardCommentRelocation>,
    pub activity: Vec<BoardActivityRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardRelocation {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardListRelocation {
    pub id: Uuid,
    pub board_id: Uuid,
    pub name: String,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCardRelocation {
    pub id: Uuid,
    pub board_id: Uuid,
    pub list_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<Uuid>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardCommentRelocation {
    pub id: Uuid,
    pub board_id: Uuid,
    pub card_id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub edited_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoardActivityRelocation {
    pub id: Uuid,
    pub board_id: Uuid,
    pub card_id: Option<Uuid>,
    pub kind: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

impl BoardsRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: BOARDS_RELOCATION_FORMAT.to_owned(),
            version: BOARDS_RELOCATION_VERSION,
            source_site_id,
            boards: Vec::new(),
            lists: Vec::new(),
            cards: Vec::new(),
            comments: Vec::new(),
            activity: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != BOARDS_RELOCATION_FORMAT {
            return Err(MaviError::validation("boards_relocation_format_invalid"));
        }
        if self.version != BOARDS_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "boards_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("boards_relocation_site_mismatch"));
        }
        let sections = [
            self.boards.len(),
            self.lists.len(),
            self.cards.len(),
            self.comments.len(),
            self.activity.len(),
        ];
        let total = sections
            .iter()
            .try_fold(0usize, |total, count| total.checked_add(*count))
            .ok_or_else(|| MaviError::validation("boards_relocation_count_overflow"))?;
        if total > MAX_BOARDS_RELOCATION_RECORDS
            || sections
                .iter()
                .any(|count| *count > MAX_BOARDS_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("boards_relocation_counts_invalid"));
        }

        let mut board_ids = BTreeSet::new();
        let mut board_names = BTreeSet::new();
        for board in &self.boards {
            if board.id.is_nil()
                || !board_ids.insert(board.id)
                || !valid_text(&board.name, 200)
                || !board
                    .description
                    .as_deref()
                    .is_none_or(|value| valid_text(value, 10_000))
                || (board.deleted_at.is_none()
                    && !board_names.insert(board.name.to_ascii_lowercase()))
            {
                return Err(MaviError::validation("boards_relocation_board_invalid"));
            }
        }

        let mut list_ids = BTreeSet::new();
        let mut list_positions = BTreeSet::new();
        for list in &self.lists {
            if list.id.is_nil()
                || !list_ids.insert(list.id)
                || !board_ids.contains(&list.board_id)
                || !valid_text(&list.name, 120)
                || list.position < 0
                || !list_positions.insert((list.board_id, list.position))
            {
                return Err(MaviError::validation("boards_relocation_list_invalid"));
            }
        }

        let mut card_ids = BTreeSet::new();
        let mut card_positions = BTreeSet::new();
        for card in &self.cards {
            let list_matches = self
                .lists
                .iter()
                .any(|list| list.id == card.list_id && list.board_id == card.board_id);
            if card.id.is_nil()
                || !card_ids.insert(card.id)
                || !board_ids.contains(&card.board_id)
                || !list_matches
                || !valid_text(&card.title, 300)
                || !card
                    .description
                    .as_deref()
                    .is_none_or(|value| valid_text(value, 20_000))
                || card.position < 0
                || !card_positions.insert((card.list_id, card.position))
            {
                return Err(MaviError::validation("boards_relocation_card_invalid"));
            }
        }

        let mut comment_ids = BTreeSet::new();
        for comment in &self.comments {
            let card_matches = self
                .cards
                .iter()
                .any(|card| card.id == comment.card_id && card.board_id == comment.board_id);
            if comment.id.is_nil()
                || !comment_ids.insert(comment.id)
                || !board_ids.contains(&comment.board_id)
                || !card_matches
                || !valid_text(&comment.body, 10_000)
            {
                return Err(MaviError::validation("boards_relocation_comment_invalid"));
            }
        }

        let mut activity_ids = BTreeSet::new();
        for activity in &self.activity {
            let card_matches = activity.card_id.is_none_or(|card_id| {
                self.cards
                    .iter()
                    .any(|card| card.id == card_id && card.board_id == activity.board_id)
            });
            if activity.id.is_nil()
                || !activity_ids.insert(activity.id)
                || !board_ids.contains(&activity.board_id)
                || !card_matches
                || activity.kind.trim().is_empty()
                || activity.kind.chars().count() > 120
                || !matches!(
                    activity.actor_kind.as_str(),
                    "public" | "account" | "assistant" | "student" | "system"
                )
                || activity
                    .actor_id
                    .as_deref()
                    .is_some_and(|value| value.chars().count() > 512)
                || !activity.detail.is_object()
            {
                return Err(MaviError::validation("boards_relocation_activity_invalid"));
            }
        }

        if serde_json::to_vec(self)
            .map_err(|_| MaviError::Internal)?
            .len()
            > MAX_BOARDS_RELOCATION_BYTES
        {
            return Err(MaviError::validation("boards_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .boards
            .len()
            .checked_add(self.lists.len())
            .and_then(|value| value.checked_add(self.cards.len()))
            .and_then(|value| value.checked_add(self.comments.len()))
            .and_then(|value| value.checked_add(self.activity.len()))
            .ok_or_else(|| MaviError::validation("boards_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("boards_relocation_count_overflow"))
    }
}

impl BoardService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<BoardsRelocation> {
        let site_id = context.site_id.into_uuid();
        let boards = sqlx::query(
            "select id, name, description, archived, created_at, updated_at, deleted_at
               from boards where site_id = $1 order by created_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(BoardRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                description: row
                    .try_get("description")
                    .map_err(|_| MaviError::Internal)?,
                archived: row.try_get("archived").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let lists = sqlx::query(
            "select id, board_id, name, position, created_at, updated_at, deleted_at
               from board_lists where site_id = $1 order by board_id, position, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(BoardListRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                board_id: row.try_get("board_id").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let cards = sqlx::query(
            "select id, board_id, list_id, title, description, assignee_id, position,
                    created_at, updated_at, archived_at
               from board_cards where site_id = $1 order by list_id, position, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(BoardCardRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                board_id: row.try_get("board_id").map_err(|_| MaviError::Internal)?,
                list_id: row.try_get("list_id").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                description: row
                    .try_get("description")
                    .map_err(|_| MaviError::Internal)?,
                assignee_id: row
                    .try_get("assignee_id")
                    .map_err(|_| MaviError::Internal)?,
                position: row.try_get("position").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                archived_at: row
                    .try_get("archived_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let comments = sqlx::query(
            "select id, board_id, card_id, author_id, body, edited_at, created_at, deleted_at
               from board_comments where site_id = $1 order by created_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(BoardCommentRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                board_id: row.try_get("board_id").map_err(|_| MaviError::Internal)?,
                card_id: row.try_get("card_id").map_err(|_| MaviError::Internal)?,
                author_id: row.try_get("author_id").map_err(|_| MaviError::Internal)?,
                body: row.try_get("body").map_err(|_| MaviError::Internal)?,
                edited_at: row.try_get("edited_at").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let activity = sqlx::query(
            "select id, board_id, card_id, kind, actor_kind, actor_id, detail, created_at
               from board_activity where site_id = $1 order by created_at, id",
        )
        .bind(site_id)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(BoardActivityRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                board_id: row.try_get("board_id").map_err(|_| MaviError::Internal)?,
                card_id: row.try_get("card_id").map_err(|_| MaviError::Internal)?,
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                actor_kind: row.try_get("actor_kind").map_err(|_| MaviError::Internal)?,
                actor_id: row.try_get("actor_id").map_err(|_| MaviError::Internal)?,
                detail: row.try_get("detail").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let relocation = BoardsRelocation {
            format: BOARDS_RELOCATION_FORMAT.to_owned(),
            version: BOARDS_RELOCATION_VERSION,
            source_site_id: context.site_id,
            boards,
            lists,
            cards,
            comments,
            activity,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &BoardsRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;
        let site_id = context.site_id.into_uuid();
        for table in [
            "board_activity",
            "board_comments",
            "board_cards",
            "board_lists",
            "boards",
        ] {
            let statement = match table {
                "board_activity" => "delete from board_activity where site_id = $1",
                "board_comments" => "delete from board_comments where site_id = $1",
                "board_cards" => "delete from board_cards where site_id = $1",
                "board_lists" => "delete from board_lists where site_id = $1",
                "boards" => "delete from boards where site_id = $1",
                _ => return Err(MaviError::Internal),
            };
            sqlx::query(statement)
                .bind(site_id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        }
        for board in &relocation.boards {
            sqlx::query(
                "insert into boards
                    (site_id, id, name, description, archived, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(site_id)
            .bind(board.id)
            .bind(&board.name)
            .bind(&board.description)
            .bind(board.archived)
            .bind(board.created_at)
            .bind(board.updated_at)
            .bind(board.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for list in &relocation.lists {
            sqlx::query(
                "insert into board_lists
                    (site_id, id, board_id, name, position, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(site_id)
            .bind(list.id)
            .bind(list.board_id)
            .bind(&list.name)
            .bind(list.position)
            .bind(list.created_at)
            .bind(list.updated_at)
            .bind(list.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for card in &relocation.cards {
            sqlx::query(
                "insert into board_cards
                    (site_id, id, board_id, list_id, title, description, assignee_id,
                     position, created_at, updated_at, archived_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
            )
            .bind(site_id)
            .bind(card.id)
            .bind(card.board_id)
            .bind(card.list_id)
            .bind(&card.title)
            .bind(&card.description)
            .bind(card.assignee_id)
            .bind(card.position)
            .bind(card.created_at)
            .bind(card.updated_at)
            .bind(card.archived_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for comment in &relocation.comments {
            sqlx::query(
                "insert into board_comments
                    (site_id, id, board_id, card_id, author_id, body, edited_at, created_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(comment.id)
            .bind(comment.board_id)
            .bind(comment.card_id)
            .bind(comment.author_id)
            .bind(&comment.body)
            .bind(comment.edited_at)
            .bind(comment.created_at)
            .bind(comment.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        for activity in &relocation.activity {
            sqlx::query(
                "insert into board_activity
                    (site_id, id, board_id, card_id, kind, actor_kind, actor_id, detail, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(site_id)
            .bind(activity.id)
            .bind(activity.board_id)
            .bind(activity.card_id)
            .bind(&activity.kind)
            .bind(&activity.actor_kind)
            .bind(&activity.actor_id)
            .bind(&activity.detail)
            .bind(activity.created_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.boards.relocated".to_owned(),
                    resource_type: "BoardsSnapshot".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "boards": relocation.boards.len(),
                        "lists": relocation.lists.len(),
                        "cards": relocation.cards.len(),
                        "comments": relocation.comments.len(),
                        "activity": relocation.activity.len(),
                        "activity_immutable": true,
                    }),
                },
            )
            .await
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}
