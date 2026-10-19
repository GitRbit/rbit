//! `?page=` (1-based) and `?per_page=` (default and maximum from `[api]`), answered with
//! RFC 8288 `Link` headers (`first`, `prev`, `next`, `last`) and `X-Total-Count`.
use axum::{
    Json,
    extract::{FromRequestParts, Query},
    http::{HeaderName, HeaderValue, header, request::Parts},
    response::{IntoResponse, Response},
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Serialize;

use super::error::{ApiError, ApiResult};
use crate::AppState;

const TOTAL_COUNT: HeaderName = HeaderName::from_static("x-total-count");

/// The requested page, and what is needed to link to its neighbours.
#[derive(Debug, Clone)]
pub struct Pager {
    pub page: u32,
    pub per_page: u32,
    /// Absolute URL of the request without its query.
    url: String,
    /// Query parameters other than `page` and `per_page`, in request order.
    other: Vec<(String, String)>,
}

impl Pager {
    /// Parses `page`/`per_page` from `query`. `per_page` is clamped to `1..=max`; a page
    /// below 1 is page 1; anything that is not a number is a 422.
    pub fn parse(pairs: Vec<(String, String)>, url: String, default_per_page: u32, max_per_page: u32) -> ApiResult<Self> {
        let mut page = 1;
        let mut per_page = default_per_page;
        let mut other = Vec::new();
        for (key, value) in pairs {
            match key.as_str() {
                "page" => page = value.parse::<u32>().map_err(|_| ApiError::invalid("page", "page must be a positive number"))?.max(1),
                "per_page" => {
                    per_page = value.parse::<u32>().map_err(|_| ApiError::invalid("per_page", "per_page must be a positive number"))?;
                }
                _ => other.push((key, value)),
            }
        }
        Ok(Self { page, per_page: per_page.clamp(1, max_per_page.max(1)), url, other })
    }

    /// Items to skip.
    pub fn offset(&self) -> usize {
        usize::try_from(u64::from(self.page - 1) * u64::from(self.per_page)).unwrap_or(usize::MAX)
    }

    pub fn limit(&self) -> usize {
        usize::try_from(self.per_page).unwrap_or(usize::MAX)
    }

    /// This page of a complete list.
    pub fn page_of<T: Serialize>(&self, items: Vec<T>) -> Paged<T> {
        let total = u64::try_from(items.len()).unwrap_or(u64::MAX);
        let items = items.into_iter().skip(self.offset()).take(self.limit()).collect();
        self.paged(items, Some(total), false)
    }

    /// A page whose total is `total` (if known); `has_next` matters only without a total.
    pub fn paged<T: Serialize>(&self, items: Vec<T>, total: Option<u64>, has_next: bool) -> Paged<T> {
        let rels = link_pages(self.page, self.per_page, total, has_next);
        let link = (!rels.is_empty())
            .then(|| rels.iter().map(|(rel, page)| format!("<{}>; rel=\"{rel}\"", self.url_for(*page))).collect::<Vec<_>>().join(", "));
        Paged { items, total, link }
    }

    fn url_for(&self, page: u32) -> String {
        let mut query: Vec<String> = self.other.iter().map(|(key, value)| format!("{}={}", encode(key), encode(value))).collect();
        query.push(format!("per_page={}", self.per_page));
        query.push(format!("page={page}"));
        format!("{}?{}", self.url, query.join("&"))
    }
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

/// The `Link` relations for `page`: `first` and `prev` after the first page; `next`
/// while there are more; `last` when the total is known.
pub fn link_pages(page: u32, per_page: u32, total: Option<u64>, has_next: bool) -> Vec<(&'static str, u32)> {
    let mut rels = Vec::new();
    if page > 1 {
        rels.push(("first", 1));
        rels.push(("prev", page - 1));
    }
    match total {
        Some(total) => {
            let last = u32::try_from(total.div_ceil(u64::from(per_page.max(1))).max(1)).unwrap_or(u32::MAX);
            if page < last {
                rels.push(("next", page + 1));
                rels.push(("last", last));
            }
        }
        None if has_next => rels.push(("next", page + 1)),
        None => {}
    }
    rels
}

impl FromRequestParts<AppState> for Pager {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let Query(pairs) = Query::<Vec<(String, String)>>::from_request_parts(parts, state).await?;
        let url = format!("{}{}", state.config.server.base_url(), parts.uri.path());
        Self::parse(pairs, url, state.config.api.default_page_size, state.config.api.max_page_size)
    }
}

/// One page of a list: the JSON array plus `Link` and `X-Total-Count`.
#[derive(Debug)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: Option<u64>,
    pub link: Option<String>,
}

impl<T: Serialize> IntoResponse for Paged<T> {
    fn into_response(self) -> Response {
        let mut response = Json(self.items).into_response();
        let headers = response.headers_mut();
        if let Some(total) = self.total {
            headers.insert(TOTAL_COUNT, HeaderValue::from(total));
        }
        if let Some(link) = self.link.and_then(|link| HeaderValue::from_str(&link).ok()) {
            headers.insert(header::LINK, link);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pager(query: &[(&str, &str)]) -> ApiResult<Pager> {
        let pairs = query.iter().map(|(key, value)| ((*key).to_owned(), (*value).to_owned())).collect();
        Pager::parse(pairs, "https://git.example.com/api/v1/user/repos".to_owned(), 30, 100)
    }

    #[test]
    fn link_arithmetic() {
        assert_eq!(link_pages(1, 30, Some(0), false), [], "an empty list is one page");
        assert_eq!(link_pages(1, 30, Some(30), false), []);
        assert_eq!(link_pages(1, 30, Some(31), false), [("next", 2), ("last", 2)]);
        assert_eq!(link_pages(2, 30, Some(31), false), [("first", 1), ("prev", 1)]);
        assert_eq!(link_pages(2, 10, Some(45), false), [("first", 1), ("prev", 1), ("next", 3), ("last", 5)]);
        assert_eq!(link_pages(5, 10, Some(45), false), [("first", 1), ("prev", 4)]);
        assert_eq!(link_pages(9, 10, Some(45), false), [("first", 1), ("prev", 8)], "past the end links back");
        assert_eq!(link_pages(1, 10, None, true), [("next", 2)], "without a total there is no last");
        assert_eq!(link_pages(3, 10, None, false), [("first", 1), ("prev", 2)]);
    }

    #[test]
    fn parses_and_clamps_page_parameters() {
        let default = pager(&[]).unwrap();
        assert_eq!((default.page, default.per_page, default.offset()), (1, 30, 0));
        let explicit = pager(&[("page", "3"), ("per_page", "20")]).unwrap();
        assert_eq!((explicit.page, explicit.per_page, explicit.offset()), (3, 20, 40));
        assert_eq!(pager(&[("per_page", "1000")]).unwrap().per_page, 100, "capped at the maximum");
        assert_eq!(pager(&[("per_page", "0"), ("page", "0")]).unwrap().per_page, 1);
        assert_eq!(pager(&[("page", "0")]).unwrap().page, 1);
        let error = pager(&[("page", "two")]).unwrap_err();
        assert_eq!((error.status.as_u16(), error.errors[0].field.as_str()), (422, "page"));
        assert!(pager(&[("per_page", "-1")]).is_err());
    }

    #[test]
    fn pages_carry_links_that_keep_other_parameters() {
        let pager = pager(&[("sha", "feature/x y"), ("page", "2"), ("per_page", "2")]).unwrap();
        let page = pager.page_of(vec![1, 2, 3, 4, 5]);
        assert_eq!(page.items, [3, 4]);
        assert_eq!(page.total, Some(5));
        let base = "https://git.example.com/api/v1/user/repos?sha=feature%2Fx%20y&per_page=2";
        assert_eq!(
            page.link.as_deref(),
            Some(format!("<{base}&page=1>; rel=\"first\", <{base}&page=1>; rel=\"prev\", <{base}&page=3>; rel=\"next\", <{base}&page=3>; rel=\"last\"").as_str())
        );

        let response = page.into_response();
        assert_eq!(response.headers().get("x-total-count").unwrap(), "5");
        assert!(response.headers().get(header::LINK).is_some());
        let single = super::tests::pager(&[]).unwrap().page_of(vec!["only"]);
        assert_eq!((single.link, single.total), (None, Some(1)));
    }
}
