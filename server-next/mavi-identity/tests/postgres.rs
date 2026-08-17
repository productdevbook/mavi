use std::env;

use chrono::Utc;
use mavi_core::{
    Action, Caller, Capability, Grant, Grants, MaviError, RequestId, SiteContext, SiteId,
};
use mavi_identity::{
    CreatePerson, CreateRole, IdentityService, PeopleListFilter, PersonStatus, ReplaceRoleGrants,
    RoleListFilter, SetupInput, UpdatePersonStatus,
};
use mavi_storage::Database;

fn owner_grants() -> Grants {
    Grants::new(Capability::ALL.into_iter().flat_map(|capability| {
        Action::ALL
            .into_iter()
            .map(move |action| Grant::new(capability, action))
    }))
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn identity_people_and_roles_are_site_scoped_and_audited() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let public_context = SiteContext::public(site_id);
    let service = IdentityService;

    let mut setup_tx = database.begin(&public_context).await.expect("setup scope");
    let owner = service
        .initialize(
            &mut setup_tx,
            &public_context,
            &SetupInput {
                site_name: "Identity test".to_owned(),
                email: "owner@example.com".to_owned(),
                name: "Owner".to_owned(),
                password: "long-enough-password".to_owned(),
            },
        )
        .await
        .expect("setup");
    setup_tx.commit().await.expect("setup commit");

    let mut owner_lookup_tx = database
        .begin(&public_context)
        .await
        .expect("owner lookup scope");
    assert_eq!(
        service
            .owner_email(&mut owner_lookup_tx, &public_context)
            .await
            .expect("owner email"),
        Some("owner@example.com".to_owned())
    );
    owner_lookup_tx.commit().await.expect("owner lookup commit");

    let owner_context = SiteContext::with_caller(
        site_id,
        Caller::Account {
            person_id: owner.id,
            session_id: None,
            grants: owner_grants(),
        },
        RequestId::new(),
    );
    let mut tx = database.begin(&owner_context).await.expect("owner scope");
    let role = service
        .create_role(
            &mut tx,
            &owner_context,
            &CreateRole {
                name: "editor".to_owned(),
                grants: vec![
                    Grant::new(Capability::Content, Action::View),
                    Grant::new(Capability::Content, Action::Write),
                ],
            },
        )
        .await
        .expect("role");
    let restricted_context = SiteContext::with_caller(
        site_id,
        Caller::Account {
            person_id: owner.id,
            session_id: None,
            grants: Grants::new([Grant::new(Capability::People, Action::Write)]),
        },
        RequestId::new(),
    );
    let delegation_error = service
        .create_person(
            &mut tx,
            &restricted_context,
            &CreatePerson {
                email: "delegation@example.com".to_owned(),
                name: "Delegation".to_owned(),
                password: "long-enough-password".to_owned(),
                role_ids: vec![role.id],
            },
        )
        .await
        .expect_err("a caller cannot assign grants it does not hold");
    assert!(matches!(delegation_error, MaviError::Forbidden));

    let person = service
        .create_person(
            &mut tx,
            &owner_context,
            &CreatePerson {
                email: "editor@example.com".to_owned(),
                name: "Editor".to_owned(),
                password: "long-enough-password".to_owned(),
                role_ids: vec![role.id],
            },
        )
        .await
        .expect("person");
    assert_eq!(person.role_ids, vec![role.id]);

    let people = service
        .list_people(&mut tx, &owner_context, &PeopleListFilter::default())
        .await
        .expect("people");
    assert!(people.items.iter().any(|item| item.id == person.id));

    let roles = service
        .list_roles(&mut tx, &owner_context, &RoleListFilter::default())
        .await
        .expect("roles");
    assert!(roles.items.iter().any(|item| item.id == role.id));

    let updated_role = service
        .replace_role_grants(
            &mut tx,
            &owner_context,
            role.id,
            &ReplaceRoleGrants {
                grants: vec![Grant::new(Capability::Content, Action::View)],
            },
        )
        .await
        .expect("replace grants");
    assert_eq!(updated_role.grants.as_slice().len(), 1);

    let suspended = service
        .update_person_status(
            &mut tx,
            &owner_context,
            person.id,
            &UpdatePersonStatus {
                status: PersonStatus::Suspended,
            },
            Utc::now(),
        )
        .await
        .expect("suspend");
    assert_eq!(suspended.status, PersonStatus::Suspended);

    let audit_count: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and resource_type in ('Person', 'Role')",
    )
    .bind(site_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .expect("audit count");
    assert!(audit_count >= 3);
    tx.commit().await.expect("owner commit");
}
