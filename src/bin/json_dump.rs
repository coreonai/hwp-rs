//! JSON dump — parse an HWP/HWPX file and output structured JSON to stdout.
//!
//! Usage: `cargo run --example json_dump -- <file.hwp|file.hwpx>`
//!
//! Intended as the subprocess backend for the REA `inspect_hwp_document` tool.

use std::fs;
use std::io::BufReader;
use std::path::Path;
use std::process;

use sha2::{Digest, Sha256};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!(
            "{}",
            serde_json::json!({"error": "Usage: json_dump <file.hwp|file.hwpx>"})
        );
        process::exit(1);
    }
    let path = &args[1];
    match run(path) {
        Ok(json) => {
            println!("{json}");
        }
        Err(e) => {
            eprintln!("{}", serde_json::json!({"error": e.to_string()}));
            process::exit(1);
        }
    }
}

fn run(path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let path = Path::new(path);

    // Read file bytes for hashing and format detection.
    let file_bytes = fs::read(path)?;
    let sha256 = format!("{:x}", Sha256::digest(&file_bytes));
    let byte_count = file_bytes.len();

    // Detect format from magic bytes.
    let format = if file_bytes.starts_with(b"PK\x03\x04") {
        "hwpx"
    } else {
        "hwp5"
    };

    // Parse the document.
    let cursor = std::io::Cursor::new(&file_bytes);
    let reader = BufReader::new(cursor);
    let doc = if format == "hwpx" {
        hwp_rs::parse_hwpx(reader)?
    } else {
        hwp_rs::parse_hwp5(reader)?
    };

    // Build the output JSON.
    let version = format!(
        "{}.{}.{}.{}",
        doc.header.version.0,
        doc.header.version.1,
        doc.header.version.2,
        doc.header.version.3
    );

    let bin_data: Vec<serde_json::Value> = doc
        .bin_data
        .iter()
        .map(|entry| {
            serde_json::json!({
                "name": entry.name,
                "bytes": entry.data.len(),
            })
        })
        .collect();

    let mut limitations = Vec::new();
    if doc.header.encrypted {
        limitations.push("Document is encrypted; content may be partially or fully unavailable.");
    }
    limitations.push(
        "Equations are represented as script text only; OLE rendering is not recovered.",
    );
    limitations.push(
        "Drawing shapes are represented structurally; visual rendering is not performed.",
    );
    limitations.push("Binary data entries report names and sizes; raw bytes are not included in JSON output.");

    let output = serde_json::json!({
        "artifact": {
            "path": path.to_string_lossy(),
            "sha256": sha256,
            "bytes": byte_count,
        },
        "format": format,
        "version": version,
        "compressed": doc.header.compressed,
        "encrypted": doc.header.encrypted,
        "sections": doc.sections,
        "doc_info": doc.doc_info,
        "bin_data": bin_data,
        "limitations": limitations,
    });

    Ok(serde_json::to_string(&output)?)
}
