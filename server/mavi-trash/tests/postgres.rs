use chrono::Utc;
use mavi_content::{ContentService, CreateContent, PublicationInput};
use mavi_core::ports::FileStore;
use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_courses::{CoursesService, CreateCourse, CreateLesson, CreateModule, CreateStudent};
use mavi_files::InMemoryFileStore;
use mavi_forms::{CreateForm, FormService};
use mavi_media::{FileVisibility, MediaService};
use mavi_shop::{
    CouponListFilter, CreateCoupon, CreateProduct, ProductListFilter, ProductPrice, ShopService,
};
use mavi_storage::Database;
use mavi_taxonomy::{CreateTerm, TaxonomyService, TermKind};
use mavi_trash::{TrashKind, TrashListFilter, TrashService};
use serde_json::json;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn trash_lists_restores_and_permanently_deletes_site_resources() {
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

    let context = SiteContext::public(first_site);
    let content_service = ContentService;
    let taxonomy_service = TaxonomyService;
    let media_service = MediaService;
    let trash_service = TrashService;
    let store = InMemoryFileStore::default();

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .initialize(&mut transaction, &context)
        .await
        .expect("content types");
    let content_entry = content_service
        .create(
            &mut transaction,
            &context,
            &CreateContent {
                kind: "post".to_owned(),
                language: "en".to_owned(),
                slug: "trash-content".to_owned(),
                title: "Trash content".to_owned(),
                excerpt: None,
                body: "body".to_owned(),
                fields: json!({}),
                publication: PublicationInput::Draft,
            },
            Utc::now(),
        )
        .await
        .expect("content");
    let term = taxonomy_service
        .create_term(
            &mut transaction,
            &context,
            &CreateTerm {
                kind: TermKind::Tag,
                language: "en".to_owned(),
                slug: "trash-term".to_owned(),
                name: "Trash term".to_owned(),
                parent_id: None,
            },
        )
        .await
        .expect("term");
    let file = media_service
        .upload(
            &mut transaction,
            &context,
            &store,
            "trash.png",
            FileVisibility::Private,
            PNG.to_vec(),
        )
        .await
        .expect("file");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .trash(&mut transaction, &context, content_entry.id)
        .await
        .expect("trash content");
    taxonomy_service
        .delete_term(&mut transaction, &context, term.id)
        .await
        .expect("trash term");
    media_service
        .trash(&mut transaction, &context, file.id)
        .await
        .expect("trash file");
    transaction.commit().await.expect("commit");

    let relocation_site = SiteId::new();
    database
        .ensure_site(relocation_site)
        .await
        .expect("relocation site");
    let mut transaction = database.begin(&context).await.expect("export transaction");
    let relocation = trash_service
        .export_for_relocation(&mut transaction, &context, &store)
        .await
        .expect("export trash relocation");
    assert_eq!(relocation.source_site_id, first_site);
    assert_eq!(relocation.content.len(), 1);
    assert!(!relocation.revisions.is_empty());
    assert_eq!(relocation.terms.len(), 1);
    assert_eq!(relocation.files.len(), 1);
    assert_eq!(relocation.files[0].id, file.id.into_uuid());
    transaction.commit().await.expect("export commit");

    let relocation_context = SiteContext::public(relocation_site);
    let mut relocation = relocation;
    relocation.source_site_id = relocation_site;
    let mut transaction = database
        .begin(&relocation_context)
        .await
        .expect("relocation transaction");
    trash_service
        .import_for_relocation(&mut transaction, &relocation_context, &store, &relocation)
        .await
        .expect("import trash relocation");
    let imported = trash_service
        .list(
            &mut transaction,
            &relocation_context,
            &TrashListFilter::default(),
        )
        .await
        .expect("imported trash list");
    assert_eq!(imported.items.len(), 3);
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("restore imported content");
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::Term,
            term.id.into_uuid(),
        )
        .await
        .expect("restore imported term");
    trash_service
        .restore(
            &mut transaction,
            &relocation_context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("restore imported file");
    assert_eq!(
        store
            .get(&relocation_context, &relocation.files[0].storage_key)
            .await
            .expect("imported file bytes"),
        PNG
    );
    transaction.commit().await.expect("relocation commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let first_page = trash_service
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(2),
                },
                kind: None,
            },
        )
        .await
        .expect("first trash page");
    assert_eq!(first_page.items.len(), 2);
    let cursor = first_page.next_cursor.clone().expect("trash cursor");
    let second_page = trash_service
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest {
                    after: Some(cursor),
                    limit: Some(2),
                },
                kind: None,
            },
        )
        .await
        .expect("second trash page");
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());

    trash_service
        .restore(
            &mut transaction,
            &context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("restore content");
    let restored = content_service
        .get(&mut transaction, &context, content_entry.id)
        .await
        .expect("restored content");
    assert_eq!(restored.id, content_entry.id);
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let file_deletion = trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::File,
            file.id.into_uuid(),
        )
        .await
        .expect("permanently delete file metadata");
    let storage_key = file_deletion
        .file_storage_key
        .clone()
        .expect("file storage key");
    assert!(store.get(&context, &storage_key).await.is_ok());
    transaction.commit().await.expect("commit");

    store
        .remove(&context, &storage_key)
        .await
        .expect("remove file bytes");
    let mut transaction = database.begin(&context).await.expect("transaction");
    media_service
        .complete_cleanup(
            &mut transaction,
            &context,
            mavi_core::FileId::from_uuid(file.id.into_uuid()),
            &storage_key,
        )
        .await
        .expect("complete cleanup");
    transaction.commit().await.expect("commit");
    assert!(store.get(&context, &storage_key).await.is_err());

    let mut transaction = database.begin(&context).await.expect("transaction");
    trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Term,
            term.id.into_uuid(),
        )
        .await
        .expect("permanently delete term");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    content_service
        .trash(&mut transaction, &context, content_entry.id)
        .await
        .expect("trash restored content");
    trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Content,
            content_entry.id.into_uuid(),
        )
        .await
        .expect("permanently delete content");
    transaction.commit().await.expect("commit");

    let mut transaction = database.begin(&context).await.expect("transaction");
    let missing = content_service
        .get(&mut transaction, &context, content_entry.id)
        .await;
    assert!(matches!(missing, Err(MaviError::NotFound { .. })));
    let completed: bool = sqlx::query_scalar(
        "select completed_at is not null from media_cleanup_tasks where site_id = $1 and file_id = $2",
    )
    .bind(first_site.into_uuid())
    .bind(file.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("cleanup receipt");
    assert!(completed);
    transaction.commit().await.expect("commit");

    let second_context = SiteContext::public(second_site);
    let mut transaction = database.begin(&second_context).await.expect("transaction");
    let isolated = trash_service
        .list(
            &mut transaction,
            &second_context,
            &TrashListFilter::default(),
        )
        .await
        .expect("isolated trash list");
    assert!(isolated.items.is_empty());
    transaction.commit().await.expect("commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn form_trash_restores_and_permanently_deletes_submissions_with_the_form() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    let other_site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    database
        .ensure_site(other_site_id)
        .await
        .expect("other site");

    let context = SiteContext::public(site_id);
    let form_service = FormService;
    let trash_service = TrashService;
    let form = {
        let mut transaction = database.begin(&context).await.expect("create transaction");
        let form = form_service
            .create(
                &mut transaction,
                &context,
                &CreateForm {
                    slug: "form-trash".to_owned(),
                    name: "Form trash".to_owned(),
                    fields: Vec::new(),
                    kept_days: None,
                },
            )
            .await
            .expect("form");
        sqlx::query(
            "insert into form_submissions (site_id, id, form_id, answers)
             values ($1, $2, $3, $4)",
        )
        .bind(site_id.into_uuid())
        .bind(uuid::Uuid::now_v7())
        .bind(form.id.into_uuid())
        .bind(json!({"email": "trash@example.test"}))
        .execute(transaction.conn())
        .await
        .expect("submission");
        transaction.commit().await.expect("create commit");
        form
    };

    let mut transaction = database.begin(&context).await.expect("trash transaction");
    form_service
        .delete(&mut transaction, &context, form.id)
        .await
        .expect("trash form");
    transaction.commit().await.expect("trash commit");

    let mut transaction = database.begin(&context).await.expect("list transaction");
    let page = trash_service
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Form),
            },
        )
        .await
        .expect("form trash list");
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, form.id.into_uuid());
    assert_eq!(page.items[0].kind, TrashKind::Form);
    trash_service
        .restore(
            &mut transaction,
            &context,
            TrashKind::Form,
            form.id.into_uuid(),
        )
        .await
        .expect("restore form");
    form_service
        .get(&mut transaction, &context, form.id)
        .await
        .expect("restored form");
    transaction.commit().await.expect("restore commit");

    let mut transaction = database
        .begin(&context)
        .await
        .expect("trash again transaction");
    form_service
        .delete(&mut transaction, &context, form.id)
        .await
        .expect("trash restored form");
    trash_service
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Form,
            form.id.into_uuid(),
        )
        .await
        .expect("permanently delete form");
    transaction.commit().await.expect("permanent delete commit");

    let mut transaction = database.begin(&context).await.expect("assert transaction");
    let form_exists: bool =
        sqlx::query_scalar("select exists(select 1 from forms where site_id = $1 and id = $2)")
            .bind(site_id.into_uuid())
            .bind(form.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("form state");
    let submission_count: i64 = sqlx::query_scalar(
        "select count(*) from form_submissions where site_id = $1 and form_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(form.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("submission state");
    assert!(!form_exists);
    assert_eq!(submission_count, 0);
    transaction.commit().await.expect("assert commit");

    let other_context = SiteContext::public(other_site_id);
    let mut transaction = database
        .begin(&other_context)
        .await
        .expect("other site transaction");
    let isolated = trash_service
        .list(
            &mut transaction,
            &other_context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Form),
            },
        )
        .await
        .expect("other site trash list");
    assert!(isolated.items.is_empty());
    transaction.commit().await.expect("other site commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn shop_trash_restores_and_permanently_deletes_products_and_coupons() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    let other_site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    database
        .ensure_site(other_site_id)
        .await
        .expect("other site");

    let context = SiteContext::public(site_id);
    let shop = ShopService;
    let trash = TrashService;
    let (product, coupon) = {
        let mut transaction = database.begin(&context).await.expect("create scope");
        let product = shop
            .create_product(
                &mut transaction,
                &context,
                &CreateProduct {
                    slug: "trash-product".to_owned(),
                    name: "Trash product".to_owned(),
                    description: None,
                    price: ProductPrice {
                        minor: 1_250,
                        currency: "TRY".to_owned(),
                    },
                    stock: 3,
                    on_sale: true,
                },
            )
            .await
            .expect("product");
        let coupon = shop
            .create_coupon(
                &mut transaction,
                &context,
                &CreateCoupon {
                    code: "TRASH10".to_owned(),
                    percent: Some(10),
                    amount_minor: None,
                    currency: None,
                    max_uses: Some(10),
                    expires_at: None,
                },
            )
            .await
            .expect("coupon");
        transaction.commit().await.expect("create commit");
        (product, coupon)
    };

    let mut transaction = database.begin(&context).await.expect("trash scope");
    shop.delete_product(&mut transaction, &context, product.id)
        .await
        .expect("trash product");
    shop.delete_coupon(&mut transaction, &context, coupon.id)
        .await
        .expect("trash coupon");
    transaction.commit().await.expect("trash commit");

    let mut transaction = database.begin(&context).await.expect("list scope");
    let products = trash
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Product),
            },
        )
        .await
        .expect("product trash list");
    assert_eq!(products.items.len(), 1);
    assert_eq!(products.items[0].id, product.id.into_uuid());
    let coupons = trash
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Coupon),
            },
        )
        .await
        .expect("coupon trash list");
    assert_eq!(coupons.items.len(), 1);
    assert_eq!(coupons.items[0].id, coupon.id.into_uuid());
    trash
        .restore(
            &mut transaction,
            &context,
            TrashKind::Product,
            product.id.into_uuid(),
        )
        .await
        .expect("restore product");
    trash
        .restore(
            &mut transaction,
            &context,
            TrashKind::Coupon,
            coupon.id.into_uuid(),
        )
        .await
        .expect("restore coupon");
    assert_eq!(
        shop.list_products(&mut transaction, &context, &ProductListFilter::default())
            .await
            .expect("restored products")
            .items
            .len(),
        1
    );
    assert!(
        shop.get_product(&mut transaction, &context, product.id)
            .await
            .expect("restored product")
            .on_sale
    );
    assert_eq!(
        shop.list_coupons(&mut transaction, &context, &CouponListFilter::default())
            .await
            .expect("restored coupons")
            .items
            .len(),
        1
    );
    transaction.commit().await.expect("restore commit");

    let mut transaction = database.begin(&context).await.expect("permanent scope");
    shop.delete_product(&mut transaction, &context, product.id)
        .await
        .expect("trash product again");
    shop.delete_coupon(&mut transaction, &context, coupon.id)
        .await
        .expect("trash coupon again");
    trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Product,
            product.id.into_uuid(),
        )
        .await
        .expect("delete product");
    trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Coupon,
            coupon.id.into_uuid(),
        )
        .await
        .expect("delete coupon");
    transaction.commit().await.expect("permanent commit");

    let mut transaction = database.begin(&context).await.expect("assert scope");
    let product_exists: bool = sqlx::query_scalar(
        "select exists(select 1 from shop_products where site_id = $1 and id = $2)",
    )
    .bind(site_id.into_uuid())
    .bind(product.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("product state");
    let coupon_exists: bool = sqlx::query_scalar(
        "select exists(select 1 from shop_coupons where site_id = $1 and id = $2)",
    )
    .bind(site_id.into_uuid())
    .bind(coupon.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("coupon state");
    assert!(!product_exists);
    assert!(!coupon_exists);
    transaction.commit().await.expect("assert commit");

    let other_context = SiteContext::public(other_site_id);
    let mut transaction = database
        .begin(&other_context)
        .await
        .expect("other site scope");
    let isolated = trash
        .list(
            &mut transaction,
            &other_context,
            &TrashListFilter::default(),
        )
        .await
        .expect("isolated trash list");
    assert!(isolated.items.is_empty());
    transaction.commit().await.expect("other site commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
async fn shop_product_trash_refuses_permanent_delete_while_stock_is_held() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    let context = SiteContext::public(site_id);
    let shop = ShopService;
    let trash = TrashService;

    let mut transaction = database.begin(&context).await.expect("create scope");
    let product = shop
        .create_product(
            &mut transaction,
            &context,
            &CreateProduct {
                slug: "held-trash-product".to_owned(),
                name: "Held trash product".to_owned(),
                description: None,
                price: ProductPrice {
                    minor: 500,
                    currency: "TRY".to_owned(),
                },
                stock: 1,
                on_sale: true,
            },
        )
        .await
        .expect("product");
    let order_id = uuid::Uuid::now_v7();
    sqlx::query(
        "insert into shop_orders
            (site_id, id, number, state, email, total_minor, currency, idempotency_key)
         values ($1, $2, 1, 'waiting', 'held@example.test', 500, 'TRY', 'held-trash-order')",
    )
    .bind(site_id.into_uuid())
    .bind(order_id)
    .execute(transaction.conn())
    .await
    .expect("order");
    sqlx::query(
        "insert into shop_stock_holds
            (site_id, id, order_id, product_id, quantity, status, expires_at)
         values ($1, $2, $3, $4, 1, 'held', clock_timestamp() + interval '1 hour')",
    )
    .bind(site_id.into_uuid())
    .bind(uuid::Uuid::now_v7())
    .bind(order_id)
    .bind(product.id.into_uuid())
    .execute(transaction.conn())
    .await
    .expect("stock hold");
    shop.delete_product(&mut transaction, &context, product.id)
        .await
        .expect("trash product");
    let conflict = trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Product,
            product.id.into_uuid(),
        )
        .await
        .expect_err("active stock hold must block permanent deletion");
    assert!(matches!(
        conflict,
        MaviError::Conflict { ref code } if code == "trash_shop_product_active_hold"
    ));
    sqlx::query(
        "update shop_stock_holds
            set status = 'released', settled_at = clock_timestamp()
          where site_id = $1 and order_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(order_id)
    .execute(transaction.conn())
    .await
    .expect("release hold");
    trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Product,
            product.id.into_uuid(),
        )
        .await
        .expect("delete after hold release");
    transaction.commit().await.expect("commit");
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn course_and_student_trash_preserves_learning_state_until_purge() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");

    let site_id = SiteId::new();
    let other_site_id = SiteId::new();
    database.ensure_site(site_id).await.expect("site");
    database
        .ensure_site(other_site_id)
        .await
        .expect("other site");
    let context = SiteContext::public(site_id);
    let courses = CoursesService;
    let trash = TrashService;
    let (course, student) = {
        let mut transaction = database.begin(&context).await.expect("create scope");
        let course = courses
            .create_course(
                &mut transaction,
                &context,
                &CreateCourse {
                    slug: "trash-course".to_owned(),
                    title: "Trash course".to_owned(),
                    about: Some("restorable curriculum".to_owned()),
                },
            )
            .await
            .expect("course");
        let module = courses
            .create_module(
                &mut transaction,
                &context,
                course.id,
                &CreateModule {
                    title: "Module".to_owned(),
                },
            )
            .await
            .expect("module");
        courses
            .create_lesson(
                &mut transaction,
                &context,
                module.id,
                &CreateLesson {
                    title: "Lesson".to_owned(),
                    body: "Body".to_owned(),
                    media_file_id: None,
                },
            )
            .await
            .expect("lesson");
        let student = courses
            .create_student(
                &mut transaction,
                &context,
                &CreateStudent {
                    email: "trash-student@example.test".to_owned(),
                    name: "Trash student".to_owned(),
                },
                Utc::now(),
            )
            .await
            .expect("student")
            .student;
        transaction.commit().await.expect("create commit");
        (course, student)
    };

    let mut transaction = database.begin(&context).await.expect("trash scope");
    courses
        .delete_course(&mut transaction, &context, course.id)
        .await
        .expect("trash course");
    courses
        .delete_student(&mut transaction, &context, student.id)
        .await
        .expect("trash student");
    let courses_in_trash = trash
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Course),
            },
        )
        .await
        .expect("course trash");
    assert_eq!(courses_in_trash.items.len(), 1);
    assert_eq!(courses_in_trash.items[0].id, course.id.into_uuid());
    let students_in_trash = trash
        .list(
            &mut transaction,
            &context,
            &TrashListFilter {
                page: PageRequest::default(),
                kind: Some(TrashKind::Student),
            },
        )
        .await
        .expect("student trash");
    assert_eq!(students_in_trash.items.len(), 1);
    assert_eq!(students_in_trash.items[0].id, student.id.into_uuid());
    trash
        .restore(
            &mut transaction,
            &context,
            TrashKind::Course,
            course.id.into_uuid(),
        )
        .await
        .expect("restore course");
    trash
        .restore(
            &mut transaction,
            &context,
            TrashKind::Student,
            student.id.into_uuid(),
        )
        .await
        .expect("restore student");
    assert_eq!(
        courses
            .get_course(&mut transaction, &context, course.id)
            .await
            .expect("restored course")
            .modules
            .len(),
        1
    );
    assert_eq!(
        courses
            .list_students(
                &mut transaction,
                &context,
                &mavi_courses::StudentListFilter::default(),
            )
            .await
            .expect("restored student list")
            .items
            .len(),
        1
    );
    transaction.commit().await.expect("restore commit");

    let mut transaction = database.begin(&context).await.expect("purge scope");
    courses
        .delete_course(&mut transaction, &context, course.id)
        .await
        .expect("trash course again");
    courses
        .delete_student(&mut transaction, &context, student.id)
        .await
        .expect("trash student again");
    trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Course,
            course.id.into_uuid(),
        )
        .await
        .expect("purge course");
    trash
        .permanently_delete(
            &mut transaction,
            &context,
            TrashKind::Student,
            student.id.into_uuid(),
        )
        .await
        .expect("purge student");
    let course_exists: bool =
        sqlx::query_scalar("select exists(select 1 from courses where site_id = $1 and id = $2)")
            .bind(site_id.into_uuid())
            .bind(course.id.into_uuid())
            .fetch_one(transaction.conn())
            .await
            .expect("course state");
    let module_count: i64 = sqlx::query_scalar(
        "select count(*) from course_modules where site_id = $1 and course_id = $2",
    )
    .bind(site_id.into_uuid())
    .bind(course.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("module state");
    let student_exists: bool = sqlx::query_scalar(
        "select exists(select 1 from course_students where site_id = $1 and id = $2)",
    )
    .bind(site_id.into_uuid())
    .bind(student.id.into_uuid())
    .fetch_one(transaction.conn())
    .await
    .expect("student state");
    assert!(!course_exists);
    assert_eq!(module_count, 0);
    assert!(!student_exists);
    transaction.commit().await.expect("purge commit");

    let other_context = SiteContext::public(other_site_id);
    let mut transaction = database.begin(&other_context).await.expect("other scope");
    assert!(
        trash
            .list(
                &mut transaction,
                &other_context,
                &TrashListFilter::default()
            )
            .await
            .expect("other trash")
            .items
            .is_empty()
    );
    transaction.commit().await.expect("other commit");
}
