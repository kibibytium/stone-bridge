mod message;
mod utils;

use core::str;
use regex::Regex;
use std::io::BufRead;
use std::{collections::HashMap, iter::Peekable};

use crate::logger::log;
use crate::mime::message::{ContentTransferEncoding, ContentType};
use crate::mime::utils::{make_body, split_colon, split_crlf, split_headers_body};

const CRLF_BYTES: &[u8] = b"\r\n";
const COLON_BYTE: u8 = b':';
const DEFAULT_CONTENT_TYPE: ContentType = ContentType::TextPlain;
const DEFAULT_TRANSFER_ENCODING: ContentTransferEncoding = ContentTransferEncoding::SevenBit;

const CONTENT_TYPE_HEADER: &'static str = "Content-Type";
const CONTENT_TRANSFER_ENCODING_HEADER: &'static str = "Content-Transfer-Encoding";

const CID_REGEX: &'static str = r#"src="(cid:)([^"]+)""#;

#[derive(Debug, Clone)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}

impl Header {
    pub fn new(line: &[u8]) -> Result<Self, MimeError> {
        let (name, rest) = split_colon(line).ok_or({ MimeError::ColonExtractorError })?;

        Ok(Self {
            name: String::from_utf8_lossy(name.trim_ascii()).to_string(),
            value: rest.trim_ascii().to_vec(),
        })
    }

    pub fn get(&self) -> String {
        String::from_utf8_lossy(&self.value).to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Headers {
    pub entries: Vec<Header>,
}

impl Headers {
    fn new(bytes: &[u8]) -> Result<Headers, MimeError> {
        let mut headers = Headers {
            entries: Vec::new(),
        };

        let mut rest = bytes;
        loop {
            if rest.is_empty() {
                return Ok(headers);
            }
            // last line might or might not end with CRLF
            let (line, after) = split_crlf(rest).unwrap_or((rest, &rest[rest.len()..]));

            // parsing done
            if line.is_empty() {
                // FIXME could be missing some data in after?
                return Ok(headers);
            }

            // line break in one header line
            // headers must have at least one element: RFC 822, 2045
            if line[0] == b' ' || line[0] == b'\t' {
                let last_header = headers
                    .entries
                    .last_mut()
                    .ok_or(MimeError::UnexpectedContinuation)?;

                if !last_header.value.is_empty() {
                    last_header.value.push(b' ');
                }
                last_header.value.extend_from_slice(line.trim_ascii());
            } else {
                headers.push(Header::new(line)?);
            }

            // got a header line:
            rest = after;
        }
    }

    pub fn get(&self, name: &str) -> Option<&Header> {
        self.entries
            .iter()
            .find(|h| h.name.eq_ignore_ascii_case(name))
    }
    pub fn push(&mut self, header: Header) {
        self.entries.push(header);
    }

    /// Parses the Content-Type header into its MIME type and optional boundary parameter.
    /// ContentType Default to TextPlain
    fn parse_content_type(&self) -> (ContentType, Option<String>) {
        let Some(cth) = self.get(CONTENT_TYPE_HEADER) else {
            return (DEFAULT_CONTENT_TYPE, None);
        };
        let idx = cth
            .value
            .iter()
            .position(|&c| c == b';')
            .unwrap_or(cth.value.len());
        let content_type_bytes = cth.value[..idx].trim_ascii().to_vec();
        let rest = cth.value.get(idx + 1..).unwrap_or(&[]);
        let boundary = rest.split(|&c| c == b';').find_map(|param| {
            let eq = param.iter().position(|&c| c == b'=')?;
            let key = param[..eq].trim_ascii();
            if !key.eq_ignore_ascii_case(b"boundary") {
                return None;
            }
            let val = param[eq + 1..].trim_ascii();
            let val = val
                .strip_prefix(b"\"")
                .and_then(|v| v.strip_suffix(b"\""))
                .unwrap_or(val);
            Some(val.to_vec())
        });

        let content_type = ContentType::from(
            String::from_utf8_lossy(&content_type_bytes)
                .to_ascii_lowercase()
                .as_str(),
        );
        let boundary = boundary.map(|b| String::from_utf8_lossy(&b).to_string());
        (content_type, boundary)
    }

    fn parse_transfer_encoding(&self) -> ContentTransferEncoding {
        self.get(CONTENT_TRANSFER_ENCODING_HEADER)
            .map(|h| ContentTransferEncoding::from(h.get().to_ascii_lowercase().as_str()))
            .unwrap_or(DEFAULT_TRANSFER_ENCODING)
    }
}

pub enum Body {
    // Single part's content, not yet decoded
    Leaf(Vec<u8>),
    // Parts in order
    Multipart(Vec<MimeEntity>),
    // RFC822 FIXME do I need this?
    // Message(Box<MimeEntity>),
}

struct MimeEntity {
    headers: Headers,
    content_type: ContentType,
    transfer_encoding: ContentTransferEncoding,
    body: Body,
}

impl MimeEntity {
    pub fn new(content: &[u8]) -> Result<Self, MimeError> {
        let (header_bytes, body_bytes) = split_headers_body(content)?;
        let headers = Headers::new(header_bytes)?;
        let (content_type, boundary) = headers.parse_content_type();
        let transfer_encoding = headers.parse_transfer_encoding();

        let body = make_body(&content_type, body_bytes, boundary.as_deref())?;
        Ok(Self {
            headers,
            content_type,
            transfer_encoding,
            body,
        })
    }
}

/// Complete MimeMessage, its parts are nested in Body as MimeEntities
pub struct MimeMessage {
    root: MimeEntity,
}

impl MimeMessage {
    pub fn new(bytes: &[u8]) -> Result<Self, MimeError> {
        let root = MimeEntity::new(bytes)?;
        Ok(Self { root })
    }
    pub fn subject(&self) -> Option<String> {
        self.root.headers.get("subject").map(|h| h.get())
    }

    // pub fn preferred_part(&self) -> Option<MimeEntity> {
    //     // let found = None;
    //     // self.root.content_type
    //     todo!()
    // }
}

#[derive(Debug)]
pub enum MimeError {
    SyntaxError(String),
    UnexpectedContinuation,
    ColonExtractorError,
    InvalidMime,
    QuotedPrintableError,
    ContentTypeNotImplemented(String),
    MissingBoundary,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn get_mime_data(file_name: &str) -> Vec<u8> {
        fs::read(format!("test/fixtures/{}", file_name)).unwrap()
    }
    #[test]
    fn parse_simple_headers() {
        let mime = get_mime_data("simple-email.txt");

        let parsed_headers = Headers::new(mime.as_slice()).unwrap();

        let header_names: Vec<String> = parsed_headers
            .entries
            .iter()
            .map(|h| h.name.clone())
            .collect();

        let header_values: Vec<String> = parsed_headers
            .entries
            .iter()
            .map(|h| h.get().trim().to_string())
            .collect();

        let expected_headers = vec![
            "Thu, 13 Nov 2025 00:06:18 +0100",
            "\"Simple Sender\" <test@simple-server.de>",
            "<test@test.de>",
            "<message-id@simple-server.de>",
            "1.0",
            "Test Email",
            "text/html; charset=utf-8",
            "8bit",
        ];
        assert_eq!(
            header_names,
            vec![
                "Date",
                "From",
                "To",
                "Message-Id",
                "MIME-Version",
                "Subject",
                "Content-Type",
                "Content-Transfer-Encoding"
            ]
        );
        assert_eq!(header_values, expected_headers);
    }

    #[test]
    fn parse_multipart_alternative_with_related() {
        let mime = get_mime_data("vaultwarden_mime.txt");

        let msg = MimeMessage::new(mime.as_slice()).unwrap();

        assert_eq!(msg.subject().as_deref(), Some("Vaultwarden SMTP Test"));
        assert_eq!(msg.root.content_type, ContentType::MultipartAlternative);

        let Body::Multipart(parts) = &msg.root.body else {
            panic!("expected multipart body");
        };
        assert_eq!(parts.len(), 2);

        let expected = [ContentType::TextPlain, ContentType::MultipartRelated];
        assert_eq!(
            parts[0].transfer_encoding,
            ContentTransferEncoding::QuotedPrintable
        );

        let Body::Leaf(_) = parts[0].body else {
            panic!("expected leaf body for text/plain");
        };

        // multipart/related does not specify a content-transfer-encoding as since
        // its body is multipart and each of them has its own encoding.
        // the defatul is 7bit
        assert_eq!(
            parts[1].transfer_encoding,
            ContentTransferEncoding::SevenBit
        );

        let Body::Multipart(_) = parts[1].body else {
            panic!("expected nested multipart body");
        };

        let related_part = &parts[1];

        let Body::Multipart(related_parts) = &related_part.body else {
            panic!("expected nested multipart body");
        };

        assert_eq!(related_parts.len(), 3);
        assert_eq!(related_parts[0].content_type, ContentType::TextHtml);
        assert_eq!(related_parts[1].content_type, ContentType::ImagePNG);
        assert_eq!(related_parts[2].content_type, ContentType::ImagePNG);
    }
}
