use std::collections::BTreeMap;

use mavi_core::{
    Action, Caller, Capability, Grant, Grants, PersonId, RequestId, SiteContext, SiteId,
};
use mavi_sealing::KeyringSealer;
use mavi_secrets::{CreateCredential, CredentialListFilter, CredentialService, RotateCredential};
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn credentials_are_sealed_site_scoped_optimistic_and_audited() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");
    let first_context = account_context(first);
    let second_context = account_context(second);
    let sealer = KeyringSealer::from_key([11; 32]);
    let transfer_sealer = KeyringSealer::from_key([12; 32]);
    let target_sealer = KeyringSealer::from_key([13; 32]);
    let wrong_transfer_sealer = KeyringSealer::from_key([14; 32]);
    let service = CredentialService;

    let mut values = BTreeMap::new();
    values.insert("api_key".to_owned(), "secret-value".to_owned());
    values.insert(
        "endpoint".to_owned(),
        "https://mail.example.test".to_owned(),
    );

    let mut tx = database.begin(&first_context).await.expect("scope");
    let created = service
        .create(
            &mut tx,
            &first_context,
            &sealer,
            &CreateCredential {
                provider: "mail".to_owned(),
                name: "primary".to_owned(),
                values: values.clone(),
            },
        )
        .await
        .expect("create");
    assert_eq!(created.version, 1);
    tx.commit().await.expect("create commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    let listed = service
        .list(&mut tx, &first_context, &CredentialListFilter::default())
        .await
        .expect("list");
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].id, created.id);
    let material = service
        .unseal(&mut tx, &first_context, &sealer, created.id)
        .await
        .expect("unseal");
    assert_eq!(material.values(), &values);
    tx.commit().await.expect("read commit");

    let mut rotated = BTreeMap::new();
    rotated.insert("api_key".to_owned(), "rotated-value".to_owned());
    let mut tx = database.begin(&first_context).await.expect("scope");
    let updated = service
        .rotate(
            &mut tx,
            &first_context,
            &sealer,
            created.id,
            &RotateCredential {
                expected_version: 1,
                values: rotated.clone(),
            },
        )
        .await
        .expect("rotate");
    assert_eq!(updated.version, 2);
    tx.commit().await.expect("rotate commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    let conflict = service
        .rotate(
            &mut tx,
            &first_context,
            &sealer,
            created.id,
            &RotateCredential {
                expected_version: 1,
                values: rotated.clone(),
            },
        )
        .await
        .expect_err("stale version");
    assert!(matches!(conflict, mavi_core::MaviError::Conflict { .. }));
    drop(tx);

    let mut tx = database.begin(&first_context).await.expect("scope");
    let relocation = service
        .export_for_relocation(&mut tx, &first_context, &sealer, &transfer_sealer)
        .await
        .expect("export provider credentials");
    assert_eq!(relocation.record_count(), 1);
    assert!(!format!("{relocation:?}").contains("rotated-value"));
    tx.commit().await.expect("relocation export commit");

    let mut tx = database.begin(&second_context).await.expect("scope");
    let second_list = service
        .list(&mut tx, &second_context, &CredentialListFilter::default())
        .await
        .expect("second list");
    assert!(second_list.items.is_empty());
    tx.commit().await.expect("second commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    let mismatch = service
        .import_for_relocation(
            &mut tx,
            &first_context,
            &target_sealer,
            &wrong_transfer_sealer,
            &relocation,
        )
        .await
        .expect_err("wrong transfer key");
    assert!(matches!(
        mismatch,
        mavi_core::MaviError::Conflict { ref code }
            if code == "credential_relocation_key_mismatch"
    ));
    drop(tx);

    let mut tx = database.begin(&first_context).await.expect("scope");
    assert_eq!(
        service
            .import_for_relocation(
                &mut tx,
                &first_context,
                &target_sealer,
                &transfer_sealer,
                &relocation,
            )
            .await
            .expect("import provider credentials"),
        1
    );
    let target_material = service
        .unseal(&mut tx, &first_context, &target_sealer, created.id)
        .await
        .expect("target unseal");
    assert_eq!(
        target_material.values().get("api_key"),
        Some(&"rotated-value".to_owned())
    );
    tx.commit().await.expect("relocation import commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    service
        .revoke(&mut tx, &first_context, created.id)
        .await
        .expect("revoke");
    tx.commit().await.expect("revoke commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    let revoked = service
        .unseal(&mut tx, &first_context, &sealer, created.id)
        .await
        .expect_err("revoked credential");
    assert!(matches!(revoked, mavi_core::MaviError::Conflict { .. }));
    tx.commit().await.expect("revoked read commit");

    let mut tx = database.begin(&first_context).await.expect("scope");
    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and resource_type = 'Credential'",
    )
    .bind(first.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("audit count");
    assert_eq!(audit_count, 5);
    tx.commit().await.expect("audit commit");
}

fn account_context(site_id: SiteId) -> SiteContext {
    SiteContext::with_caller(
        site_id,
        Caller::Account {
            person_id: PersonId::new(),
            session_id: None,
            grants: Grants::new([
                Grant::new(Capability::Credentials, Action::View),
                Grant::new(Capability::Credentials, Action::Write),
                Grant::new(Capability::Credentials, Action::Delete),
            ]),
        },
        RequestId::new(),
    )
}
