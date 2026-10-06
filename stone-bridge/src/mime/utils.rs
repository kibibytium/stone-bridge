use crate::mime::message::ContentType;
use crate::mime::{Body, COLON_BYTE, CRLF_BYTES, MimeEntity, MimeError};

pub fn split_crlf(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let pos = find(input, CRLF_BYTES)?;
    Some((&input[..pos], &input[pos + 2..]))
}

pub fn split_colon(input: &[u8]) -> Option<(&[u8], &[u8])> {
    let pos = input.iter().position(|&c| c == COLON_BYTE)?;
    Some((&input[..pos], &input[pos + 1..]))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Splits an entity into its header block and body.
/// The blank line separating them belongs to neither.
/// An entity starting with a blank line has no headers (RFC 2046 5.1.1).
pub fn split_headers_body(content: &[u8]) -> Result<(&[u8], &[u8]), MimeError> {
    if let Some(body) = content.strip_prefix(CRLF_BYTES) {
        return Ok((&content[..0], body));
    }
    let idx = find(content, b"\r\n\r\n").ok_or(MimeError::SyntaxError(String::from(
        "Could not find end of headers",
    )))?;
    Ok((&content[..idx + 2], &content[idx + 4..]))
}

/// Splits a multipart body into its parts (RFC 2046 5.1.1).
///
/// The preamble before the first delimiter and the epilogue after the
/// close delimiter (`--boundary--`) are discarded. The CRLF preceding a
/// delimiter is part of the delimiter, not of the part.
pub fn split_multipart<'a>(body: &'a [u8], boundary: &[u8]) -> Vec<&'a [u8]> {
    let mut parts = Vec::new();
    if boundary.is_empty() {
        return parts;
    }

    let mut dash_boundary = b"--".to_vec();
    dash_boundary.extend_from_slice(boundary);
    let mut delimiter = CRLF_BYTES.to_vec();
    delimiter.extend_from_slice(&dash_boundary);

    // Locate the first delimiter, which may sit at the very start of the body
    let mut rest = if body.starts_with(&dash_boundary) {
        &body[dash_boundary.len()..]
    } else {
        match find(body, &delimiter) {
            Some(pos) => &body[pos + delimiter.len()..],
            None => return parts,
        }
    };

    loop {
        // close delimiter: everything after is epilogue
        if rest.starts_with(b"--") {
            return parts;
        }
        // skip transport padding up to the end of the delimiter line
        let Some((_, after)) = split_crlf(rest) else {
            return parts;
        };
        match find(after, &delimiter) {
            Some(pos) => {
                parts.push(&after[..pos]);
                rest = &after[pos + delimiter.len()..];
            }
            None => {
                // unterminated multipart: keep what we have
                parts.push(after);
                return parts;
            }
        }
    }
}

pub fn make_body(
    content_type: &ContentType,
    bytes: &[u8],
    boundary: Option<&str>,
) -> Result<Body, MimeError> {
    match content_type {
        ContentType::TextPlain
        | ContentType::TextHtml
        | ContentType::ApplicationOctetStream
        | ContentType::ApplicationPdf
        | ContentType::ImagePNG
        | ContentType::ImageJPEG => Ok(Body::Leaf(bytes.to_vec())),
        ContentType::MultipartAlternative
        | ContentType::MultipartRelated
        | ContentType::MultipartMixed => {
            let boundary = boundary.ok_or(MimeError::MissingBoundary)?;
            let entities = split_multipart(bytes, boundary.as_bytes())
                .into_iter()
                .map(MimeEntity::new)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Body::Multipart(entities))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_multipart_skips_preamble_and_epilogue() {
        let body =
            b"preamble\r\n--b\r\nA: 1\r\n\r\none\r\n--b  \r\n\r\ntwo\r\n--b--\r\nepilogue\r\n.\r\n";
        let parts = split_multipart(body, b"b");
        assert_eq!(parts, vec![&b"A: 1\r\n\r\none"[..], &b"\r\ntwo"[..]]);
    }

    #[test]
    fn split_multipart_delimiter_at_start() {
        let body = b"--b\r\n\r\nonly\r\n--b--";
        assert_eq!(split_multipart(body, b"b"), vec![&b"\r\nonly"[..]]);
    }

    #[test]
    fn split_headers_body_without_headers() {
        let (headers, body) = split_headers_body(b"\r\nbody").unwrap();
        assert!(headers.is_empty());
        assert_eq!(body, b"body");
    }
}
