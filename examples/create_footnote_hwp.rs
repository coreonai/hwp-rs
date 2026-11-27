//! Create a minimal HWP5 file with footnotes for testing.
//! Usage: cargo run --example create_footnote_hwp

use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::io::Write;

fn main() {
    let out_path = "tests/fixtures/footnote_test.hwp";

    // Build section records
    let section_data = build_section();

    // Build docinfo records
    let docinfo_data = build_docinfo();

    // Compress both
    let section_compressed = deflate_compress(&section_data);
    let docinfo_compressed = deflate_compress(&docinfo_data);

    // Build FileHeader
    let file_header = build_file_header();

    // Create CFB file
    let file = std::fs::File::create(out_path).expect("Failed to create file");
    let mut cfb = cfb::CompoundFile::create(file).expect("Failed to create CFB");

    cfb.create_stream("/FileHeader")
        .expect("create FileHeader")
        .write_all(&file_header)
        .expect("write FileHeader");

    cfb.create_stream("/DocInfo")
        .expect("create DocInfo")
        .write_all(&docinfo_compressed)
        .expect("write DocInfo");

    cfb.create_storage("/BodyText").ok();
    cfb.create_stream("/BodyText/Section0")
        .expect("create Section0")
        .write_all(&section_compressed)
        .expect("write Section0");

    cfb.flush().expect("flush");
    drop(cfb);

    eprintln!(
        "Created {} (section: {} -> {} bytes, docinfo: {} -> {} bytes)",
        out_path,
        section_data.len(),
        section_compressed.len(),
        docinfo_data.len(),
        docinfo_compressed.len(),
    );
}

fn deflate_compress(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).expect("compress");
    encoder.finish().expect("finish compress")
}

// ── Record builder ──

fn make_record(tag: u16, level: u16, payload: &[u8]) -> Vec<u8> {
    let size = payload.len() as u32;
    let mut out = Vec::new();
    if size >= 0xFFF {
        let header: u32 = (0xFFF << 20) | ((level as u32 & 0x3FF) << 10) | (tag as u32 & 0x3FF);
        out.extend_from_slice(&header.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
    } else {
        let header: u32 =
            ((size & 0xFFF) << 20) | ((level as u32 & 0x3FF) << 10) | (tag as u32 & 0x3FF);
        out.extend_from_slice(&header.to_le_bytes());
    }
    out.extend_from_slice(payload);
    out
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect()
}

fn para_text_simple(text: &str) -> Vec<u8> {
    let mut data = utf16le(text);
    data.extend_from_slice(&0x000Du16.to_le_bytes()); // paragraph end
    data
}

fn para_text_with_section(text: &str) -> Vec<u8> {
    let mut data = Vec::new();
    // 0x0002 = section/column def (8-position extended)
    data.extend_from_slice(&0x0002u16.to_le_bytes());
    for _ in 0..7 {
        data.extend_from_slice(&0u16.to_le_bytes());
    }
    data.extend_from_slice(&utf16le(text));
    data.extend_from_slice(&0x000Du16.to_le_bytes());
    data
}

fn para_text_with_footnote(before: &str, after: &str) -> Vec<u8> {
    let mut data = utf16le(before);
    // 0x0010 = footnote/endnote anchor (8-position extended)
    data.extend_from_slice(&0x0010u16.to_le_bytes());
    for _ in 0..7 {
        data.extend_from_slice(&0u16.to_le_bytes());
    }
    data.extend_from_slice(&utf16le(after));
    data.extend_from_slice(&0x000Du16.to_le_bytes());
    data
}

fn para_header() -> Vec<u8> {
    let mut data = vec![0u8; 24];
    // text_len, ctrl_mask, para_shape_id=0, style_id=0, etc.
    // char_shape_count=1 at offset 13
    data[13] = 1; // u16 LE = 0x0001
    data
}

fn para_char_shape() -> Vec<u8> {
    // (start_pos=0, char_shape_id=0)
    vec![0u8; 8]
}

fn line_seg() -> Vec<u8> {
    vec![0u8; 36]
}

fn ctrl_header_secd() -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0x73656364u32.to_le_bytes()); // secd
    data.extend_from_slice(&vec![0u8; 43]);
    data
}

fn ctrl_header_footnote(is_endnote: bool) -> Vec<u8> {
    let mut data = Vec::new();
    if is_endnote {
        data.extend_from_slice(&0x656E2020u32.to_le_bytes()); // en
    } else {
        data.extend_from_slice(&0x666E2020u32.to_le_bytes()); // fn
    }
    data.extend_from_slice(&[0u8; 8]); // properties + padding
    data
}

fn page_def_a4() -> Vec<u8> {
    let mut data = Vec::new();
    for &v in &[59528u32, 84188, 8504, 8504, 5668, 4252, 4252, 4252, 0, 0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data
}

fn footnote_shape() -> Vec<u8> {
    vec![0u8; 28]
}

fn list_header(n_paras: u16) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&n_paras.to_le_bytes());
    data.extend_from_slice(&[0u8; 16]); // flags + padding
    data
}

const TAG_PARA_HEADER: u16 = 66;
const TAG_PARA_TEXT: u16 = 67;
const TAG_PARA_CHAR_SHAPE: u16 = 68;
const TAG_PARA_LINE_SEG: u16 = 69;
const TAG_CTRL_HEADER: u16 = 71;
const TAG_LIST_HEADER: u16 = 72;
const TAG_PAGE_DEF: u16 = 73;
const TAG_FOOTNOTE_SHAPE: u16 = 74;

fn build_section() -> Vec<u8> {
    let mut records = Vec::new();

    // ── Paragraph 0: Section def + title ──
    let text0 = para_text_with_section("각주 테스트 문서");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text0));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));
    records.extend(make_record(TAG_CTRL_HEADER, 1, &ctrl_header_secd()));
    records.extend(make_record(TAG_PAGE_DEF, 2, &page_def_a4()));
    records.extend(make_record(TAG_FOOTNOTE_SHAPE, 2, &footnote_shape()));
    records.extend(make_record(TAG_FOOTNOTE_SHAPE, 2, &footnote_shape()));

    // ── Paragraph 1: Simple text ──
    let text1 = para_text_simple("한글 워드프로세서는 한국에서 가장 많이 사용됩니다.");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text1));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));

    // ── Paragraph 2: Text with footnote 1 ──
    let text2 = para_text_with_footnote("HWP 파일 형식", "은 한국 정부 공문서의 표준입니다.");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text2));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));
    // Footnote 1
    records.extend(make_record(TAG_CTRL_HEADER, 1, &ctrl_header_footnote(false)));
    records.extend(make_record(TAG_LIST_HEADER, 2, &list_header(1)));
    let fn1 = para_text_simple("HWP는 Hangul Word Processor의 약자입니다.");
    records.extend(make_record(TAG_PARA_HEADER, 2, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 3, &fn1));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 3, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 3, &line_seg()));

    // ── Paragraph 3: Text with footnote 2 ──
    let text3 = para_text_with_footnote("HWPX 형식", "은 XML 기반 개방형 포맷입니다.");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text3));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));
    // Footnote 2
    records.extend(make_record(TAG_CTRL_HEADER, 1, &ctrl_header_footnote(false)));
    records.extend(make_record(TAG_LIST_HEADER, 2, &list_header(1)));
    let fn2 = para_text_simple("HWPX는 KS X 6101 표준의 XML 기반 문서 포맷입니다.");
    records.extend(make_record(TAG_PARA_HEADER, 2, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 3, &fn2));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 3, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 3, &line_seg()));

    // ── Paragraph 4: Text with endnote ──
    let text4 = para_text_with_footnote("참고 문헌", "을 확인하세요.");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text4));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));
    // Endnote
    records.extend(make_record(TAG_CTRL_HEADER, 1, &ctrl_header_footnote(true)));
    records.extend(make_record(TAG_LIST_HEADER, 2, &list_header(1)));
    let en1 = para_text_simple("한컴오피스 공식 문서를 참조하세요.");
    records.extend(make_record(TAG_PARA_HEADER, 2, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 3, &en1));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 3, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 3, &line_seg()));

    // ── Paragraph 5: Closing ──
    let text5 = para_text_simple("문서의 마지막 문단입니다.");
    records.extend(make_record(TAG_PARA_HEADER, 0, &para_header()));
    records.extend(make_record(TAG_PARA_TEXT, 1, &text5));
    records.extend(make_record(TAG_PARA_CHAR_SHAPE, 1, &para_char_shape()));
    records.extend(make_record(TAG_PARA_LINE_SEG, 1, &line_seg()));

    records
}

fn build_docinfo() -> Vec<u8> {
    let mut records = Vec::new();

    // TAG_DOCUMENT_PROPERTIES (16)
    let mut doc_props = vec![0u8; 26];
    doc_props[0] = 1; // section count
    records.extend(make_record(16, 0, &doc_props));

    // TAG_ID_MAPPINGS (17): 32 u32 counts
    let mut id_map = vec![0u8; 128];
    // char shapes at index 2, para shapes at 3, styles at 8
    id_map[8..12].copy_from_slice(&1u32.to_le_bytes()); // 1 char shape
    id_map[12..16].copy_from_slice(&1u32.to_le_bytes()); // 1 para shape
    id_map[32..36].copy_from_slice(&1u32.to_le_bytes()); // 1 style
    records.extend(make_record(17, 0, &id_map));

    // TAG_FACE_NAME (19): Korean font
    let mut face = vec![0x01u8]; // properties
    let name = utf16le("함초롬돋움");
    face.extend_from_slice(&(name.len() as u16 / 2).to_le_bytes());
    face.extend_from_slice(&name);
    face.extend_from_slice(&[0u8; 20]);
    records.extend(make_record(19, 0, &face));

    // TAG_CHAR_SHAPE (21): 72 bytes
    let mut cs = vec![0u8; 72];
    // height at offset 54
    cs[54..58].copy_from_slice(&1000u32.to_le_bytes());
    records.extend(make_record(21, 0, &cs));

    // TAG_PARA_SHAPE (23): 48 bytes
    records.extend(make_record(23, 0, &vec![0u8; 48]));

    // TAG_STYLE (29)
    let mut style = Vec::new();
    let kname = utf16le("본문");
    style.extend_from_slice(&(kname.len() as u16 / 2).to_le_bytes());
    style.extend_from_slice(&kname);
    let ename = utf16le("Body");
    style.extend_from_slice(&(ename.len() as u16 / 2).to_le_bytes());
    style.extend_from_slice(&ename);
    style.push(0); // type
    style.extend_from_slice(&0u16.to_le_bytes()); // next style
    style.extend_from_slice(&0u16.to_le_bytes()); // language
    style.extend_from_slice(&0u16.to_le_bytes()); // para shape id
    style.extend_from_slice(&0u16.to_le_bytes()); // char shape id
    records.extend(make_record(29, 0, &style));

    records
}

fn build_file_header() -> Vec<u8> {
    let mut header = vec![0u8; 256];
    // HWP signature
    let sig = b"HWP Document File\x00";
    header[..sig.len()].copy_from_slice(sig);
    // Version at offset 32
    header[32..36].copy_from_slice(&0x05010203u32.to_le_bytes());
    // Flags at offset 36: bit 0 = compressed
    header[36..40].copy_from_slice(&1u32.to_le_bytes());
    header
}
