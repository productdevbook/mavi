//! What actually answers.
//!
//! [] is what this installation *describes*. This is what it
//! *serves*, and the two are compared rather than assumed: whatever is
//! described and not here comes back from [],
//! by name, which is how "written and tested" stops being mistaken for
//! reachable.

pub mod about;
pub mod analytics;
pub mod audit;
pub mod boards;
pub mod content;
pub mod courses;
pub mod design;
pub mod flows;
pub mod forms;
pub mod health;
pub mod helpers;
pub mod mail;
pub mod media;
pub mod people;
pub mod portable;
pub mod second;
pub mod settings;
pub mod shop;
pub mod taxonomy;
pub mod trash;

pub use helpers::THAT_IS_NOT_AN_ID;

use std::sync::Arc;

use mavi_core::ports::{Files, Seals};
use mavi_db::Db;
use mavi_serve::{Site, WhoIsAsking};

/// Everything on one address: the API, and the site itself.
pub fn everything(db: &Db, files: &Arc<dyn Files>, who_is_asking: WhoIsAsking) -> axum::Router {
    with_all_of_it(db, files, &None, who_is_asking)
}

/// The same, with the ports that may not be there.
pub fn with_all_of_it(
    db: &Db,
    files: &Arc<dyn Files>,
    seals: &Option<Arc<dyn Seals>>,
    who_is_asking: WhoIsAsking,
) -> axum::Router {
    let showing = crate::showing::Site {
        db: db.clone(),
        files: Arc::clone(files),
    };

    with_everything(db, files, seals, who_is_asking)
        .into_router()
        .fallback(move |request: axum::extract::Request| {
            let showing = showing.clone();

            async move { crate::showing::serve(showing, request).await }
        })
}

/// Everything this installation serves today.
#[must_use]
pub fn site(db: &Db, files: &Arc<dyn Files>, who_is_asking: WhoIsAsking) -> Site {
    with_everything(db, files, &None, who_is_asking)
}

/// The same, with the ports that may not be there.
#[must_use]
pub fn with_everything(
    db: &Db,
    files: &Arc<dyn Files>,
    seals: &Option<Arc<dyn Seals>>,
    who_is_asking: WhoIsAsking,
) -> Site {
    let site = Site::new(who_is_asking);

    let site = people::the_way_in(site, db);
    let site = settings::what_this_site_is(site, db);
    let site = taxonomy::what_it_files_things_under(site, db);

    let site = second::the_second_step(site, db, seals.as_ref());
    let site = health::whether_it_is_well(site, db);
    let site = analytics::how_many_read_it(site, db);
    let site = portable::how_a_site_leaves(site, db);
    let site = trash::what_it_threw_away(site, db);
    let site = about::what_it_holds_about_somebody(site, db);
    let site = content::what_it_wrote(site, db);
    let site = forms::what_it_asks_people(site, db);
    let site = boards::what_is_being_worked_on(site, db);
    let site = courses::what_it_teaches(site, db);
    let site = shop::what_it_sells(site, db);
    let site = mail::what_it_writes_to_people(site, db);
    let site = flows::what_it_does_by_itself(site, db);
    let site = design::how_it_looks(site, db);
    let site = media::what_somebody_uploaded(site, db, files);

    let site = audit::what_has_been_done(site, db);

    crate::assistant::mounted(site)
}
