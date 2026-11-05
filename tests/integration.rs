//! Integration tests using real HWP5 and HWPX fixture files.

use std::process::Command;

// ──────────────────────────────────────────────────────────────────────
// HWP5 binary format — tests/fixtures/example.hwp
// ──────────────────────────────────────────────────────────────────────

#[test]
fn parse_hwp5_fixture() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    assert_eq!(doc.header.version, (5, 0, 5, 0));
    assert!(doc.header.compressed);
    assert!(!doc.header.encrypted);
}

#[test]
fn hwp5_has_one_section_with_paragraphs() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    assert_eq!(doc.sections.len(), 1);
    assert_eq!(doc.sections[0].paragraphs.len(), 20);
}

#[test]
fn hwp5_page_def_is_a4_portrait() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    let page = &doc.sections[0].sec_def.page_def;
    // A4 dimensions in HWP units (1/7200 inch)
    assert!(page.width > 59000 && page.width < 60000, "width {}", page.width);
    assert!(
        page.height > 84000 && page.height < 85000,
        "height {}",
        page.height
    );
    assert!(!page.landscape);
}

#[test]
fn hwp5_extracts_english_text() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    let texts: Vec<&str> = doc.sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("HWP test file")),
        "expected 'HWP test file' in paragraphs"
    );
    assert!(
        texts.iter().any(|t| t.contains("John Doe")),
        "expected 'John Doe' in paragraphs"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("Document file type: HWP")),
        "expected 'Document file type: HWP' in paragraphs"
    );
}

#[test]
fn hwp5_has_embedded_binary_data() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    assert_eq!(doc.bin_data.len(), 3);
    let names: Vec<&str> = doc.bin_data.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"BIN0001.jpg"));
    assert!(names.contains(&"BIN0002.png"));
    assert!(names.contains(&"BIN0003.svg"));
    // BIN0001.jpg is the largest
    let jpg = doc.bin_data.iter().find(|b| b.name == "BIN0001.jpg").unwrap();
    assert!(jpg.data.len() > 600_000, "jpg bytes: {}", jpg.data.len());
}

#[test]
fn hwp5_doc_info_has_shapes_and_styles() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    assert_eq!(doc.doc_info.char_shapes.len(), 17);
    assert_eq!(doc.doc_info.para_shapes.len(), 12);
    assert_eq!(doc.doc_info.styles.len(), 17);
    // 7 font script groups
    assert_eq!(doc.doc_info.fonts.len(), 7);
    // Each group has at least one font
    for (i, group) in doc.doc_info.fonts.iter().enumerate() {
        assert!(!group.is_empty(), "font group {i} is empty");
    }
}

#[test]
fn hwp5_has_known_fonts() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    let all_fonts: Vec<&str> = doc
        .doc_info
        .fonts
        .iter()
        .flat_map(|g| g.iter().map(|f| f.name.as_str()))
        .collect();
    assert!(
        all_fonts.contains(&"Arial"),
        "expected Arial in fonts: {all_fonts:?}"
    );
    assert!(
        all_fonts.contains(&"Times New Roman"),
        "expected Times New Roman in fonts: {all_fonts:?}"
    );
}

#[test]
fn hwp5_char_shape_properties() {
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    // The document has bold and italic shapes
    assert!(
        doc.doc_info.char_shapes.iter().any(|cs| cs.bold),
        "expected at least one bold char shape"
    );
    assert!(
        doc.doc_info.char_shapes.iter().any(|cs| cs.italic),
        "expected at least one italic char shape"
    );
    // Check underline types exist
    assert!(
        doc.doc_info.char_shapes.iter().any(|cs| cs.underline
            != hwp_rs::model::charshape::UnderlineType::None),
        "expected at least one underlined char shape"
    );
}

// ──────────────────────────────────────────────────────────────────────
// HWPX XML format — tests/fixtures/sample.hwpx
// ──────────────────────────────────────────────────────────────────────

#[test]
fn parse_hwpx_fixture() {
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    assert_eq!(doc.header.version, (5, 0, 0, 0));
    assert!(!doc.header.compressed);
    assert!(!doc.header.encrypted);
}

#[test]
fn hwpx_has_one_section_with_paragraphs() {
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    assert_eq!(doc.sections.len(), 1);
    assert_eq!(doc.sections[0].paragraphs.len(), 89);
}

#[test]
fn hwpx_extracts_korean_text() {
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    let texts: Vec<&str> = doc.sections[0]
        .paragraphs
        .iter()
        .map(|p| p.text.as_str())
        .collect();
    assert!(
        texts
            .iter()
            .any(|t| t.contains("유니버설디자인")),
        "expected '유니버설디자인' in paragraphs"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("공모전")),
        "expected '공모전' in paragraphs"
    );
}

#[test]
fn hwpx_page_def_is_a4() {
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    let page = &doc.sections[0].sec_def.page_def;
    assert!(page.width > 59000 && page.width < 60000, "width {}", page.width);
    assert!(
        page.height > 84000 && page.height < 85000,
        "height {}",
        page.height
    );
}

#[test]
fn hwpx_has_font_groups() {
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    assert_eq!(doc.doc_info.fonts.len(), 7);
}

// ──────────────────────────────────────────────────────────────────────
// Format auto-detection
// ──────────────────────────────────────────────────────────────────────

#[test]
fn auto_detect_hwp5_format() {
    // parse() should auto-detect HWP5 from CFB magic bytes
    let doc = hwp_rs::parse("tests/fixtures/example.hwp").unwrap();
    assert!(doc.header.compressed); // HWP5-specific flag
}

#[test]
fn auto_detect_hwpx_format() {
    // parse() should auto-detect HWPX from ZIP magic bytes
    let doc = hwp_rs::parse("tests/fixtures/sample.hwpx").unwrap();
    assert!(!doc.header.compressed); // HWPX doesn't use stream compression
}

#[test]
fn nonexistent_file_returns_error() {
    let result = hwp_rs::parse("tests/fixtures/nonexistent.hwp");
    assert!(result.is_err());
}

// ──────────────────────────────────────────────────────────────────────
// json_dump binary output
// ──────────────────────────────────────────────────────────────────────

#[test]
fn json_dump_hwp5_produces_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .arg("tests/fixtures/example.hwp")
        .output()
        .expect("failed to run json_dump");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("invalid JSON output");
    assert_eq!(json["format"], "hwp5");
    assert_eq!(json["version"], "5.0.5.0");
    assert_eq!(json["compressed"], true);
    assert_eq!(json["encrypted"], false);
    assert!(json["artifact"]["sha256"].as_str().unwrap().len() == 64);
    assert!(json["artifact"]["bytes"].as_u64().unwrap() > 0);
    assert!(json["sections"].as_array().unwrap().len() == 1);
    assert!(json["bin_data"].as_array().unwrap().len() == 3);
    assert!(json["limitations"].as_array().unwrap().len() >= 1);
}

#[test]
fn json_dump_hwpx_produces_valid_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .arg("tests/fixtures/sample.hwpx")
        .output()
        .expect("failed to run json_dump");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("invalid JSON output");
    assert_eq!(json["format"], "hwpx");
    assert_eq!(json["version"], "5.0.0.0");
    assert_eq!(json["compressed"], false);
    assert!(json["artifact"]["sha256"].as_str().unwrap().len() == 64);
    assert_eq!(json["sections"].as_array().unwrap().len(), 1);
    assert!(json["doc_info"]["fonts"].as_array().unwrap().len() == 7);
}

#[test]
fn json_dump_hwp5_text_is_present() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .arg("tests/fixtures/example.hwp")
        .output()
        .expect("failed to run json_dump");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paragraphs = json["sections"][0]["paragraphs"].as_array().unwrap();
    let all_text: String = paragraphs
        .iter()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(all_text.contains("HWP test file"), "missing 'HWP test file'");
    assert!(all_text.contains("John Doe"), "missing 'John Doe'");
}

#[test]
fn json_dump_hwpx_korean_text_is_present() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .arg("tests/fixtures/sample.hwpx")
        .output()
        .expect("failed to run json_dump");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paragraphs = json["sections"][0]["paragraphs"].as_array().unwrap();
    let all_text: String = paragraphs
        .iter()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        all_text.contains("유니버설디자인"),
        "missing Korean text"
    );
}

#[test]
fn json_dump_nonexistent_file_exits_nonzero() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .arg("tests/fixtures/nonexistent.hwp")
        .output()
        .expect("failed to run json_dump");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error"), "stderr: {stderr}");
}

#[test]
fn json_dump_no_args_exits_nonzero() {
    let output = Command::new(env!("CARGO_BIN_EXE_json_dump"))
        .output()
        .expect("failed to run json_dump");
    assert!(!output.status.success());
}
