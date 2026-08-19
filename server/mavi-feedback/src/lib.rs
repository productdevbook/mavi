//! Site-scoped product feedback and problem reports.
//!
//! Reports are ordinary domain records, not a browser-only toast. Creation is
//! available to an authenticated account or assistant with the feedback write
//! grant; reading the inbox is an explicit feedback view permission. Every
//! report is written with an immutable audit receipt in the same transaction.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Caller, Capability, Cursor, ErrorCode, FeedbackReportId, MaviError, Page, PageRequest,
    Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use uuid::Uuid;

pub const MAX_TITLE: usize = 300;
pub const MAX_BODY: usize = 20_000;
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    Broken,
    Missing,
    Wanted,
}

impl ReportKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Broken => "broken",
            Self::Missing => "missing",
            Self::Wanted => "wanted",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportState {
    Open,
    Closed,
}

impl ReportState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateReport {
    pub kind: ReportKind,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default = "empty_context")]
    pub context: Value,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<ReportState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub id: FeedbackReportId,
    pub reporter_kind: String,
    pub kind: ReportKind,
    pub title: String,
    pub body: String,
    pub context: Value,
    pub state: ReportState,
    pub answer: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ReportCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    let write = Permission {
        capability: Capability::Feedback,
        action: Action::Write,
    };
    let view = Permission {
        capability: Capability::Feedback,
        action: Action::View,
    };

    mavi_contract::Api::new([
        Endpoint::new(
            Method::Post,
            "/api/v1/feedback/reports",
            "feedback.reports.create",
            "Create a site feedback report",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateReport")
        .returns(201, "FeedbackReport")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/feedback/reports",
            "feedback.reports.list",
            "List site feedback reports with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("ReportListFilter")
        .returns(200, "FeedbackReportPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "ReportKind",
            json!({"type":"string","enum":["broken","missing","wanted"]}),
        ),
        Shape::new(
            "ReportState",
            json!({"type":"string","enum":["open","closed"]}),
        ),
        Shape::new(
            "CreateReport",
            json!({"type":"object","required":["kind","title"],"additionalProperties":false,"properties":{"kind":{"$ref":"#/components/schemas/ReportKind"},"title":{"type":"string","minLength":1,"maxLength":300},"body":{"type":"string","maxLength":20000},"context":{"type":"object","additionalProperties":true}}}),
        ),
        Shape::new(
            "ReportListFilter",
            json!({"type":"object","additionalProperties":false,"properties":{"after":{"type":["string","null"],"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"state":{"$ref":"#/components/schemas/ReportState"}}}),
        ),
        Shape::new(
            "FeedbackReport",
            json!({"type":"object","required":["id","reporter_kind","kind","title","body","context","state","answer","created_at","updated_at"],"additionalProperties":false,"properties":{"id":{"type":"string","format":"uuid"},"reporter_kind":{"type":"string","enum":["account","assistant"]},"kind":{"$ref":"#/components/schemas/ReportKind"},"title":{"type":"string"},"body":{"type":"string"},"context":{"type":"object","additionalProperties":true},"state":{"$ref":"#/components/schemas/ReportState"},"answer":{"type":["string","null"]},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "FeedbackReportPage",
            json!({"type":"object","required":["items","next_cursor"],"additionalProperties":false,"properties":{"items":{"type":"array","items":{"$ref":"#/components/schemas/FeedbackReport"}},"next_cursor":{"type":["string","null"]}}}),
        ),
    ]
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FeedbackService;

impl FeedbackService {
    pub async fn create(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateReport,
    ) -> Result<Report> {
        let title = bounded_text(&input.title, MAX_TITLE, "feedback_title_invalid")?;
        let body = bounded_text_allow_empty(&input.body, MAX_BODY, "feedback_body_invalid")?;
        let context_value = validate_context(&input.context)?;
        let (reporter_kind, reporter_id) = reporter(context)?;
        let id = FeedbackReportId::new();

        sqlx::query(
            "insert into feedback_reports
                (site_id, id, reporter_kind, reporter_id, kind, title, body, context)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(reporter_kind)
        .bind(&reporter_id)
        .bind(input.kind.as_str())
        .bind(&title)
        .bind(&body)
        .bind(&context_value)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "feedback.report.created".to_owned(),
                    resource_type: "FeedbackReport".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"kind": input.kind, "title": title}),
                },
            )
            .await?;

        self.get(tx, id).await
    }

    pub async fn list(&self, tx: &mut SiteTx, filter: &ReportListFilter) -> Result<Page<Report>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, reporter_kind, kind, title, body, context, state, answer, created_at, updated_at
             from feedback_reports where true",
        );
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
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
        let mut items = rows.iter().map(report_row).collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let item = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(item.created_at, item.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get(&self, tx: &mut SiteTx, id: FeedbackReportId) -> Result<Report> {
        let row = sqlx::query(
            "select id, reporter_kind, kind, title, body, context, state, answer, created_at, updated_at
             from feedback_reports where id = $1",
        )
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "feedback_report",
        })?;
        report_row(&row)
    }
}

fn reporter(context: &SiteContext) -> Result<(&'static str, String)> {
    match &context.caller {
        Caller::Account { person_id, .. } => Ok(("account", person_id.to_string())),
        Caller::Assistant { key_id, .. } => Ok(("assistant", key_id.to_string())),
        Caller::Public | Caller::Student { .. } | Caller::System { .. } => {
            Err(MaviError::Forbidden)
        }
    }
}

fn report_row(row: &sqlx::postgres::PgRow) -> Result<Report> {
    let kind: String = row.try_get("kind").map_err(|_| MaviError::Internal)?;
    let state: String = row.try_get("state").map_err(|_| MaviError::Internal)?;
    Ok(Report {
        id: FeedbackReportId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        reporter_kind: row
            .try_get("reporter_kind")
            .map_err(|_| MaviError::Internal)?,
        kind: parse_kind(&kind)?,
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        context: row.try_get("context").map_err(|_| MaviError::Internal)?,
        state: parse_state(&state)?,
        answer: row.try_get("answer").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn parse_kind(value: &str) -> Result<ReportKind> {
    match value {
        "broken" => Ok(ReportKind::Broken),
        "missing" => Ok(ReportKind::Missing),
        "wanted" => Ok(ReportKind::Wanted),
        _ => Err(MaviError::Internal),
    }
}

fn parse_state(value: &str) -> Result<ReportState> {
    match value {
        "open" => Ok(ReportState::Open),
        "closed" => Ok(ReportState::Closed),
        _ => Err(MaviError::Internal),
    }
}

fn bounded_text(value: &str, max: usize, code: &'static str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max || trimmed.chars().any(char::is_control)
    {
        return Err(MaviError::validation(code));
    }
    Ok(trimmed.to_owned())
}

fn bounded_text_allow_empty(value: &str, max: usize, code: &'static str) -> Result<String> {
    if value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(MaviError::validation(code));
    }
    Ok(value.trim().to_owned())
}

fn validate_context(value: &Value) -> Result<Value> {
    if !value.is_object() {
        return Err(MaviError::validation("feedback_context_invalid"));
    }
    let bytes =
        serde_json::to_vec(value).map_err(|_| MaviError::validation("feedback_context_invalid"))?;
    if bytes.len() > MAX_CONTEXT_BYTES {
        return Err(MaviError::validation("feedback_context_too_large"));
    }
    Ok(value.clone())
}

fn empty_context() -> Value {
    json!({})
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let payload =
        serde_json::to_vec(&ReportCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(payload))
}

fn decode_cursor(value: &Cursor) -> Result<ReportCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value.as_str())
        .map_err(|_| MaviError::validation("feedback_cursor_invalid"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("feedback_cursor_invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_bounded_and_object_only() {
        assert!(validate_context(&json!({"screen":"/dashboard"})).is_ok());
        assert!(validate_context(&json!([])).is_err());
        assert!(validate_context(&json!({"value":"x".repeat(MAX_CONTEXT_BYTES)})).is_err());
    }

    #[test]
    fn cursors_round_trip() {
        let created_at = Utc::now();
        let id = Uuid::now_v7();
        let cursor = encode_cursor(created_at, id).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded cursor");
        assert_eq!(decoded.id, id);
        assert_eq!(decoded.created_at, created_at);
    }
}
