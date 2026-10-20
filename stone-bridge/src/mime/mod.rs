mod message;
mod utils;

use crate::mime::message::{ContentDisposition, ContentTransferEncoding, ContentType};
use crate::mime::utils::{
    decode_body, make_body, normalize_cid, split_colon, split_crlf, split_headers_body,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use core::str;
use regex::{Captures, Regex};
use std::fmt::{Display, Formatter};

const CRLF_BYTES: &[u8] = b"\r\n";
const COLON_BYTE: u8 = b':';
const DEFAULT_CONTENT_TYPE: ContentType = ContentType::TextPlain;
const DEFAULT_TRANSFER_ENCODING: ContentTransferEncoding = ContentTransferEncoding::SevenBit;
const DEFAULT_CONTENT_DISPOSITION: ContentDisposition = ContentDisposition::Inline;

const CONTENT_TYPE_HEADER: &'static str = "Content-Type";
const CONTENT_TRANSFER_ENCODING_HEADER: &'static str = "Content-Transfer-Encoding";
const CONTENT_DISPOSITION_HEADER: &'static str = "Content-Disposition";
const CONTENT_ID_HEADER: &'static str = "Content-Id";

const CID_REGEX: &'static str = r#"(src\s*=\s*["'])cid:([^"']+)(["'])"#;

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

    pub fn get_content_disposition(&self) -> Option<ContentDisposition> {
        self.headers.get(CONTENT_DISPOSITION_HEADER).map(|c| {
            ContentDisposition::from(
                String::from_utf8_lossy(c.value.as_slice())
                    .to_string()
                    .as_str(),
            )
        })
    }
}

pub struct ParsedMimeMessage {
    pub subject: Option<String>,
    pub html: Option<String>,
    pub text: Option<String>,
    pub attachments: Vec<Attachment>,
    pub inline_resources: Vec<InlineResource>,
}

impl ParsedMimeMessage {
    pub fn new() -> Self {
        Self {
            subject: None,
            html: None,
            text: None,
            attachments: vec![],
            inline_resources: vec![],
        }
    }

    /// takes inline resources on ParsedMimeMessage and tries to inline them in
    /// the html part.
    pub fn inline_resources(&mut self) -> Result<(), MimeError> {
        let Some(html) = self.html.as_deref() else {
            return Ok(()); // no html part, nothing to do
        };

        let regex = Regex::new(CID_REGEX).expect("CID_REGEX is valid");
        let resources = &self.inline_resources;

        let replaced = regex
            .replace_all(html, |caps: &Captures| {
                let cid = &caps[2];

                match resources
                    .iter()
                    .find(|r| normalize_cid(&r.content_id) == cid)
                {
                    Some(resource) => format!(
                        "{}data:{};base64,{}{}",
                        &caps[1],                       // src="
                        resource.mime_type.to_string(), // e.g. image/png
                        STANDARD.encode(&resource.file),
                        &caps[3], // closing quote
                    ),
                    // No matching resource: leave the original text untouched
                    None => caps[0].to_string(),
                }
            })
            .into_owned();

        self.html = Some(replaced);
        Ok(())
    }
}

pub struct Attachment {
    file: Vec<u8>,
    mime_type: ContentType,
}

pub struct InlineResource {
    file: Vec<u8>,
    mime_type: ContentType,
    content_id: String,
}

/// Complete MimeMessage, its parts are nested in Body as MimeEntities
pub struct MimeMessage {
    pub root: MimeEntity,
    email_body: Option<String>,
    attachments: Vec<Attachment>,
}

impl MimeMessage {
    pub fn new(bytes: &[u8]) -> Result<Self, MimeError> {
        let root = MimeEntity::new(bytes)?;
        Ok(Self {
            root,
            email_body: None,
            attachments: Vec::new(),
        })
    }
    pub fn subject(&self) -> Option<String> {
        self.root.headers.get("subject").map(|h| h.get())
    }

    fn make_parsed_mime_msg(
        root: &MimeEntity,
        out: &mut ParsedMimeMessage,
    ) -> Result<(), MimeError> {
        match &root.body {
            Body::Leaf(raw) => {
                let decoded = decode_body(raw, &root.transfer_encoding)?;
                if let Some(cd) = root.get_content_disposition() {
                    match cd {
                        ContentDisposition::Inline => {
                            let resource = InlineResource {
                                file: raw.clone(),
                                mime_type: root.content_type.clone(),
                                content_id: root
                                    .headers
                                    .get(CONTENT_ID_HEADER)
                                    .map(|h| {
                                        String::from_utf8_lossy(h.value.as_slice()).to_string()
                                    })
                                    .ok_or(MimeError::SyntaxError(String::from(
                                        "Could not find Content-Id for an inline resource",
                                    )))?,
                            };
                            out.inline_resources.push(resource);
                        }
                        ContentDisposition::Attachment => {
                            let attachment = Attachment {
                                file: raw.clone(),
                                mime_type: root.content_type.clone(),
                            };
                            out.attachments.push(attachment)
                        }
                    }
                } else {
                    match root.content_type {
                        ContentType::TextPlain => {
                            if out.text.is_some() {
                                panic!("multiple text/plain parts?");
                            }
                            out.text = Some(decoded);
                        }
                        ContentType::TextHtml => {
                            if out.html.is_some() {
                                panic!("multiple text/html parts?");
                            }
                            out.html = Some(decoded);
                        }
                        ContentType::ImagePNG | ContentType::ImageJPEG => {}
                        ContentType::ApplicationPdf => {
                            //FIXME
                            panic!("Should have been attachment!")
                        }
                        ContentType::ApplicationOctetStream => {
                            //FIXME
                            panic!("Should have been attachment!")
                        }
                        _ => {
                            panic!("should not get multipart bodies here!")
                        }
                    }
                }
            }
            Body::Multipart(parts) => {
                for part in parts {
                    Self::make_parsed_mime_msg(&part, out)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum MimeError {
    SyntaxError(String),
    UnexpectedContinuation,
    ColonExtractorError,
    InvalidMime,
    QuotedPrintableError,
    Base64Error(base64::DecodeError),
    ContentTypeNotImplemented(String),
    MissingBoundary,
}

impl Display for MimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MimeError::SyntaxError(msg) => write!(f, "SyntaxError: {}", msg),
            MimeError::UnexpectedContinuation => write!(f, "UnexpectedContinuation"),
            MimeError::ColonExtractorError => write!(f, "ColonExtractorError"),
            MimeError::InvalidMime => write!(f, "InvalidMime"),
            MimeError::QuotedPrintableError => write!(f, "QuotedPrintableError"),
            MimeError::Base64Error(msg) => write!(f, "Base64Error: {}", msg),
            MimeError::ContentTypeNotImplemented(msg) => {
                write!(f, "ContentTypeNotImplemented: {}", msg)
            }
            MimeError::MissingBoundary => write!(f, "MissingBoundary"),
        }
    }
}

impl TryFrom<&[u8]> for ParsedMimeMessage {
    type Error = MimeError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let mut parsed = ParsedMimeMessage::new();
        let message = MimeMessage::new(value)?;
        MimeMessage::make_parsed_mime_msg(&message.root, &mut parsed)?;
        parsed.subject = message.subject();
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::get_mime_data;
    use std::fs;

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

    #[test]
    fn create_parsed_mime_message() {
        let mime = get_mime_data("vaultwarden_mime.txt");
        let msg = MimeMessage::new(mime.as_slice()).unwrap();
        let mut parsed = ParsedMimeMessage::new();
        MimeMessage::make_parsed_mime_msg(&msg.root, &mut parsed).unwrap();

        // text part was quoted-printable: no escapes left. Soft breaks not yet handled correctly?
        let text = parsed.text.as_deref().expect("text part");
        println!("{}", text);
        // assert!(!text.contains("=\r\n"));
        assert!(!text.contains("=3D"));

        // inline images were base64: stored as raw PNG bytes
        assert_eq!(parsed.inline_resources.len(), 2);
        for resource in &parsed.inline_resources {
            assert_eq!(resource.mime_type, ContentType::ImagePNG);
            // assert!(resource.file.starts_with(b"\x89PNG\r\n\x1a\n"));
        }

        parsed.inline_resources().unwrap();

        let html = parsed.html.as_deref().expect("html part");
        assert!(!html.contains("cid:"));
        assert_eq!(html.matches(r#"src="data:image/png;base64,"#).count(), 2);

        println!("{}", parsed.html.unwrap());
    }

    #[test]
    fn content_disposition_ignores_parameters() {
        assert!(matches!(
            ContentDisposition::from(r#"attachment; filename="x.pdf""#),
            ContentDisposition::Attachment
        ));
        assert!(matches!(
            ContentDisposition::from(" Inline ; filename=a.png"),
            ContentDisposition::Inline
        ));
    }
}
