//! Keyset pagination on `id`, newest first (api.md AC-4). Repository ports
//! take a [`PageRequest`] and return a [`Page`]; adapters handle the cursor encoding.

use uuid::Uuid;

use crate::error::{FieldCode, FieldError};

/// Page size when the client sends no `limit`
/// ([Stripe pagination](https://docs.stripe.com/api/pagination); api.md AC-4).
pub const DEFAULT_LIMIT: u32 = 10;

/// Largest accepted `limit`; anything above returns `422`
/// ([Stripe pagination](https://docs.stripe.com/api/pagination); api.md AC-4).
pub const MAX_LIMIT: u32 = 100;

/// Smallest accepted `limit` ([Stripe pagination](https://docs.stripe.com/api/pagination):
/// "between 1 and 100").
pub const MIN_LIMIT: u32 = 1;

/// A validated page request: at most `limit` rows with `id < after`, newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    limit: u32,
    after: Option<Uuid>,
}

impl PageRequest {
    /// `limit` defaults to [`DEFAULT_LIMIT`]. Outside `MIN_LIMIT..=MAX_LIMIT` it is an
    /// `out_of_range` error on the `limit` field.
    pub fn new(limit: Option<u32>, after: Option<Uuid>) -> Result<Self, FieldError> {
        let limit = limit.unwrap_or(DEFAULT_LIMIT);
        if !(MIN_LIMIT..=MAX_LIMIT).contains(&limit) {
            return Err(FieldError::new(
                "limit",
                FieldCode::OutOfRange,
                format!("must be between {MIN_LIMIT} and {MAX_LIMIT}"),
            ));
        }
        Ok(Self { limit, after })
    }

    pub fn limit(self) -> u32 {
        self.limit
    }

    /// The last `id` of the previous page, if any.
    pub fn after(self) -> Option<Uuid> {
        self.after
    }

    /// Rows a repository fetches: one more than the page, to learn whether a next page
    /// exists without a count query.
    pub fn fetch_limit(self) -> i64 {
        i64::from(self.limit) + 1
    }
}

/// One page of results. `next` is the key to pass as `after` for the next page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<Uuid>,
}

impl<T> Page<T> {
    /// Builds a page from up to `fetch_limit()` rows, already ordered newest first. When
    /// there is an extra row, it is dropped and the last kept row's key becomes `next`.
    pub fn from_overfetch(
        mut rows: Vec<T>,
        request: PageRequest,
        key: impl Fn(&T) -> Uuid,
    ) -> Self {
        let limit = request.limit as usize;
        let next = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(key)
        } else {
            None
        };
        Self { items: rows, next }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            next: self.next,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_defaults_and_bounds() {
        assert_eq!(PageRequest::new(None, None).unwrap().limit(), 10);
        assert_eq!(PageRequest::new(Some(1), None).unwrap().limit(), 1);
        assert_eq!(PageRequest::new(Some(100), None).unwrap().limit(), 100);
        for bad in [0, 101] {
            let err = PageRequest::new(Some(bad), None).unwrap_err();
            assert_eq!(
                (err.field.as_str(), err.code),
                ("limit", FieldCode::OutOfRange)
            );
        }
    }

    #[test]
    fn overfetch_sets_next_to_the_last_kept_key() {
        let ids: Vec<Uuid> = (1..=4).map(Uuid::from_u128).collect();
        let request = PageRequest::new(Some(3), None).unwrap();
        assert_eq!(request.fetch_limit(), 4);

        let page = Page::from_overfetch(ids.clone(), request, |id| *id);
        assert_eq!(page.items, ids[..3]);
        assert_eq!(page.next, Some(ids[2]));

        let last = Page::from_overfetch(ids[..2].to_vec(), request, |id| *id);
        assert_eq!(last.items.len(), 2);
        assert_eq!(last.next, None);
    }
}
