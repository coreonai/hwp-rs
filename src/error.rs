use thiserror::Error;

#[derive(Debug, Error)]
pub enum HwpError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("XML attribute error: {0}")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),

    #[error("Invalid signature: expected 'HWP Document File', got {0:?}")]
    InvalidSignature(String),

    #[error("Unsupported version: {0}.{1}.{2}.{3}")]
    UnsupportedVersion(u8, u8, u8, u8),

    #[error("Encrypted documents are not supported")]
    Encrypted,

    #[error("Decompression failed: {0}")]
    Decompress(String),

    #[error("CRC mismatch: expected {expected:#010x}, got {actual:#010x}")]
    CrcMismatch { expected: u32, actual: u32 },

    #[error("Invalid record: {0}")]
    InvalidRecord(String),

    #[error("Unexpected end of data")]
    UnexpectedEof,

    #[error("Invalid HWPX: {0}")]
    InvalidHwpx(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("Integer parse error: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    #[error("Float parse error: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
}

pub type Result<T> = std::result::Result<T, HwpError>;
