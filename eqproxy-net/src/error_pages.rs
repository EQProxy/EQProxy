//! Custom error pages (B-H15 · B-E18 · B-N14, ADR-0045).
//!
//! One page answers one or more status codes. The bytes are resolved at
//! snapshot time (a bad file rejects the configuration, it never fails per
//! request) and served from memory, so a page costs no I/O on the hot path.
//!
//! Two classes exist, on purpose:
//!
//! - **local failures** — EQProxy generated the status (no server, connect
//!   failure, header timeout, request rejected). Any page covering the code
//!   applies.
//! - **interception** — the backend answered, and the page only replaces that
//!   response when the entry declared `intercept`. This mirrors nginx
//!   `proxy_intercept_errors` / ingress-nginx `custom-http-errors`: an origin
//!   error is discarded, never appended to.
//!
//! The upstream connection of an intercepted exchange is closed, not reused:
//! its body was never read, so parking it would desync the next request.

use std::sync::Arc;

/// One resolved error page.
#[derive(Debug, Clone)]
pub struct ErrorPage {
    /// Codes this page answers (400-599).
    pub codes: Vec<u16>,
    /// Status actually sent when one is fixed (`error_page 404 =200`,
    /// `errorfile`'s status line, the 302 of an `errorloc`). `None` answers
    /// with the code that fired, which is what a page covering several codes
    /// normally means.
    pub status: Option<u16>,
    /// `Content-Type` of the payload.
    pub content_type: String,
    /// Payload bytes, already loaded and bounded.
    pub body: Arc<[u8]>,
    /// Redirect target instead of a body (HAProxy `errorloc`). When present,
    /// `status` is a 3xx and `body` is empty.
    pub location: Option<String>,
    /// Replace matching upstream responses too.
    pub intercept: bool,
}

/// Resolved pages of one listener. Empty = the built-in minimal errors.
#[derive(Debug, Clone, Default)]
pub struct ErrorPages {
    pages: Vec<ErrorPage>,
}

impl ErrorPages {
    /// Wraps already-resolved pages. The caller guarantees one page per code
    /// (configuration validation rejects duplicates), so lookup is a scan
    /// over at most a few dozen entries.
    #[must_use]
    pub fn from_pages(pages: Vec<ErrorPage>) -> Self {
        Self { pages }
    }

    /// Is the listener using custom pages at all?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// Page covering a status EQProxy generated locally.
    #[must_use]
    pub fn page_for(&self, code: u16) -> Option<&ErrorPage> {
        self.pages.iter().find(|page| page.codes.contains(&code))
    }

    /// Page that must replace a matching **upstream** response.
    #[must_use]
    pub fn intercept_for(&self, code: u16) -> Option<&ErrorPage> {
        self.pages
            .iter()
            .find(|page| page.intercept && page.codes.contains(&code))
    }

    /// Number of declared pages.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(codes: &[u16], status: Option<u16>, intercept: bool) -> ErrorPage {
        ErrorPage {
            codes: codes.to_vec(),
            status,
            content_type: "text/html".to_owned(),
            body: Arc::from(b"down\n".as_slice()),
            location: None,
            intercept,
        }
    }

    #[test]
    fn local_and_upstream_lookups_are_separate() {
        // A page without `intercept` answers local failures only, exactly the
        // nginx default (`proxy_intercept_errors off`).
        let pages = ErrorPages::from_pages(vec![page(&[502, 504], Some(500), false)]);
        assert_eq!(pages.page_for(504).map(|p| p.status), Some(Some(500)));
        assert!(pages.intercept_for(504).is_none());

        let pages = ErrorPages::from_pages(vec![page(&[502], None, true)]);
        assert!(pages.intercept_for(502).is_some());
        assert!(pages.page_for(503).is_none(), "a code outside the page");
        assert_eq!(pages.len(), 1);
        assert!(!pages.is_empty());
        assert!(ErrorPages::default().is_empty());
    }
}
