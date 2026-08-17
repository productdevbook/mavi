//! Site-scoped taxonomy: categories, tags, trees and content assignments.
//!
//! Terms are deliberately one model with a closed `kind`: categories may have
//! category parents, tags are always flat. Assignment replacement is an
//! application command and runs in the same scoped transaction as its audit
//! receipt.

mod assignments;
mod terms;

use mavi_core::{ContentId, Page, Result, SiteContext, TermId};
use mavi_storage::SiteTx;

pub use assignments::{
    ContentTermAssignment, ContentTermAssignmentListFilter, ReplaceContentTerms,
};
pub use terms::{
    CreateTerm, TERM_ASSIGNMENT_LIMIT, TERM_CYCLE, TERM_KIND_INVALID, TERM_LANGUAGE_INVALID,
    TERM_NAME_INVALID, TERM_NOT_FOUND, TERM_PARENT_INVALID, TERM_PARENT_LANGUAGE_INVALID,
    TERM_PARENT_NOT_FOUND, TERM_SLUG_INVALID, TERM_SLUG_TAKEN, Term, TermKind, TermListFilter,
    UpdateTerm,
};

pub const TAXONOMY: &str = "taxonomy";

#[must_use]
pub fn api() -> mavi_contract::Api {
    let mut api = mavi_contract::Api::new(terms::endpoints());
    api.endpoints.extend(assignments::endpoints());
    let mut shapes = terms::shapes();
    shapes.extend(assignments::shapes());
    api.with_shapes(shapes)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TaxonomyService;

impl TaxonomyService {
    pub async fn list_terms(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &TermListFilter,
    ) -> Result<Page<Term>> {
        terms::list(tx, context, filter).await
    }

    pub async fn create_term(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateTerm,
    ) -> Result<Term> {
        terms::create(tx, context, input).await
    }

    pub async fn get_term(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: TermId,
    ) -> Result<Term> {
        terms::get(tx, context, id).await
    }

    pub async fn update_term(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: TermId,
        input: &UpdateTerm,
    ) -> Result<Term> {
        terms::update(tx, context, id, input).await
    }

    pub async fn delete_term(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: TermId,
    ) -> Result<()> {
        terms::delete(tx, context, id).await
    }

    pub async fn list_content_terms(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        content_id: ContentId,
    ) -> Result<Vec<Term>> {
        assignments::list_for_content(tx, context, content_id).await
    }

    pub async fn replace_content_terms(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        content_id: ContentId,
        input: &ReplaceContentTerms,
    ) -> Result<Vec<Term>> {
        assignments::replace_for_content(tx, context, content_id, input).await
    }

    pub async fn list_term_content(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        term_id: TermId,
        filter: &ContentTermAssignmentListFilter,
    ) -> Result<Page<ContentTermAssignment>> {
        assignments::list_content_for_term(tx, context, term_id, filter).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_api_is_canonical_and_site_scoped() {
        let api = api();
        api.validate().expect("taxonomy API contract");
        assert!(api.endpoints.iter().all(|endpoint| {
            endpoint.path.starts_with("/api/v1/") && endpoint.scope == mavi_contract::Scope::Site
        }));
    }
}
