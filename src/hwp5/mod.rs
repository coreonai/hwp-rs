pub mod bodytext;
pub mod decompress;
pub mod docinfo;
pub mod header;
pub mod record;

use std::io::{Read, Seek};

use crate::error::{HwpError, Result};
use crate::model::{BinEntry, HwpDocument};

/// Parse an HWP5 binary document from a reader.
pub fn parse_hwp5<R: Read + Seek>(reader: R) -> Result<HwpDocument> {
    let mut cfb = cfb::CompoundFile::open(reader)?;

    let file_header = header::parse_file_header(&mut cfb)?;

    if file_header.encrypted {
        return Err(HwpError::Encrypted);
    }

    let doc_info = docinfo::parse_doc_info(&mut cfb, &file_header)?;
    let sections = bodytext::parse_body_text(&mut cfb, &file_header, &doc_info)?;
    let bin_data = parse_bin_data(&mut cfb)?;

    Ok(HwpDocument {
        header: file_header,
        doc_info,
        sections,
        bin_data,
    })
}

/// Extract binary data entries (images, etc.) from BinData storage.
fn parse_bin_data<R: Read + Seek>(cfb: &mut cfb::CompoundFile<R>) -> Result<Vec<BinEntry>> {
    let mut entries = Vec::new();

    // List entries under /BinData/
    let bin_entries: Vec<String> = {
        let mut names = Vec::new();
        for entry in cfb.walk() {
            let path = entry.path().to_string_lossy().to_string();
            if path.starts_with("/BinData/") && entry.is_stream() {
                names.push(path);
            }
        }
        names
    };

    for name in &bin_entries {
        if let Ok(mut stream) = cfb.open_stream(name) {
            let mut data = Vec::new();
            if stream.read_to_end(&mut data).is_ok() {
                let entry_name = name
                    .strip_prefix("/BinData/")
                    .unwrap_or(name)
                    .to_string();
                entries.push(BinEntry {
                    name: entry_name,
                    data,
                });
            }
        }
    }

    Ok(entries)
}
