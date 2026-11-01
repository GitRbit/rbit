//! Strict, bounded projection of explicitly declared GET query fields.
use super::{WebError, WebResult};
use rbit_services::plugin_pages::{MAX_QUERY_BYTES, MAX_QUERY_FIELDS, PageQuery, RouteQuery};
pub(super) fn parse(raw: Option<&str>, schema: &RouteQuery) -> WebResult<PageQuery> {
    let invalid = || WebError::BadRequest("Invalid plugin query.".into());
    let Some(raw) = raw.filter(|raw| !raw.is_empty()) else {
        return Ok(PageQuery::default());
    };
    if raw.len() > MAX_QUERY_BYTES {
        return Err(invalid());
    }
    let mut pairs = Vec::new();
    for field in raw.split('&') {
        if pairs.len() >= MAX_QUERY_FIELDS {
            return Err(invalid());
        }
        let (key, value) = field.split_once('=').ok_or_else(invalid)?;
        let key = super::form_input::decode_component(key).map_err(|_| invalid())?;
        let value = super::form_input::decode_component(value).map_err(|_| invalid())?;
        if !schema.allows(&key) {
            return Err(invalid());
        }
        pairs.push((key, value));
    }
    PageQuery::new(pairs).map_err(|_| invalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn query_projection_rejects_unknown_duplicates_and_encoded_aliases() {
        let schema = RouteQuery::new(&["run", "branch"]).unwrap();
        assert_eq!(parse(Some("run=1&branch=release%2Fnext"), &schema).unwrap().get("branch"), Some("release/next"));
        for query in ["other=1", "run=1&%72un=2", "run=%", "run=%GG", "run=%FF", "run=1&&branch=main", "run=1&flag", "run=%0a"] {
            assert!(parse(Some(query), &schema).is_err(), "{query}");
        }
    }
}
