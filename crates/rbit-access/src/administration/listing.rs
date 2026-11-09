//! Administration listings: request normalisation and page arithmetic.

/// Rows per page in administration listings.
pub const PAGE_SIZE: u32 = 30;
/// Longest accepted search text; longer input is truncated.
pub const MAX_SEARCH_LEN: usize = 100;

/// One page of a listing plus the total number of matching rows.
#[derive(Debug, Clone)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// Which page of a listing to fetch, and an optional case-insensitive substring filter.
#[derive(Debug, Clone, Copy)]
pub struct ListFilter<'a> {
    pub search: &'a str,
    pub limit: i64,
    pub offset: i64,
}

/// Which page of a listing to show and what to filter by, as requested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListQuery {
    pub search: String,
    /// 1-based; `0` is treated as `1`.
    pub page: u32,
}

impl ListQuery {
    /// The trimmed, length-capped search text.
    pub fn search(&self) -> &str {
        let search = self.search.trim();
        match search.char_indices().nth(MAX_SEARCH_LEN) {
            Some((end, _)) => &search[..end],
            None => search,
        }
    }

    pub fn page(&self) -> u32 {
        self.page.max(1)
    }

    /// The page window and search text the store filters by.
    pub fn filter(&self) -> ListFilter<'_> {
        ListFilter { search: self.search(), limit: i64::from(PAGE_SIZE), offset: i64::from(self.page() - 1) * i64::from(PAGE_SIZE) }
    }
}

/// One page of an administration listing.
#[derive(Debug, Clone)]
pub struct Paged<T> {
    pub items: Vec<T>,
    /// Matching rows across all pages.
    pub total: u64,
    /// 1-based.
    pub page: u32,
    pub per_page: u32,
}

impl<T> Paged<T> {
    pub fn new(listing: Listing<T>, query: &ListQuery) -> Self {
        Self { items: listing.items, total: listing.total.try_into().unwrap_or(0), page: query.page(), per_page: PAGE_SIZE }
    }

    pub fn has_previous(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        u64::from(self.page) * u64::from(self.per_page) < self.total
    }

    /// 1-based index of the first item on this page (0 when the page is empty).
    pub fn first_index(&self) -> u64 {
        if self.items.is_empty() { 0 } else { u64::from(self.page - 1) * u64::from(self.per_page) + 1 }
    }

    /// 1-based index of the last item on this page (0 when the page is empty).
    pub fn last_index(&self) -> u64 {
        if self.items.is_empty() { 0 } else { self.first_index() + self.items.len() as u64 - 1 }
    }
}
