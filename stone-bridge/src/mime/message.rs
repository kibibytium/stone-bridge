use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentType {
    TextPlain,
    TextHtml,
    MultipartAlternative,
    MultipartRelated,
    MultipartMixed,
    ApplicationPdf,
    ApplicationOctetStream,
    ImagePNG,
    ImageJPEG,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentTransferEncoding {
    SevenBit,
    EightBit,
    QuotedPrintable,
    Base64,
}

pub enum Charset {
    UTF8,
    ASCII,
}

pub enum ContentDisposition {
    Inline,
    Attachment,
}

impl From<&str> for ContentType {
    fn from(value: &str) -> Self {
        match value {
            "text/plain" => ContentType::TextPlain,
            "text/html" => ContentType::TextHtml,
            "multipart/alternative" => ContentType::MultipartAlternative,
            "multipart/related" => ContentType::MultipartRelated,
            "multipart/mixed" => ContentType::MultipartMixed,
            "application/pdf" => ContentType::ApplicationPdf,
            "application/octet-stream" => ContentType::ApplicationOctetStream,
            "image/png" => ContentType::ImagePNG,
            "image/jpeg" | "image/jpg" => ContentType::ImageJPEG,
            _ => ContentType::TextPlain,
        }
    }
}

impl From<&str> for ContentTransferEncoding {
    fn from(value: &str) -> Self {
        match value {
            "7bit" => ContentTransferEncoding::SevenBit,
            "8bit" => ContentTransferEncoding::EightBit,
            "quoted-printable" => ContentTransferEncoding::QuotedPrintable,
            "base64" => ContentTransferEncoding::Base64,
            _ => ContentTransferEncoding::SevenBit,
        }
    }
}

impl Display for ContentType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::TextPlain => {
                write!(f, "text/plain")
            }
            ContentType::TextHtml => {
                write!(f, "text/html")
            }
            ContentType::MultipartAlternative => {
                write!(f, "multipart/alternative")
            }
            ContentType::MultipartRelated => {
                write!(f, "multipart/related")
            }
            ContentType::MultipartMixed => {
                write!(f, "multipart/mixed")
            }
            ContentType::ApplicationPdf => {
                write!(f, "application/pdf")
            }
            ContentType::ApplicationOctetStream => {
                write!(f, "application/octet-stream")
            }
            ContentType::ImagePNG => {
                write!(f, "image/png")
            }
            ContentType::ImageJPEG => {
                write!(f, "image/jpeg")
            }
        }
    }
}

impl From<&str> for ContentDisposition {
    /// Matches on the disposition type only; parameters such as
    /// `filename="x.pdf"` are ignored.
    fn from(value: &str) -> Self {
        let disposition = value.split(';').next().unwrap_or("").trim();
        match disposition.to_ascii_lowercase().as_str() {
            "inline" => ContentDisposition::Inline,
            "attachment" => ContentDisposition::Attachment,
            _ => ContentDisposition::Inline,
        }
    }
}
