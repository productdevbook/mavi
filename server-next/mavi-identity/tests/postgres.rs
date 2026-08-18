use std::env;

use chrono::Utc;
use mavi_core::{
    Action, Caller, Capability, Grant, Grants, MaviError, RequestId, SiteContext, SiteId,
};
use mavi_identity::{
    CreatePerson, CreateRole, EmailVerificationRedeemInput, EmailVerificationRequestInput,
    IdentityService, LoginInput, PasswordResetRedeemInput, PasswordResetRequestInput,
    PeopleListFilter, PersonStatus, ReplaceRoleGrants, RoleListFilter, SetupInput,
    UpdatePersonStatus,
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

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn password_reset_is_generic_one_time_and_revokes_sessions() {
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
    service
        .initialize(
            &mut setup_tx,
            &public_context,
            &SetupInput {
                site_name: "Password reset test".to_owned(),
                email: "owner@example.com".to_owned(),
                name: "Owner".to_owned(),
                password: "long-enough-password".to_owned(),
            },
        )
        .await
        .expect("setup");
    setup_tx.commit().await.expect("setup commit");

    let mut session_tx = database
        .begin(&public_context)
        .await
        .expect("session scope");
    let session = service
        .create_session(
            &mut session_tx,
            &public_context,
            &LoginInput {
                email: "owner@example.com".to_owned(),
                password: "long-enough-password".to_owned(),
            },
            chrono::Utc::now(),
        )
        .await
        .expect("session");
    session_tx.commit().await.expect("session commit");

    let mut request_tx = database
        .begin(&public_context)
        .await
        .expect("request scope");
    let notification = service
        .request_password_reset(
            &mut request_tx,
            &public_context,
            &PasswordResetRequestInput {
                email: "owner@example.com".to_owned(),
            },
            chrono::Utc::now(),
        )
        .await
        .expect("reset request")
        .expect("eligible account notification");
    let stored_hash: Vec<u8> = sqlx::query_scalar(
        "select token_hash from password_reset_tokens where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(notification.id.into_uuid())
    .fetch_one(request_tx.conn())
    .await
    .expect("stored token hash");
    assert_ne!(stored_hash, notification.token.as_bytes());
    request_tx.commit().await.expect("request commit");

    let mut redeem_tx = database.begin(&public_context).await.expect("redeem scope");
    service
        .redeem_password_reset(
            &mut redeem_tx,
            &public_context,
            &PasswordResetRedeemInput {
                token: notification.token.clone(),
                password: "new-long-enough-password".to_owned(),
            },
            chrono::Utc::now(),
        )
        .await
        .expect("redeem");
    redeem_tx.commit().await.expect("redeem commit");

    let mut replay_tx = database.begin(&public_context).await.expect("replay scope");
    let replay = service
        .redeem_password_reset(
            &mut replay_tx,
            &public_context,
            &PasswordResetRedeemInput {
                token: notification.token,
                password: "another-long-enough-password".to_owned(),
            },
            chrono::Utc::now(),
        )
        .await
        .expect_err("a reset token is one-time");
    assert!(matches!(replay, MaviError::Conflict { .. }));

    let revoked: bool = sqlx::query_scalar(
        "select revoked_at is not null from sessions where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(session.id.into_uuid())
    .fetch_one(replay_tx.conn())
    .await
    .expect("revoked session");
    assert!(revoked);
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn email_verification_is_one_time_scoped_and_throttled() {
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
                site_name: "Email verification test".to_owned(),
                email: "owner@example.com".to_owned(),
                name: "Owner".to_owned(),
                password: "long-enough-password".to_owned(),
            },
        )
        .await
        .expect("setup");
    setup_tx.commit().await.expect("setup commit");

    let owner_context = SiteContext::with_caller(
        site_id,
        Caller::Account {
            person_id: owner.id,
            session_id: None,
            grants: owner_grants(),
        },
        RequestId::new(),
    );
    let mut create_tx = database.begin(&owner_context).await.expect("create scope");
    let person = service
        .create_person(
            &mut create_tx,
            &owner_context,
            &CreatePerson {
                email: "verify@example.com".to_owned(),
                name: "Verify".to_owned(),
                password: "long-enough-password".to_owned(),
                role_ids: Vec::new(),
            },
        )
        .await
        .expect("person");
    assert!(!person.email_verified);
    create_tx.commit().await.expect("create commit");

    let login_error = {
        let mut tx = database.begin(&public_context).await.expect("login scope");
        let error = service
            .create_session(
                &mut tx,
                &public_context,
                &LoginInput {
                    email: "verify@example.com".to_owned(),
                    password: "long-enough-password".to_owned(),
                },
                Utc::now(),
            )
            .await
            .expect_err("unverified accounts cannot sign in");
        tx.commit().await.expect("login audit commit");
        error
    };
    assert!(matches!(login_error, MaviError::Conflict { .. }));

    let mut latest_notification = None;
    for _ in 0..5 {
        let mut tx = database
            .begin(&public_context)
            .await
            .expect("verification request scope");
        let notification = service
            .request_email_verification(
                &mut tx,
                &public_context,
                &EmailVerificationRequestInput {
                    email: "verify@example.com".to_owned(),
                },
                Utc::now(),
            )
            .await
            .expect("verification request")
            .expect("eligible verification notification");
        latest_notification = Some(notification);
        tx.commit().await.expect("verification request commit");
    }
    let notification = latest_notification.expect("last verification notification");

    let mut limited_tx = database
        .begin(&public_context)
        .await
        .expect("limited request scope");
    let limited = service
        .request_email_verification(
            &mut limited_tx,
            &public_context,
            &EmailVerificationRequestInput {
                email: "verify@example.com".to_owned(),
            },
            Utc::now(),
        )
        .await
        .expect("limited request");
    assert!(limited.is_none());
    limited_tx.commit().await.expect("limited request commit");

    let mut hash_tx = database.begin(&public_context).await.expect("hash scope");
    let stored_hash: Vec<u8> = sqlx::query_scalar(
        "select token_hash from email_verification_tokens where site_id = $1 and id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(notification.id.into_uuid())
    .fetch_one(hash_tx.conn())
    .await
    .expect("stored verification hash");
    assert_ne!(stored_hash, notification.token.as_bytes());
    hash_tx.commit().await.expect("hash commit");

    let mut redeem_tx = database.begin(&public_context).await.expect("redeem scope");
    service
        .redeem_email_verification(
            &mut redeem_tx,
            &public_context,
            &EmailVerificationRedeemInput {
                token: notification.token.clone(),
            },
            Utc::now(),
        )
        .await
        .expect("redeem");
    redeem_tx.commit().await.expect("redeem commit");

    let mut replay_tx = database.begin(&public_context).await.expect("replay scope");
    let replay = service
        .redeem_email_verification(
            &mut replay_tx,
            &public_context,
            &EmailVerificationRedeemInput {
                token: notification.token,
            },
            Utc::now(),
        )
        .await
        .expect_err("verification token is one-time");
    assert!(matches!(replay, MaviError::Conflict { .. }));
    replay_tx.commit().await.expect("replay commit");

    let mut signed_in_tx = database
        .begin(&public_context)
        .await
        .expect("signed-in scope");
    service
        .create_session(
            &mut signed_in_tx,
            &public_context,
            &LoginInput {
                email: "verify@example.com".to_owned(),
                password: "long-enough-password".to_owned(),
            },
            Utc::now(),
        )
        .await
        .expect("verified account can sign in");
    signed_in_tx.commit().await.expect("signed-in commit");
}
