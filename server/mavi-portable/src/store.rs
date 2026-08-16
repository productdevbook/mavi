//! Writing a site out, and reading one back.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Tx;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::bundle::{Bundle, Language, Read, Term, VERSION, Writing};

pub const THAT_FILE_IS_FROM_A_LATER_VERSION: &str = "that_file_is_from_a_later_version";

/// The whole site.
pub async fn take(tx: &mut Tx) -> Result<Bundle> {
    Ok(Bundle {
        version: VERSION,
        languages: languages(tx).await?,
        terms: terms(tx).await?,
        writings: writings(tx).await?,
    })
}

async fn languages(tx: &mut Tx) -> Result<Vec<Language>> {
    let rows = sqlx::query("select tag, name, is_the_sites_own from languages order by tag")
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Language {
                tag: row.try_get("tag").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                is_the_sites_own: row.try_get("is_the_sites_own").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// Parents before children, so reading it back never points at a row that is
/// not there yet.
async fn terms(tx: &mut Tx) -> Result<Vec<Term>> {
    let rows = sqlx::query(
        "select id, sort, language, slug, name, parent_id from terms
          where deleted_at is null
          order by parent_id nulls first, slug",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Term {
                id: row.try_get("id").map_err(Error::internal)?,
                sort: row.try_get("sort").map_err(Error::internal)?,
                language: row.try_get("language").map_err(Error::internal)?,
                slug: row.try_get("slug").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                parent: row.try_get("parent_id").map_err(Error::internal)?,
            })
        })
        .collect()
}

async fn writings(tx: &mut Tx) -> Result<Vec<Writing>> {
    let rows = sqlx::query(
        "select w.id, w.kind, w.language, w.slug, w.title, w.excerpt, w.body, w.fields,
                w.state, w.published_at,
                coalesce(
                    array_agg(f.term_id) filter (where f.term_id is not null),
                    '{}'
                ) as terms
           from writings w
           left join filed_under f on f.writing_id = w.id
          where w.deleted_at is null
          group by w.id
          order by w.created_at",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Writing {
                id: row.try_get("id").map_err(Error::internal)?,
                kind: row.try_get("kind").map_err(Error::internal)?,
                language: row.try_get("language").map_err(Error::internal)?,
                slug: row.try_get("slug").map_err(Error::internal)?,
                title: row.try_get("title").map_err(Error::internal)?,
                excerpt: row.try_get("excerpt").map_err(Error::internal)?,
                body: row.try_get("body").map_err(Error::internal)?,
                fields: row.try_get("fields").map_err(Error::internal)?,
                state: row.try_get("state").map_err(Error::internal)?,
                published_at: row.try_get("published_at").map_err(Error::internal)?,
                terms: row.try_get("terms").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// Reads a file back in.
///
/// Nothing is overwritten. Everything is written with `on conflict do nothing`
/// against the address that makes it unique, and what did not take is counted
/// as left alone — so reading the wrong file into a site can only add.
///
/// The ids in the file are not the ids that come out. A file carries its own,
/// so that a writing can say what it is filed under, and reading it in maps
/// them to whatever this site ends up calling those things — including where
/// the term was already here under a different id.
pub async fn read_in(tx: &mut Tx, bundle: &Bundle) -> Result<Read> {
    if bundle.version > VERSION {
        return Err(Error::invalid(
            Say::of(THAT_FILE_IS_FROM_A_LATER_VERSION)
                .with("this", &VERSION)
                .with("that", &bundle.version),
        ));
    }

    let mut read = Read::default();

    for language in &bundle.languages {
        let took = sqlx::query(
            "insert into languages (tag, name, is_the_sites_own) values ($1, $2, false)
             on conflict (tag) do nothing",
        )
        .bind(&language.tag)
        .bind(&language.name)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?
        .rows_affected();

        // Never the site's own. Which language a site writes in is a decision
        // it has already made, and a file read into it does not get to change
        // that from underneath whoever made it.
        tally(&mut read.languages, &mut read.left_alone, took);
    }

    let mut theirs_to_ours: BTreeMap<Uuid, Uuid> = BTreeMap::new();

    for term in &bundle.terms {
        let ours = Uuid::now_v7();
        let parent = term
            .parent
            .and_then(|parent| theirs_to_ours.get(&parent).copied());

        let row = sqlx::query(
            "insert into terms (id, sort, language, slug, name, parent_id)
             values ($1, $2, $3, $4, $5, $6)
             on conflict do nothing
             returning id",
        )
        .bind(ours)
        .bind(&term.sort)
        .bind(&term.language)
        .bind(&term.slug)
        .bind(&term.name)
        .bind(parent)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?;

        match row {
            Some(_) => {
                theirs_to_ours.insert(term.id, ours);
                read.terms += 1;
            }
            // Already here. What a writing in this file files itself under has
            // to point at the one that is here, not at nothing.
            None => {
                let here: Option<Uuid> = sqlx::query_scalar(
                    "select id from terms
                      where sort = $1 and language = $2 and slug = $3 and deleted_at is null",
                )
                .bind(&term.sort)
                .bind(&term.language)
                .bind(&term.slug)
                .fetch_optional(tx.conn())
                .await
                .map_err(Error::internal)?;

                if let Some(here) = here {
                    theirs_to_ours.insert(term.id, here);
                }

                read.left_alone += 1;
            }
        }
    }

    for writing in &bundle.writings {
        let ours = Uuid::now_v7();

        let row = sqlx::query(
            "insert into writings
                (id, kind, language, slug, title, excerpt, body, fields, state, published_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             on conflict do nothing
             returning id",
        )
        .bind(ours)
        .bind(&writing.kind)
        .bind(&writing.language)
        .bind(&writing.slug)
        .bind(&writing.title)
        .bind(writing.excerpt.as_deref())
        .bind(&writing.body)
        .bind(&writing.fields)
        .bind(&writing.state)
        .bind(writing.published_at)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?;

        if row.is_none() {
            read.left_alone += 1;
            continue;
        }

        read.writings += 1;

        for theirs in &writing.terms {
            let Some(ours_term) = theirs_to_ours.get(theirs) else {
                continue;
            };

            sqlx::query(
                "insert into filed_under (writing_id, term_id) values ($1, $2)
                 on conflict do nothing",
            )
            .bind(ours)
            .bind(ours_term)
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;
        }
    }

    Ok(read)
}

/// One row, counted as added or as left alone.
fn tally(added: &mut u64, left_alone: &mut u64, took: u64) {
    if took > 0 {
        *added += 1;
    } else {
        *left_alone += 1;
    }
}
