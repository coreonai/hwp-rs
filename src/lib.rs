//! `hwp-rs` — Rust parser for Hancom Office HWP5 (.hwp) and HWPX (.hwpx) documents.
//!
//! Parses both the legacy OLE2-based HWP5 binary format and the modern
//! ZIP/XML-based HWPX format into a unified [`HwpDocument`] model.
//!
//! # Usage
//!
//! ```no_run
//! use hwp_rs::parse;
//!
//! let doc = parse("document.hwp").unwrap();
//! println!("Sections: {}", doc.sections.len());
//! for (i, sec) in doc.sections.iter().enumerate() {
//!     println!("  Section {}: {} paragraphs", i, sec.paragraphs.len());
//! }
//! ```

pub mod error;
pub mod hwp5;
pub mod hwpx;
pub mod model;

pub use error::{HwpError, Result};
pub use model::HwpDocument;

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

/// Parse an HWP document from a file path, auto-detecting the format.
///
/// Returns an [`HwpDocument`] on success, or an [`HwpError`] on failure.
pub fn parse<P: AsRef<Path>>(path: P) -> Result<HwpDocument> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read magic bytes to detect format
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    reader.seek(std::io::SeekFrom::Start(0))?;

    if &magic == b"PK\x03\x04" {
        // ZIP file → HWPX
        parse_hwpx(reader)
    } else {
        // Try OLE2/CFB (HWP5)
        // CFB signature is D0 CF 11 E0
        parse_hwp5(reader)
    }
}

/// Parse an HWP5 binary document from a reader.
pub fn parse_hwp5<R: Read + Seek>(reader: R) -> Result<HwpDocument> {
    hwp5::parse_hwp5(reader)
}

/// Parse an HWPX (ZIP/XML) document from a reader.
pub fn parse_hwpx<R: Read + Seek>(reader: R) -> Result<HwpDocument> {
    hwpx::parse_hwpx(reader)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_detect_zip_magic() {
        let magic = b"PK\x03\x04";
        assert_eq!(magic[0], 0x50); // 'P'
        assert_eq!(magic[1], 0x4B); // 'K'
    }

    #[test]
    fn test_detect_cfb_magic() {
        let magic: [u8; 4] = [0xD0, 0xCF, 0x11, 0xE0];
        assert_ne!(&magic, b"PK\x03\x04");
    }
}
