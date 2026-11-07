//! Catalog authorization precedes any immutable Git object admission or read.
use std::fmt;

use rbit_contracts::RepoId;

use super::{WikiCatalog, WikiCatalogPage, WikiHistory, WikiHistoryRequest, WikiPageRequest, WikiRevisionRequest, WikiRevisionStore};
use crate::{WikiContent, WikiPage};

pub enum WikiReadError<C, R> {
    NotFound,
    Catalog(C),
    Revision(R),
    Inconsistent,
}
impl<C, R> fmt::Debug for WikiReadError<C, R> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NotFound => "NotFound",
            Self::Catalog(_) => "Catalog(..)",
            Self::Revision(_) => "Revision(..)",
            Self::Inconsistent => "Inconsistent",
        })
    }
}

#[derive(Debug)]
pub enum WikiReadView {
    Empty { repository: RepoId },
    Page { page: WikiPage, content: WikiContent },
}

#[derive(Debug)]
pub struct WikiReader;

impl WikiReader {
    pub async fn page<C: WikiCatalog, R: WikiRevisionStore>(
        catalog: &C,
        revisions: &R,
        request: WikiPageRequest<'_>,
    ) -> Result<WikiReadView, WikiReadError<C::Error, R::Error>> {
        let selection = catalog.page(request).await.map_err(WikiReadError::Catalog)?;
        Self::selected(revisions, request.repository, selection).await
    }

    pub async fn revision<C: WikiCatalog, R: WikiRevisionStore>(
        catalog: &C,
        revisions: &R,
        request: WikiRevisionRequest,
    ) -> Result<WikiReadView, WikiReadError<C::Error, R::Error>> {
        let selection = catalog.revision(request).await.map_err(WikiReadError::Catalog)?;
        if selection.as_ref().and_then(|selection| selection.page.as_ref()).is_none() {
            return Err(WikiReadError::NotFound);
        }
        if selection
            .as_ref()
            .and_then(|selection| selection.page.as_ref())
            .is_some_and(|page| page.id != request.page || page.head.generation != request.generation)
        {
            return Err(WikiReadError::Inconsistent);
        }
        Self::selected(revisions, request.repository, selection).await
    }

    pub async fn history<C: WikiCatalog>(
        catalog: &C,
        request: WikiHistoryRequest,
    ) -> Result<WikiHistory, WikiReadError<C::Error, std::convert::Infallible>> {
        catalog.history(request).await.map_err(WikiReadError::Catalog)?.ok_or(WikiReadError::NotFound)
    }

    async fn selected<C, R: WikiRevisionStore>(
        revisions: &R,
        repository: RepoId,
        selection: Option<WikiCatalogPage>,
    ) -> Result<WikiReadView, WikiReadError<C, R::Error>> {
        let selection = selection.ok_or(WikiReadError::NotFound)?;
        if selection.repository != repository {
            return Err(WikiReadError::Inconsistent);
        }
        let Some(head) = selection.head else {
            return if selection.page.is_none() { Ok(WikiReadView::Empty { repository }) } else { Err(WikiReadError::Inconsistent) };
        };
        let page = selection.page.ok_or(WikiReadError::NotFound)?;
        if page.repository != repository || page.head != head {
            return Err(WikiReadError::Inconsistent);
        }
        let content = revisions.read_page(selection.store, &page).await.map_err(WikiReadError::Revision)?;
        Ok(WikiReadView::Page { page, content })
    }
}
