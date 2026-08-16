//! Audit receipts for all state-changing application services.

use mavi_core::{Caller, MaviError, Result, SiteContext};
use mavi_storage::SiteTx;
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct AuditEntry {
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuditService;

impl AuditService {
    /// Writes the receipt in the caller's existing transaction. This must be
    /// called before the domain transaction commits.
    pub async fn record(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        entry: &AuditEntry,
    ) -> Result<()> {
        let (actor_kind, actor_id) = actor(context);
        sqlx::query(
            "insert into audit_events
                (site_id, id, request_id, actor_kind, actor_id, action, resource_type, resource_id, payload)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(context.site_id.into_uuid())
        .bind(Uuid::now_v7())
        .bind(context.request_id.into_uuid())
        .bind(actor_kind)
        .bind(actor_id)
        .bind(&entry.action)
        .bind(&entry.resource_type)
        .bind(entry.resource_id)
        .bind(&entry.payload)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        Ok(())
    }
}

fn actor(context: &SiteContext) -> (&'static str, Option<String>) {
    match &context.caller {
        Caller::Public => ("public", None),
        Caller::Account { person_id, .. } => ("account", Some(person_id.to_string())),
        Caller::Student { student_id } => ("student", Some(student_id.to_string())),
        Caller::Assistant { key_id, .. } => ("assistant", Some(key_id.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::{ContentId, SiteId};

    #[test]
    fn public_audit_actor_is_not_fabricated_as_an_account() {
        let context = SiteContext::public(SiteId::new());
        assert_eq!(actor(&context), ("public", None));
    }

    #[test]
    fn content_id_can_be_recorded_as_a_uuid_resource() {
        let id = ContentId::new();
        let entry = AuditEntry {
            action: "content.created".to_owned(),
            resource_type: "Content".to_owned(),
            resource_id: Some(id.into_uuid()),
            payload: Value::Object(serde_json::Map::new()),
        };
        assert_eq!(entry.resource_id, Some(id.into_uuid()));
    }
}
