//! Reading and writing what a site files things under.
//!
//! The two rules that cannot be a constraint are here: a term may not go under
//! itself at any depth, and what a writing is filed under is replaced as one
//! thing rather than added to and taken from.

use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::BY_RECENT;
use crate::term::{
    A_CATEGORY_GOES_UNDER_A_CATEGORY, NOTHING_GOES_UNDER_ITSELF, Sort, Term, TermId, goes_under,
    name_is_taken, taken,
};

pub const NOTHING_IS_FILED_UNDER_THAT: &str = "nothing_is_filed_under_that";

const COLUMNS: &str = "id, sort, language, slug, name, parent, created_at, updated_at";

fn a_term(row: &PgRow) -> Result<Term> {
    let sort: String = row.try_get("sort").map_err(Error::internal)?;
    let slug: String = row.try_get("slug").map_err(Error::internal)?;
    let parent: Option<Uuid> = row.try_get("parent").map_err(Error::internal)?;

    Ok(Term {
        id: TermId(row.try_get("id").map_err(Error::internal)?),
        sort: if sort == Sort::Tag.as_str() {
            Sort::Tag
        } else {
            Sort::Category
        },
        language: row.try_get("language").map_err(Error::internal)?,
        slug: Slug::parse(&slug)?,
        name: row.try_get("name").map_err(Error::internal)?,
        parent: parent.map(TermId),
        created_at: row.try_get("created_at").map_err(Error::internal)?,
        updated_at: row.try_get("updated_at").map_err(Error::internal)?,
    })
}

/// What a site files things under.
pub async fn list(
    tx: &mut Tx,
    sort: Option<Sort>,
    language: Option<&str>,
    query: &Query,
) -> Result<Page<Term>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(sort) = sort {
        binds.push(sort.as_str().to_owned());
        wheres.push(format!("sort = ${}", binds.len()));
    }

    if let Some(language) = language {
        binds.push(language.to_owned());
        wheres.push(format!("language = ${}", binds.len()));
    }

    let cursor = walk.after(binds.len() + 1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select {COLUMNS} from terms where {} order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

    for bind in binds {
        asking = asking.bind(bind);
    }

    if let Some((_, values)) = cursor {
        for value in values {
            asking = asking.bind(value);
        }
    }

    let rows = asking
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?
        .iter()
        .map(a_term)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |term| {
        vec![term.created_at.to_rfc3339(), term.id.to_string()]
    })
}

/// What making one asks for.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewTerm {
    pub sort: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent: Option<Uuid>,
}

/// Makes one.
pub async fn make(tx: &mut Tx, new: &NewTerm) -> Result<Term> {
    let sort = match new.sort.as_str() {
        "tag" => Sort::Tag,
        "category" => Sort::Category,
        _ => return Err(Error::invalid(Say::of("that_is_not_a_sort_of_term"))),
    };

    let slug = Slug::parse(&new.slug)?;
    let id = TermId::new();

    let parent = under(tx, sort, id, new.parent).await?;

    let row = sqlx::query(&format!(
        "insert into terms (id, sort, language, slug, name, parent)
         values ($1, $2, $3, $4, $5, $6)
         returning {COLUMNS}"
    ))
    .bind(Uuid::from(id))
    .bind(sort.as_str())
    .bind(&new.language)
    .bind(slug.as_str())
    .bind(new.name.trim())
    .bind(parent)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| {
        if taken(&cause) {
            name_is_taken()
        } else {
            Error::internal(cause)
        }
    })?;

    a_term(&row)
}

/// Whether this may go under that, including the part no constraint can see.
///
/// A term under itself at one step is a check in the schema. At two steps it is
/// not, and a category under its own child is a tree with a loop in it: every
/// screen that draws it runs until something stops it.
async fn under(tx: &mut Tx, sort: Sort, id: TermId, parent: Option<Uuid>) -> Result<Option<Uuid>> {
    let Some(parent) = parent else {
        goes_under(sort, id, None)?;

        return Ok(None);
    };

    let theirs: Option<String> =
        sqlx::query_scalar("select sort from terms where id = $1 and deleted_at is null")
            .bind(parent)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

    let theirs = theirs.ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_FILED_UNDER_THAT)))?;

    let theirs = if theirs == Sort::Tag.as_str() {
        Sort::Tag
    } else {
        Sort::Category
    };

    goes_under(sort, id, Some((TermId(parent), theirs)))?;

    // Walking up, which is the half a constraint cannot do. Bounded by the
    // number of rows rather than by trust: a loop that already exists must not
    // make this run for ever while proving there is one.
    let mut climbing = Some(parent);
    let mut steps = 0;

    while let Some(at) = climbing {
        if at == Uuid::from(id) {
            return Err(Error::invalid(Say::of(NOTHING_GOES_UNDER_ITSELF)));
        }

        steps += 1;
        if steps > 64 {
            return Err(Error::invalid(Say::of(A_CATEGORY_GOES_UNDER_A_CATEGORY)));
        }

        climbing = sqlx::query_scalar("select parent from terms where id = $1")
            .bind(at)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?
            .flatten();
    }

    Ok(Some(parent))
}

/// What changing one asks for.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TermChanges {
    pub name: Option<String>,
    /// `Some(None)` moves it out from under anything.
    pub parent: Option<Option<Uuid>>,
}

/// Renames one, or moves it.
pub async fn change(tx: &mut Tx, id: TermId, changes: &TermChanges) -> Result<Term> {
    let now = read(tx, id).await?;

    let parent = match changes.parent {
        None => now.parent.map(Uuid::from),
        Some(parent) => under(tx, now.sort, id, parent).await?,
    };

    let row = sqlx::query(&format!(
        "update terms
            set name = coalesce($2, name), parent = $3, updated_at = now()
          where id = $1 and deleted_at is null
         returning {COLUMNS}"
    ))
    .bind(Uuid::from(id))
    .bind(changes.name.as_deref())
    .bind(parent)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_term)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_FILED_UNDER_THAT)))
}

/// One term.
pub async fn read(tx: &mut Tx, id: TermId) -> Result<Term> {
    let row = sqlx::query(&format!(
        "select {COLUMNS} from terms where id = $1 and deleted_at is null"
    ))
    .bind(Uuid::from(id))
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_term)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_FILED_UNDER_THAT)))
}

/// Removes one. What was filed under it stays, filed under nothing — and what
/// was *inside* it comes up a level rather than disappearing with it.
pub async fn remove(tx: &mut Tx, id: TermId) -> Result<()> {
    let gone =
        sqlx::query("update terms set deleted_at = now() where id = $1 and deleted_at is null")
            .bind(Uuid::from(id))
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(NOTHING_IS_FILED_UNDER_THAT)));
    }

    // Children keep existing. A category thrown away does not take the six
    // under it, because what somebody meant was to remove one heading.
    sqlx::query("update terms set parent = null, updated_at = now() where parent = $1")
        .bind(Uuid::from(id))
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    sqlx::query("delete from filed_under where term_id = $1")
        .bind(Uuid::from(id))
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// Says what one writing is filed under, replacing whatever it was.
///
/// One statement to clear and one to write, in the caller's transaction: a
/// reader between them would see a writing filed under nothing, and the
/// transaction is what makes that impossible rather than unlikely.
pub async fn file_under(tx: &mut Tx, writing: Uuid, terms: &[Uuid]) -> Result<Vec<Term>> {
    sqlx::query("delete from filed_under where writing_id = $1")
        .bind(writing)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    for term in terms {
        sqlx::query(
            "insert into filed_under (writing_id, term_id) values ($1, $2)
             on conflict do nothing",
        )
        .bind(writing)
        .bind(term)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    let rows = sqlx::query(&format!(
        "select {COLUMNS} from terms
          where id in (select term_id from filed_under where writing_id = $1)
            and deleted_at is null
          order by name"
    ))
    .bind(writing)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter().map(a_term).collect()
}
