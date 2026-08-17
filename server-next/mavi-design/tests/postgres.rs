use mavi_core::{MaviError, PageRequest, SiteContext, SiteId, ports::FileStore};
use mavi_design::{
    BuildEngine, DesignChangeListFilter, DesignFileInput, DesignService, StartDesignChange,
    StaticBuildEngine,
};
use mavi_files::InMemoryFileStore;
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn design_changes_builds_and_publishing_are_site_scoped() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");
    let first_context = SiteContext::public(first_site);
    let second_context = SiteContext::public(second_site);
    let service = DesignService;
    let builder = StaticBuildEngine;
    let store = InMemoryFileStore::default();

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let first = service
        .start_change(
            &mut transaction,
            &first_context,
            &StartDesignChange {
                name: "Initial design".to_owned(),
            },
        )
        .await
        .expect("start first change");
    service
        .write_file(
            &mut transaction,
            &first_context,
            first.id,
            &DesignFileInput {
                path: "public/index.html".to_owned(),
                contents: "<h1>first</h1>".to_owned(),
            },
        )
        .await
        .expect("write entrypoint");
    service
        .write_file(
            &mut transaction,
            &first_context,
            first.id,
            &DesignFileInput {
                path: "src/main.ts".to_owned(),
                contents: "console.log('private source');".to_owned(),
            },
        )
        .await
        .expect("write source");
    transaction.commit().await.expect("commit first change");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let request = service
        .start_build(&mut transaction, &first_context, first.id)
        .await
        .expect("start build");
    transaction.commit().await.expect("commit build start");
    let artifacts = builder
        .build(&first_context, request.build.id, &request.source)
        .await
        .expect("static build");
    assert_eq!(artifacts.len(), 1, "src/ must never become public artifact");
    let stored = service
        .persist_artifacts(&first_context, &store, request.build.id, artifacts)
        .await
        .expect("persist build");
    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let first_build = service
        .finish_build_success(&mut transaction, &first_context, request.build.id, &stored)
        .await
        .expect("finish build");
    transaction.commit().await.expect("commit build");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let published = service
        .publish(&mut transaction, &first_context, first.id)
        .await
        .expect("publish first");
    assert_eq!(published.state.as_str(), "published");
    transaction.commit().await.expect("commit publish");
    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let live = service
        .live_artifact(&mut transaction, &first_context, "index.html")
        .await
        .expect("live artifact");
    transaction.commit().await.expect("commit live read");
    assert_eq!(
        store
            .get(&first_context, &live.storage_key)
            .await
            .expect("live bytes"),
        b"<h1>first</h1>"
    );

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let second = service
        .start_change(
            &mut transaction,
            &first_context,
            &StartDesignChange {
                name: "Second design".to_owned(),
            },
        )
        .await
        .expect("start second change");
    let copied = service
        .read_file(
            &mut transaction,
            &first_context,
            second.id,
            "public/index.html",
        )
        .await
        .expect("copy published files");
    assert_eq!(copied.contents, "<h1>first</h1>");
    service
        .write_file(
            &mut transaction,
            &first_context,
            second.id,
            &DesignFileInput {
                path: "public/index.html".to_owned(),
                contents: "<h1>second</h1>".to_owned(),
            },
        )
        .await
        .expect("write second entrypoint");
    transaction.commit().await.expect("commit second change");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let second_request = service
        .start_build(&mut transaction, &first_context, second.id)
        .await
        .expect("start second build");
    transaction
        .commit()
        .await
        .expect("commit second build start");
    let second_artifacts = builder
        .build(
            &first_context,
            second_request.build.id,
            &second_request.source,
        )
        .await
        .expect("second static build");
    let second_stored = service
        .persist_artifacts(
            &first_context,
            &store,
            second_request.build.id,
            second_artifacts,
        )
        .await
        .expect("persist second build");
    let mut transaction = database.begin(&first_context).await.expect("transaction");
    service
        .finish_build_success(
            &mut transaction,
            &first_context,
            second_request.build.id,
            &second_stored,
        )
        .await
        .expect("finish second build");
    transaction.commit().await.expect("commit second build");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    service
        .publish(&mut transaction, &first_context, second.id)
        .await
        .expect("publish second");
    transaction.commit().await.expect("commit second publish");
    let mut transaction = database.begin(&first_context).await.expect("transaction");
    service
        .rollback(&mut transaction, &first_context, first.id)
        .await
        .expect("rollback first");
    transaction.commit().await.expect("commit rollback");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let live_after_rollback = service
        .live_artifact(&mut transaction, &first_context, "index.html")
        .await
        .expect("live after rollback");
    transaction.commit().await.expect("commit live rollback");
    assert_eq!(
        store
            .get(&first_context, &live_after_rollback.storage_key)
            .await
            .expect("rolled back bytes"),
        b"<h1>first</h1>"
    );

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let page = service
        .list_changes(
            &mut transaction,
            &first_context,
            &DesignChangeListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                state: None,
            },
        )
        .await
        .expect("list changes");
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_some());
    transaction.commit().await.expect("commit list");

    let mut transaction = database.begin(&second_context).await.expect("transaction");
    assert!(matches!(
        service
            .get_change(&mut transaction, &second_context, first.id)
            .await,
        Err(MaviError::NotFound { .. })
    ));
    transaction.commit().await.expect("commit isolation read");

    let mut transaction = database.begin(&first_context).await.expect("transaction");
    let published_write = service
        .write_file(
            &mut transaction,
            &first_context,
            first.id,
            &DesignFileInput {
                path: "public/index.html".to_owned(),
                contents: "must fail".to_owned(),
            },
        )
        .await;
    assert!(matches!(published_write, Err(MaviError::Conflict { .. })));
    transaction.commit().await.expect("commit rejected write");
    assert_eq!(first_build.state.as_str(), "ready");
}
