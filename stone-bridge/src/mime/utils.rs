use crate::logger::log;
use crate::mime::message::ContentType;
use crate::mime::{Body, COLON_BYTE, CRLF_BYTES, MimeEntity, MimeError};
use base64::{Engine as _, engine::general_purpose::STANDARD};

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

/// Decodes RFC 2047 encoded-words in a header, e.g. "=?UTF-8?B?SGVsbG8=?= world".
pub fn decode_mime_subject(subject: &str) -> String {
    let mut out = String::new();
    let mut rest = subject;
    let mut prev_was_word = false;

    while let Some(pos) = rest.find("=?") {
        let (plain, candidate) = rest.split_at(pos);
        match decode_word(candidate) {
            Some((text, len)) => {
                // Whitespace between two adjacent encoded-words is ignored.
                if !(prev_was_word && plain.trim().is_empty()) {
                    out.push_str(plain);
                }
                out.push_str(&text);
                rest = &candidate[len..];
                prev_was_word = true;
            }
            None => {
                out.push_str(plain);
                out.push_str("=?");
                rest = &candidate[2..];
                prev_was_word = false;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_word(s: &str) -> Option<(String, usize)> {
    let mut parts = s.strip_prefix("=?")?.splitn(3, '?');
    let charset = parts.next()?;
    let enc = parts.next()?;
    let tail = parts.next()?;
    let end = tail.find("?=")?;
    let text = &tail[..end];

    let decoded = match enc {
        "B" | "b" => decode_base64(text.as_bytes()).ok()?,
        "Q" | "q" => decode_quoted_printable(text.replace('_', " ").as_bytes()).ok()?,
        _ => return None,
    };

    let consumed = 2 + charset.len() + 1 + enc.len() + 1 + end + 2;
    Some((decoded, consumed))
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

pub fn to_pre(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<pre>{}</pre>", escaped)
}

/// Content-ID headers look like `<abc@host>`; html references `cid:abc@host`.
pub fn normalize_cid(id: &str) -> &str {
    id.trim().trim_start_matches('<').trim_end_matches('>')
}

/// Decodes base64, ignoring the whitespace / line breaks used to wrap it.
pub fn decode_base64(input: &[u8]) -> Result<String, MimeError> {
    let compact: Vec<u8> = input
        .iter()
        .copied()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    STANDARD.decode(compact).map_err(MimeError::Base64Error).map(|s| String::from_utf8_lossy(&s).into_owned())
}

pub fn clean_mime_base64(mime_base64: &str) -> String {
    let split: Vec<&str> = mime_base64.split("\r\n").collect();
    let no_crlf = split.join("");
    no_crlf.chars().filter(|c| !c.is_whitespace()).collect()
}

// FIXME does not handle soft breaks
pub fn decode_quoted_printable(input: &[u8]) -> Result<String, MimeError> {
    let mut bytes = Vec::with_capacity(input.len());
    let mut i = 0;

    while i < input.len() {
        if input[i] == b'=' {
            // 1. Check for soft line breaks (=\r\n or =\n or trailing =)
            if i + 1 == input.len() {
                break; // Trailing '=' at the very end of input
            }
            if input[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if input[i + 1] == b'\r' {
                if i + 2 < input.len() && input[i + 2] == b'\n' {
                    i += 3;
                } else {
                    i += 2;
                }
                continue;
            }

            // 2. Decode hex pair without heap allocations
            if i + 2 < input.len() {
                let hi = (input[i + 1] as char).to_digit(16);
                let lo = (input[i + 2] as char).to_digit(16);

                if let (Some(h), Some(l)) = (hi, lo) {
                    bytes.push(((h << 4) | l) as u8);
                    i += 3;
                    continue;
                }
            }

            // 3. Fallback per RFC 2045: if invalid hex, push the '=' literally
            bytes.push(b'=');
            i += 1;
        } else {
            bytes.push(input[i]);
            i += 1;
        }
    }

    String::from_utf8(bytes).map_err(|e| {
        log::error!("{}", e);
        MimeError::QuotedPrintableError
    })
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
    fn qp_soft_line_break_is_removed() {
        assert_eq!(decode_quoted_printable(b"foo=\r\nbar").unwrap(), "foobar");
        assert_eq!(decode_quoted_printable(b"foo=\nbar").unwrap(), "foobar");
    }

    #[test]
    fn qp_hex_escapes_are_decoded() {
        assert_eq!(
            decode_quoted_printable(b"src=3D\"cid:x\"").unwrap(),
            "src=\"cid:x\""
        );
        assert_eq!(decode_quoted_printable(b"=c3=a9").unwrap(), "é");
        assert_eq!(decode_quoted_printable(b"=C3=A9").unwrap(), "é");
    }

    #[test]
    fn qp_hard_line_breaks_are_preserved() {
        assert_eq!(
            decode_quoted_printable(b"a\r\nb\r\n").unwrap(),
            "a\r\nb\r\n"
        );
    }

    #[test]
    fn base64_ignores_line_wrapping() {
        assert_eq!(
            decode_base64(b"aGVs\r\nbG8g\r\nd29y\r\nbGQ=\r\n").unwrap(),
            "hello world"
        );
        assert!(matches!(
            decode_base64(b"!!!!"),
            Err(MimeError::Base64Error(_))
        ));
    }

    #[test]
    fn split_headers_body_without_headers() {
        let (headers, body) = split_headers_body(b"\r\nbody").unwrap();
        assert!(headers.is_empty());
        assert_eq!(body, b"body");
    }
}
