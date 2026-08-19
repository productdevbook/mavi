use std::env;

use mavi_boards::{
    BoardListFilter, BoardService, CardPageFilter, CreateBoard, CreateCard, CreateComment,
    CreateList, MoveCard, ReorderLists,
};
use mavi_core::{
    Action, Caller, Capability, Grant, Grants, PageRequest, RequestId, SiteContext, SiteId,
};
use mavi_identity::{CreatePerson, IdentityService, SetupInput};
use mavi_storage::Database;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

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
async fn boards_are_ordered_site_scoped_and_audited() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");

    let public_context = SiteContext::public(first_site);
    let identity = IdentityService;
    let owner = {
        let mut tx = database.begin(&public_context).await.expect("setup scope");
        let owner = identity
            .initialize(
                &mut tx,
                &public_context,
                &SetupInput {
                    site_name: "Boards test".to_owned(),
                    email: "owner-boards@example.com".to_owned(),
                    name: "Owner".to_owned(),
                    password: "long-enough-password".to_owned(),
                },
            )
            .await
            .expect("setup");
        tx.commit().await.expect("setup commit");
        owner
    };
    let owner_context = SiteContext::with_caller(
        first_site,
        Caller::Account {
            person_id: owner.id,
            session_id: None,
            grants: owner_grants(),
        },
        RequestId::new(),
    );
    let person = {
        let mut tx = database.begin(&owner_context).await.expect("person scope");
        let person = identity
            .create_person(
                &mut tx,
                &owner_context,
                &CreatePerson {
                    email: "assignee-boards@example.com".to_owned(),
                    name: "Assignee".to_owned(),
                    password: "long-enough-password".to_owned(),
                    role_ids: vec![],
                },
            )
            .await
            .expect("person");
        tx.commit().await.expect("person commit");
        person
    };

    let boards = BoardService;
    let (board, first_list, second_list, first_card, second_card) = {
        let mut tx = database.begin(&owner_context).await.expect("board scope");
        let board = boards
            .create_board(
                &mut tx,
                &owner_context,
                &CreateBoard {
                    name: "Editorial board".to_owned(),
                    description: Some("Site-local work".to_owned()),
                },
            )
            .await
            .expect("board");
        let first_list = boards
            .create_list(
                &mut tx,
                &owner_context,
                board.id,
                &CreateList {
                    name: "Backlog".to_owned(),
                },
            )
            .await
            .expect("first list");
        let second_list = boards
            .create_list(
                &mut tx,
                &owner_context,
                board.id,
                &CreateList {
                    name: "Published".to_owned(),
                },
            )
            .await
            .expect("second list");
        let first_card = boards
            .create_card(
                &mut tx,
                &owner_context,
                first_list.id,
                &CreateCard {
                    title: "First post".to_owned(),
                    description: None,
                    assignee_id: Some(person.id),
                },
            )
            .await
            .expect("first card");
        let second_card = boards
            .create_card(
                &mut tx,
                &owner_context,
                first_list.id,
                &CreateCard {
                    title: "Second post".to_owned(),
                    description: Some("Needs review".to_owned()),
                    assignee_id: None,
                },
            )
            .await
            .expect("second card");
        tx.commit().await.expect("board commit");
        (board, first_list, second_list, first_card, second_card)
    };

    let mut tx = database.begin(&owner_context).await.expect("order scope");
    let lists = boards
        .reorder_lists(
            &mut tx,
            &owner_context,
            board.id,
            &ReorderLists {
                order: vec![second_list.id, first_list.id],
            },
        )
        .await
        .expect("reorder");
    assert_eq!(lists.items[0].id, second_list.id);
    let first_page = boards
        .list_cards(
            &mut tx,
            first_list.id,
            &CardPageFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
                assignee_id: None,
            },
        )
        .await
        .expect("cursor card list");
    assert_eq!(first_page.items.len(), 1);
    assert!(first_page.next_cursor.is_some());
    let comment = boards
        .create_comment(
            &mut tx,
            &owner_context,
            first_card.id,
            &CreateComment {
                body: "Please review the slug.".to_owned(),
            },
        )
        .await
        .expect("comment");
    assert_eq!(comment.author_id, Some(owner.id));
    boards
        .move_card(
            &mut tx,
            &owner_context,
            second_card.id,
            &MoveCard {
                list_id: second_list.id,
                before_card_id: None,
            },
        )
        .await
        .expect("move");
    let target_cards = boards
        .list_cards(&mut tx, second_list.id, &CardPageFilter::default())
        .await
        .expect("target cards");
    assert_eq!(target_cards.items[0].id, second_card.id);
    let activity = boards
        .list_activity(
            &mut tx,
            board.id,
            &mavi_boards::ActivityPageFilter::default(),
        )
        .await
        .expect("activity");
    assert!(
        activity
            .items
            .iter()
            .any(|item| item.kind == "board.card.moved")
    );
    boards
        .delete_card(&mut tx, &owner_context, second_card.id)
        .await
        .expect("delete card");
    assert!(boards.get_card(&mut tx, second_card.id).await.is_err());
    let activity = boards
        .list_activity(
            &mut tx,
            board.id,
            &mavi_boards::ActivityPageFilter::default(),
        )
        .await
        .expect("activity after delete");
    assert!(
        activity
            .items
            .iter()
            .any(|item| item.kind == "board.card.deleted")
    );
    tx.commit().await.expect("order commit");

    let second_context = SiteContext::public(second_site);
    let mut tx = database
        .begin(&second_context)
        .await
        .expect("isolation scope");
    assert!(
        boards
            .list_boards(&mut tx, &BoardListFilter::default())
            .await
            .expect("isolation list")
            .items
            .is_empty()
    );
    assert!(boards.get_board(&mut tx, board.id).await.is_err());
    tx.commit().await.expect("isolation commit");
}
