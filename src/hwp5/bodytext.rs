use std::io::{Cursor, Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::error::Result;
use crate::model::control::Control;
use crate::model::table::{Cell, Table};
use crate::model::{
    CharRun, DocInfo, FileHeader, PageDef, Paragraph, Section,
};

use super::decompress::decompress_stream;
use super::record::{parse_records, Record};

// Section tag IDs (HWPTAG_BEGIN=16, section tags start at HWPTAG_BEGIN+50=66)
const TAG_PARA_HEADER: u16 = 66;     // HWPTAG_PARA_HEADER
const TAG_PARA_TEXT: u16 = 67;       // HWPTAG_PARA_TEXT
const TAG_PARA_CHAR_SHAPE: u16 = 68; // HWPTAG_PARA_CHAR_SHAPE
#[allow(dead_code)]
const TAG_PARA_LINE_SEG: u16 = 69;   // HWPTAG_PARA_LINE_SEG
#[allow(dead_code)]
const TAG_PARA_RANGE_TAG: u16 = 70;  // HWPTAG_PARA_RANGE_TAG
const TAG_CTRL_HEADER: u16 = 71;     // HWPTAG_CTRL_HEADER
const TAG_LIST_HEADER: u16 = 72;     // HWPTAG_LIST_HEADER
const TAG_PAGE_DEF: u16 = 73;        // HWPTAG_PAGE_DEF
const TAG_TABLE: u16 = 78;           // HWPTAG_TABLE
const TAG_CELL: u16 = 79;            // HWPTAG_CELL
#[allow(dead_code)]
const TAG_SHAPE_PICTURE: u16 = 89;   // HWPTAG_SHAPE_COMPONENT_PICTURE
#[allow(dead_code)]
const TAG_EQUATION: u16 = 92;        // HWPTAG_SHAPE_COMPONENT_EQUATION

/// Parse all BodyText sections from an OLE2 compound file.
pub fn parse_body_text<R: Read + Seek>(
    cfb: &mut cfb::CompoundFile<R>,
    header: &FileHeader,
    _doc_info: &DocInfo,
) -> Result<Vec<Section>> {
    let mut sections = Vec::new();
    let mut section_idx = 0u32;

    loop {
        let stream_name = format!("/BodyText/Section{}", section_idx);
        let mut stream = match cfb.open_stream(&stream_name) {
            Ok(s) => s,
            Err(_) => break,
        };
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw)?;

        let data = if header.compressed {
            decompress_stream(&raw)?
        } else {
            raw
        };

        let records = parse_records(&data)?;
        let section = parse_section_records(&records)?;
        sections.push(section);
        section_idx += 1;
    }

    Ok(sections)
}

/// Parse records from a single section stream into a Section.
fn parse_section_records(records: &[Record]) -> Result<Section> {
    let mut section = Section::default();
    let mut paragraphs: Vec<Paragraph> = Vec::new();
    let mut current_para: Option<Paragraph> = None;

    // Table parsing state
    let mut table_stack: Vec<TableBuildState> = Vec::new();
    // Position map from most recent PARA_TEXT (u16 stream index → text char index)
    let mut current_pos_map: Option<Vec<u32>> = None;

    let mut i = 0;
    while i < records.len() {
        let record = &records[i];
        match record.header.tag {
            TAG_PAGE_DEF => {
                if let Ok(pd) = parse_page_def(&record.payload) {
                    section.sec_def.page_def = pd;
                }
            }
            TAG_PARA_HEADER => {
                // Flush previous paragraph
                if let Some(para) = current_para.take() {
                    push_paragraph(&mut paragraphs, &mut table_stack, para);
                }
                current_para = Some(parse_para_header(&record.payload).unwrap_or_default());
                current_pos_map = None;
            }
            TAG_PARA_TEXT => {
                if let Some(ref mut para) = current_para {
                    if let Ok((text, map)) = parse_para_text(&record.payload) {
                        para.text = text;
                        // Stash pos_map temporarily in char_count (will be used
                        // when PARA_CHAR_SHAPE is processed below)
                        // Store the map in a side channel
                        current_pos_map = Some(map);
                    }
                }
            }
            TAG_PARA_CHAR_SHAPE => {
                if let Some(ref mut para) = current_para {
                    if let Ok(mut runs) = parse_para_char_shape(&record.payload) {
                        // Remap start_pos from u16 stream indices to text char indices
                        if let Some(ref map) = current_pos_map {
                            for run in &mut runs {
                                let pos = run.start_pos as usize;
                                run.start_pos = if pos < map.len() {
                                    map[pos]
                                } else {
                                    // Beyond stream end — clamp to text length
                                    map.last().copied().unwrap_or(0)
                                };
                            }
                        }
                        para.char_runs = runs;
                    }
                }
            }
            TAG_CTRL_HEADER => {
                if let Some(ref mut para) = current_para {
                    if record.payload.len() >= 4 {
                        let ctrl_id = u32::from_le_bytes([
                            record.payload[0],
                            record.payload[1],
                            record.payload[2],
                            record.payload[3],
                        ]);
                        // "tbl " = table control
                        if ctrl_id == u32::from_le_bytes(*b"tbl ") {
                            // Table detected — we'll collect it from TAG_TABLE
                        } else {
                            para.controls.push(Control::Unknown {
                                ctrl_id,
                                data: record.payload.clone(),
                            });
                        }
                    }
                }
            }
            TAG_TABLE => {
                if let Ok(table) = parse_table_record(&record.payload) {
                    table_stack.push(TableBuildState {
                        table,
                        cells_collected: 0,
                    });
                }
            }
            TAG_CELL => {
                if let Some(tbs) = table_stack.last_mut() {
                    if let Ok(cell) = parse_cell_record(&record.payload) {
                        tbs.table.cells.push(cell);
                        tbs.cells_collected += 1;
                    }
                }
            }
            TAG_LIST_HEADER => {
                // List header for cell content — skip for now
            }
            _ => {
                // Other tags: line seg, range tag, shapes, etc.
            }
        }
        i += 1;
    }

    // Flush last paragraph
    if let Some(para) = current_para.take() {
        push_paragraph(&mut paragraphs, &mut table_stack, para);
    }

    // Flush remaining tables
    while let Some(tbs) = table_stack.pop() {
        // Orphan table — attach to section as a control on a synthetic paragraph
        let mut para = Paragraph::default();
        para.controls.push(Control::Table(tbs.table));
        paragraphs.push(para);
    }

    section.paragraphs = paragraphs;
    Ok(section)
}

struct TableBuildState {
    table: Table,
    cells_collected: u32,
}

fn push_paragraph(
    paragraphs: &mut Vec<Paragraph>,
    table_stack: &mut Vec<TableBuildState>,
    para: Paragraph,
) {
    if let Some(tbs) = table_stack.last_mut() {
        // If we're inside a table cell, add paragraph to the last cell
        if let Some(cell) = tbs.table.cells.last_mut() {
            cell.paragraphs.push(para);
            return;
        }
    }
    paragraphs.push(para);
}

fn parse_page_def(data: &[u8]) -> Result<PageDef> {
    if data.len() < 40 {
        return Ok(PageDef::default());
    }
    let mut c = Cursor::new(data);
    let mut pd = PageDef::default();

    pd.width = c.read_u32::<LittleEndian>()?;
    pd.height = c.read_u32::<LittleEndian>()?;
    pd.margin_left = c.read_u32::<LittleEndian>()?;
    pd.margin_right = c.read_u32::<LittleEndian>()?;
    pd.margin_top = c.read_u32::<LittleEndian>()?;
    pd.margin_bottom = c.read_u32::<LittleEndian>()?;
    pd.margin_header = c.read_u32::<LittleEndian>()?;
    pd.margin_footer = c.read_u32::<LittleEndian>()?;
    pd.margin_gutter = c.read_u32::<LittleEndian>()?;

    // Landscape flag
    let flags = c.read_u32::<LittleEndian>()?;
    pd.landscape = flags & 1 != 0;

    Ok(pd)
}

fn parse_para_header(data: &[u8]) -> Result<Paragraph> {
    let mut para = Paragraph::default();

    if data.len() >= 24 {
        let mut c = Cursor::new(data);
        // Number of characters in text
        let _text_len = c.read_u32::<LittleEndian>()?;
        // Control mask
        let _ctrl_mask = c.read_u32::<LittleEndian>()?;
        // Paragraph shape ID
        para.para_shape_id = c.read_u32::<LittleEndian>()?;
        // Style ID
        para.style_id = c.read_u16::<LittleEndian>()?;
        // Column dividing
        let _col_div = c.read_u8()?;
        // Number of char shape changes
        let _char_shape_cnt = c.read_u16::<LittleEndian>()?;
        // Number of range tags
        let _range_cnt = c.read_u16::<LittleEndian>()?;
        // Number of line alignment
        let _align_cnt = c.read_u16::<LittleEndian>()?;
        // Instance ID
        let _instance_id = c.read_u32::<LittleEndian>()?;
    }

    Ok(para)
}

/// Parse PARA_TEXT record: UTF-16LE text with special character codes.
///
/// Returns `(text, pos_map)` where `pos_map[stream_index]` gives the character
/// index in the output text. This mapping is needed because extended control
/// characters occupy 8 positions in the stream but produce 0-1 output chars.
fn parse_para_text(data: &[u8]) -> Result<(String, Vec<u32>)> {
    let mut text = String::new();
    let u16s: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    // pos_map[i] = index of the output char at u16 stream position i.
    // For positions that don't produce output, it maps to the index of the
    // next char that will be produced (i.e., current text.len()).
    let mut pos_map: Vec<u32> = Vec::with_capacity(u16s.len());
    let mut char_idx: u32 = 0;

    let mut i = 0;
    while i < u16s.len() {
        let ch = u16s[i];

        // Record the mapping for position i before processing
        pos_map.push(char_idx);

        match ch {
            // 8-position extended controls: occupy 8 u16 slots (1 header + 7 data)
            0x0001 | // Reserved
            0x0003 | // Field begin
            0x0004 | // Field end (also 8-position extended)
            0x0008 | // Inline control anchor (table, picture, etc.)
            0x000B | // Drawing object anchor
            0x000C | // Reserved extended
            0x0010 | // Footnote/endnote
            0x0011 | // Auto number
            0x0015   // Page/section/column break control
            => {
                // Fill pos_map for the 7 data positions
                for _ in 0..7 {
                    i += 1;
                    if i < u16s.len() {
                        pos_map.push(char_idx);
                    }
                }
            }
            0x0002 => {
                // Section/column definition — 8 positions, emit newline
                text.push('\n');
                char_idx += 1;
                for _ in 0..7 {
                    i += 1;
                    if i < u16s.len() {
                        pos_map.push(char_idx);
                    }
                }
            }
            // Single-position controls that produce no output
            0x0000 | // NULL
            0x0005..=0x0007 |
            0x000D | // Paragraph end
            0x000E | 0x000F | // Reserved single-position
            0x0012..=0x0014 | // Reserved single-position
            0x0016 | 0x0017 | // Reserved single-position
            0x0019..=0x001D   // Reserved single-position
            => {}
            // Single-position controls that produce output
            0x0009 => { text.push('\t'); char_idx += 1; }
            0x000A => { text.push('\n'); char_idx += 1; }
            0x0018 => { text.push('-'); char_idx += 1; }  // Hyphen
            0x001E => { text.push(' '); char_idx += 1; }  // Non-breaking space
            0x001F => { text.push(' '); char_idx += 1; }  // Fixed-width space
            _ => {
                // Regular character
                if let Some(c) = char::from_u32(ch as u32) {
                    text.push(c);
                    char_idx += 1;
                }
            }
        }
        i += 1;
    }

    Ok((text, pos_map))
}

/// Parse PARA_CHAR_SHAPE record: array of (position_u32, charShapeID_u32) pairs.
fn parse_para_char_shape(data: &[u8]) -> Result<Vec<CharRun>> {
    let mut runs = Vec::new();
    let mut c = Cursor::new(data);

    while (c.position() as usize) + 8 <= data.len() {
        let start_pos = c.read_u32::<LittleEndian>()?;
        let char_shape_id = c.read_u32::<LittleEndian>()?;
        runs.push(CharRun {
            start_pos,
            char_shape_id,
        });
    }

    Ok(runs)
}

fn parse_table_record(data: &[u8]) -> Result<Table> {
    let mut table = Table::default();
    if data.len() >= 8 {
        let mut c = Cursor::new(data);
        // Flags
        let _flags = c.read_u32::<LittleEndian>()?;
        table.rows = c.read_u16::<LittleEndian>()?;
        table.cols = c.read_u16::<LittleEndian>()?;
        if data.len() >= 10 {
            table.cell_spacing = c.read_u16::<LittleEndian>()?;
        }
        // Padding: left, right, top, bottom
        for pad in &mut table.padding {
            if (c.position() as usize) + 2 <= data.len() {
                *pad = c.read_u16::<LittleEndian>()?;
            }
        }
    }
    Ok(table)
}

fn parse_cell_record(data: &[u8]) -> Result<Cell> {
    let mut cell = Cell::default();
    if data.len() >= 22 {
        let mut c = Cursor::new(data);
        cell.col_addr = c.read_u16::<LittleEndian>()?;
        cell.row_addr = c.read_u16::<LittleEndian>()?;
        cell.col_span = c.read_u16::<LittleEndian>()?;
        cell.row_span = c.read_u16::<LittleEndian>()?;
        cell.width = c.read_u32::<LittleEndian>()?;
        cell.height = c.read_u32::<LittleEndian>()?;
        // Padding
        for pad in &mut cell.padding {
            if (c.position() as usize) + 2 <= data.len() {
                *pad = c.read_u16::<LittleEndian>()?;
            }
        }
        if (c.position() as usize) + 2 <= data.len() {
            cell.border_fill_id = c.read_u16::<LittleEndian>()?;
        }
    }
    Ok(cell)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_para_text_simple() {
        // "AB" in UTF-16LE, followed by paragraph end (0x000D)
        let data = [0x41, 0x00, 0x42, 0x00, 0x0D, 0x00];
        let (text, pos_map) = parse_para_text(&data).unwrap();
        assert_eq!(text, "AB");
        // pos_map: [0, 1, 2] — 'A' at 0, 'B' at 1, para-end maps to 2 (past end)
        assert_eq!(pos_map[0], 0); // 'A'
        assert_eq!(pos_map[1], 1); // 'B'
        assert_eq!(pos_map[2], 2); // paragraph end
    }

    #[test]
    fn test_parse_para_text_special_chars() {
        // Tab (0x0009), 'A', line break (0x000A), 'B', paragraph end (0x000D)
        let data = [
            0x09, 0x00, 0x41, 0x00, 0x0A, 0x00, 0x42, 0x00, 0x0D, 0x00,
        ];
        let (text, _) = parse_para_text(&data).unwrap();
        assert_eq!(text, "\tA\nB");
    }

    #[test]
    fn test_parse_para_text_korean() {
        // "한글" (U+D55C U+AE00) + paragraph end
        let data = [0x5C, 0xD5, 0x00, 0xAE, 0x0D, 0x00];
        let (text, _) = parse_para_text(&data).unwrap();
        assert_eq!(text, "한글");
    }

    #[test]
    fn test_parse_para_text_extended_control() {
        // Field begin (0x0003) + 7 data u16s + 'A' + paragraph end
        let mut data = Vec::new();
        data.extend_from_slice(&0x0003u16.to_le_bytes()); // field begin
        for _ in 0..7 {
            data.extend_from_slice(&0x6C6Bu16.to_le_bytes()); // extended data (garbage)
        }
        data.extend_from_slice(&0x0041u16.to_le_bytes()); // 'A'
        data.extend_from_slice(&0x000Du16.to_le_bytes()); // paragraph end

        let (text, pos_map) = parse_para_text(&data).unwrap();
        assert_eq!(text, "A"); // Only 'A', no garbage
        // Position 0-7 are the 8-position field begin control
        // Position 8 is 'A' → char index 0
        assert_eq!(pos_map[8], 0);
    }

    #[test]
    fn test_parse_para_char_shape() {
        let mut data = Vec::new();
        // Run 1: pos=0, shape=3
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        // Run 2: pos=10, shape=5
        data.extend_from_slice(&10u32.to_le_bytes());
        data.extend_from_slice(&5u32.to_le_bytes());

        let runs = parse_para_char_shape(&data).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].start_pos, 0);
        assert_eq!(runs[0].char_shape_id, 3);
        assert_eq!(runs[1].start_pos, 10);
        assert_eq!(runs[1].char_shape_id, 5);
    }

    #[test]
    fn test_parse_page_def() {
        let mut data = vec![0u8; 44];
        let mut c = Cursor::new(&mut data[..]);
        use byteorder::WriteBytesExt;
        c.write_u32::<LittleEndian>(59528).unwrap(); // width
        c.write_u32::<LittleEndian>(84188).unwrap(); // height
        c.write_u32::<LittleEndian>(8504).unwrap(); // margin_left
        c.write_u32::<LittleEndian>(8504).unwrap(); // margin_right
        c.write_u32::<LittleEndian>(5668).unwrap(); // margin_top
        c.write_u32::<LittleEndian>(4252).unwrap(); // margin_bottom
        c.write_u32::<LittleEndian>(4252).unwrap(); // margin_header
        c.write_u32::<LittleEndian>(4252).unwrap(); // margin_footer
        c.write_u32::<LittleEndian>(0).unwrap(); // margin_gutter
        c.write_u32::<LittleEndian>(0).unwrap(); // flags (portrait)

        let pd = parse_page_def(&data).unwrap();
        assert_eq!(pd.width, 59528);
        assert_eq!(pd.height, 84188);
        assert!(!pd.landscape);
    }
}
