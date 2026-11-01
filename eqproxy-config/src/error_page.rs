//! Parsing of a `format = "raw"` error page (ADR-0045).
//!
//! HAProxy's `errorfile <code> <file>` does not store a payload: the file is
//! a **complete HTTP response** (status line, headers, body). EQProxy keeps
//! that spelling working by parsing the file at load time and serving its
//! status, content type and body, so an operator can move an `errorfile`
//! without rewriting it.
//!
//! The parser is strict on purpose: a file whose declared `Content-Length`
//! disagrees with the bytes it carries is rejected instead of served short,
//! and a chunked response is refused because the page is served as identity.

/// A parsed raw error page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawErrorPage {
    /// Status code from the status line (400-599).
    pub status: u16,
    /// `Content-Type` header, when the file declares one.
    pub content_type: Option<String>,
    /// Payload (everything after the head terminator).
    pub body: Vec<u8>,
}

/// Parses a complete HTTP response file.
///
/// # Errors
///
/// The message names the problem and the fix (the caller adds the path).
pub fn parse_raw_error_page(bytes: &[u8]) -> Result<RawErrorPage, String> {
    let Some(head_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err(
            "a raw error page must be a complete HTTP response: no CRLFCRLF ending the head; \
             add the headers and the blank line, or use format = \"body\""
                .to_owned(),
        );
    };
    let head = &bytes[..head_end];
    let body = bytes[head_end + 4..].to_vec();
    let mut lines = head
        .split(|&b| b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line));
    let Some(status_line) = lines.next() else {
        return Err("a raw error page is empty".to_owned());
    };
    let status_line = std::str::from_utf8(status_line)
        .map_err(|_| "the status line is not valid UTF-8".to_owned())?;
    let mut parts = status_line.splitn(3, ' ');
    let version = parts.next().unwrap_or_default();
    if !version.starts_with("HTTP/1.") {
        return Err(format!(
            "the status line starts with \"{version}\": expected HTTP/1.1 503 …"
        ));
    }
    let code = parts.next().unwrap_or_default();
    let status: u16 = code.parse().map_err(|_| {
        format!(
            "the status line has no numeric code (\"{status_line}\"); expected \"HTTP/1.1 503 Service Unavailable\""
        )
    })?;
    if !(400..=599).contains(&status) {
        return Err(format!(
            "the status line answers {status}: an error page must be 400-599"
        ));
    }

    let mut content_type = None;
    let mut declared_length: Option<usize> = None;
    for line in lines {
        let line =
            std::str::from_utf8(line).map_err(|_| "a header line is not valid UTF-8".to_owned())?;
        if line.trim().is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err(format!(
                "\"{line}\" is not a header (expected `Name: value`)"
            ));
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-type") {
            content_type = Some(value.to_owned());
        } else if name.eq_ignore_ascii_case("content-length") {
            declared_length = Some(
                value
                    .parse()
                    .map_err(|_| format!("content-length \"{value}\" is not a number"))?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(
                "a raw error page is served identity: remove transfer-encoding (the proxy \
                 sets the length)"
                    .to_owned(),
            );
        }
    }
    if let Some(declared) = declared_length
        && declared != body.len()
    {
        return Err(format!(
            "content-length says {declared} bytes but the file carries {}: fix the body or the header",
            body.len()
        ));
    }
    Ok(RawErrorPage {
        status,
        content_type,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_complete_response_is_parsed_into_status_type_and_body() {
        let raw = b"HTTP/1.1 503 Service Unavailable\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: 6\r\n\r\n<p>no\n";
        let page = parse_raw_error_page(raw).expect("valid raw page");
        assert_eq!(page.status, 503);
        assert_eq!(
            page.content_type.as_deref(),
            Some("text/html; charset=utf-8")
        );
        assert_eq!(page.body, b"<p>no\n");
    }

    #[test]
    fn a_body_without_headers_is_not_a_raw_page() {
        let err = parse_raw_error_page(b"<h1>down</h1>\n").expect_err("plain body");
        assert!(err.contains("CRLFCRLF"), "{err}");
    }

    #[test]
    fn a_status_line_must_be_an_error() {
        let err =
            parse_raw_error_page(b"HTTP/1.1 200 OK\r\n\r\nok").expect_err("200 is not an error");
        assert!(err.contains("400-599"), "{err}");
        let err = parse_raw_error_page(b"GARBAGE\r\n\r\n").expect_err("no version");
        assert!(err.contains("HTTP/1.1"), "{err}");
    }

    #[test]
    fn a_length_that_disagrees_with_the_body_is_refused() {
        let err =
            parse_raw_error_page(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 99\r\n\r\nshort")
                .expect_err("truncated file");
        assert!(err.contains("content-length says 99"), "{err}");
        let err = parse_raw_error_page(
            b"HTTP/1.1 502 Bad Gateway\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
        )
        .expect_err("chunked");
        assert!(err.contains("identity"), "{err}");
    }
}
