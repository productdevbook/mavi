use std::env;

use mavi_core::{MaviError, SiteContext, SiteId};
use mavi_settings::{
    AnalyticsRetentionInput, CanonicalSiteUrl, CanonicalUrlUpdate, CreateLanguage,
    DEFAULT_LANGUAGE_REQUIRED, LanguageListFilter, MailSenderUpdate, SettingsService,
    UpdateLanguage, UpdateSiteSettings,
};
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a PostgreSQL role that is subject to RLS"]
#[allow(clippy::too_many_lines)]
async fn settings_languages_are_site_scoped_and_audited() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2)
        .await
        .expect("database connection");
    database.migrate().await.expect("migrations");

    let first = SiteId::new();
    let second = SiteId::new();
    database.ensure_site(first).await.expect("first site");
    database.ensure_site(second).await.expect("second site");
    insert_settings(&database, first, "First site").await;
    insert_settings(&database, second, "Second site").await;

    let service = SettingsService;
    let first_context = SiteContext::public(first);
    let mut first_tx = database.begin(&first_context).await.expect("first scope");

    let first_language = service
        .create_language(
            &mut first_tx,
            &first_context,
            &CreateLanguage {
                tag: "en".to_owned(),
                name: "English".to_owned(),
                is_default: false,
            },
        )
        .await
        .expect("first language");
    assert!(first_language.is_default);

    service
        .create_language(
            &mut first_tx,
            &first_context,
            &CreateLanguage {
                tag: "de".to_owned(),
                name: "Deutsch".to_owned(),
                is_default: false,
            },
        )
        .await
        .expect("second language");

    service
        .update_settings(
            &mut first_tx,
            &first_context,
            &UpdateSiteSettings {
                name: Some("Updated first site".to_owned()),
                timezone: Some("Europe/Berlin".to_owned()),
                canonical_url: CanonicalUrlUpdate::Set(
                    "https://first.example.test/site/".to_owned(),
                ),
                mail_sender: MailSenderUpdate::Set {
                    address: "noreply@example.test".to_owned(),
                    name: Some("First site".to_owned()),
                },
                analytics_retention: Some(AnalyticsRetentionInput {
                    raw_days: 30,
                    aggregate_days: 365,
                }),
            },
        )
        .await
        .expect("settings update");

    let settings = service
        .get_settings(&mut first_tx, &first_context)
        .await
        .expect("updated settings");
    assert_eq!(
        settings
            .canonical_url
            .as_ref()
            .map(CanonicalSiteUrl::as_str),
        Some("https://first.example.test/site")
    );
    let sender = settings.mail_sender.as_ref().expect("sender settings");
    assert_eq!(sender.address.as_str(), "noreply@example.test");
    assert_eq!(sender.name.as_deref(), Some("First site"));
    assert_eq!(settings.analytics_retention.raw_days, 30);
    assert_eq!(settings.analytics_retention.aggregate_days, 365);

    service
        .update_settings(
            &mut first_tx,
            &first_context,
            &UpdateSiteSettings {
                name: None,
                timezone: None,
                canonical_url: CanonicalUrlUpdate::Clear,
                mail_sender: MailSenderUpdate::Unchanged,
                analytics_retention: None,
            },
        )
        .await
        .expect("clear canonical URL");
    assert!(
        service
            .get_settings(&mut first_tx, &first_context)
            .await
            .expect("cleared settings")
            .canonical_url
            .is_none()
    );

    service
        .update_language(
            &mut first_tx,
            &first_context,
            "de",
            &UpdateLanguage {
                name: None,
                is_default: Some(true),
            },
        )
        .await
        .expect("default language switch");

    service
        .delete_language(&mut first_tx, &first_context, "en")
        .await
        .expect("old default language delete");

    let default_delete = service
        .delete_language(&mut first_tx, &first_context, "de")
        .await
        .expect_err("current default language must remain");
    assert!(matches!(
        default_delete,
        MaviError::Conflict { code } if code == DEFAULT_LANGUAGE_REQUIRED
    ));

    let remaining = service
        .list_languages(
            &mut first_tx,
            &first_context,
            &LanguageListFilter::default(),
        )
        .await
        .expect("first site languages");
    assert_eq!(remaining.items.len(), 1);
    assert_eq!(remaining.items[0].tag.as_str(), "de");
    first_tx.commit().await.expect("first commit");

    let second_context = SiteContext::public(second);
    let mut second_tx = database.begin(&second_context).await.expect("second scope");
    let second_languages = service
        .list_languages(
            &mut second_tx,
            &second_context,
            &LanguageListFilter::default(),
        )
        .await
        .expect("second site languages");
    assert!(second_languages.items.is_empty());

    service
        .create_language(
            &mut second_tx,
            &second_context,
            &CreateLanguage {
                tag: "en".to_owned(),
                name: "English".to_owned(),
                is_default: true,
            },
        )
        .await
        .expect("second site default language");

    let second_candidates = service
        .public_language_candidates(&mut second_tx, &second_context, Some("de-DE"))
        .await
        .expect("second site default language");
    assert_eq!(second_candidates, ["en"]);
    let cross_site = service
        .get_settings(&mut second_tx, &second_context)
        .await
        .expect("second site settings");
    assert_eq!(cross_site.name, "Second site");
    second_tx.commit().await.expect("second commit");

    let first_context = SiteContext::public(first);
    let mut audit_tx = database.begin(&first_context).await.expect("audit scope");
    let audited: i64 = sqlx::query_scalar(
        "select count(*) from audit_events where site_id = $1 and action like 'settings.%'",
    )
    .bind(first.into_uuid())
    .fetch_one(audit_tx.conn())
    .await
    .expect("settings audit count");
    assert_eq!(audited, 6);
    audit_tx.commit().await.expect("audit commit");
}

async fn insert_settings(database: &Database, site_id: SiteId, name: &str) {
    let context = SiteContext::public(site_id);
    let mut tx = database.begin(&context).await.expect("settings scope");
    sqlx::query("insert into site_settings (site_id, name) values ($1, $2)")
        .bind(site_id.into_uuid())
        .bind(name)
        .execute(tx.conn())
        .await
        .expect("site settings");
    tx.commit().await.expect("settings commit");
}
