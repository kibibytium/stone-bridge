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
