//! Shared bounded listing controls. Search and navigation work without JavaScript.
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;
use std::fmt::Write;

pub const LIST_PAGE_SIZE: usize = 100;

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default)]
pub struct ListQuery {
    pub q: String,
    pub page: usize,
}

impl ListQuery {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.q = self.q.trim().chars().take(100).collect();
        self
    }
}

#[derive(Debug)]
pub struct Pagination {
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub start: usize,
    pub end: usize,
    pub query: String,
    pub action: String,
    pub extra: Vec<(String, String)>,
    pub previous: Option<String>,
    pub next: Option<String>,
}

impl Pagination {
    pub fn new(total: usize, query: &ListQuery, action: String, extra: Vec<(String, String)>) -> Self {
        let pages = total.div_ceil(LIST_PAGE_SIZE).max(1);
        let page = query.page.clamp(1, pages);
        let start = (page - 1) * LIST_PAGE_SIZE;
        let mut value = Self {
            total,
            page,
            pages,
            start,
            end: start.saturating_add(LIST_PAGE_SIZE).min(total),
            query: query.q.clone(),
            action,
            extra,
            previous: None,
            next: None,
        };
        value.previous = (page > 1).then(|| value.url(page - 1));
        value.next = (page < pages).then(|| value.url(page + 1));
        value
    }
    pub fn url(&self, page: usize) -> String {
        let encode = |s: &str| utf8_percent_encode(s, NON_ALPHANUMERIC).to_string();
        let mut url = format!("{}?q={}&page={page}", self.action, encode(&self.query));
        for (name, value) in &self.extra {
            let _ = write!(url, "&{}={}", encode(name), encode(value));
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_preserves_query_and_snapshot() {
        let p = Pagination::new(
            205,
            &ListQuery { q: "a & #".into(), page: usize::MAX },
            "/files".into(),
            vec![("revision".into(), "a/b".into())],
        );
        assert_eq!((p.page, p.start, p.end), (3, 200, 205));
        assert!(p.next.is_none());
        let previous = p.previous.unwrap();
        assert!(previous.contains("q=a%20%26%20%23"));
        assert!(previous.contains("revision=a%2Fb"));
        let empty = Pagination::new(0, &ListQuery::default(), "/files".into(), vec![]);
        assert_eq!((empty.start, empty.end, empty.pages), (0, 0, 1));
    }
}
