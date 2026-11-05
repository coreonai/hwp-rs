use std::io::{Cursor, Read, Seek};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::error::{HwpError, Result};
use crate::model::charshape::{
    CharShape, OutlineType, ShadowType, StrikeoutType, UnderlineType,
};
use crate::model::parashape::{Alignment, ParaShape, SpacingType};
use crate::model::style::Style;
use crate::model::{
    BinDataDef, BorderFill, Bullet, DocInfo, FileHeader, FontFace, Numbering, TabDef,
};

use super::decompress::decompress_stream;
use super::record::{parse_records, Record};

// Tag IDs (DocInfo stream)
const TAG_DOC_PROPERTIES: u16 = 16;
const TAG_ID_MAPPINGS: u16 = 17;
const TAG_BIN_DATA: u16 = 18;
const TAG_FACE_NAME: u16 = 19;
const TAG_BORDER_FILL: u16 = 20;
const TAG_CHAR_SHAPE: u16 = 21;
const TAG_TAB_DEF: u16 = 22;
const TAG_NUMBERING: u16 = 23;
const TAG_BULLET: u16 = 24;
const TAG_PARA_SHAPE: u16 = 25;
const TAG_STYLE: u16 = 26;

/// Parse the DocInfo stream from an OLE2 compound file.
pub fn parse_doc_info<R: Read + Seek>(
    cfb: &mut cfb::CompoundFile<R>,
    header: &FileHeader,
) -> Result<DocInfo> {
    let mut stream = cfb.open_stream("/DocInfo")?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw)?;

    let data = if header.compressed {
        decompress_stream(&raw)?
    } else {
        raw
    };

    let records = parse_records(&data)?;
    parse_doc_info_records(&records)
}

/// Parse DocInfo records into a DocInfo struct.
fn parse_doc_info_records(records: &[Record]) -> Result<DocInfo> {
    let mut info = DocInfo::default();
    // Initialize 7 font lists (one per script type)
    info.fonts = vec![Vec::new(); 7];

    // Track expected counts from ID_MAPPINGS
    let mut _expected_counts: Option<IdMappingCounts> = None;
    let current_font_script: usize = 0;

    for record in records {
        match record.header.tag {
            TAG_DOC_PROPERTIES => {
                // Doc properties (section count etc.) — we skip for now
            }
            TAG_ID_MAPPINGS => {
                _expected_counts = Some(parse_id_mappings(&record.payload)?);
            }
            TAG_BIN_DATA => {
                if let Ok(bd) = parse_bin_data_def(&record.payload) {
                    info.bin_data_defs.push(bd);
                }
            }
            TAG_FACE_NAME => {
                if let Ok(face) = parse_face_name(&record.payload) {
                    if current_font_script < 7 {
                        info.fonts[current_font_script].push(face);
                    }
                }
            }
            TAG_BORDER_FILL => {
                if let Ok(bf) = parse_border_fill(&record.payload) {
                    info.border_fills.push(bf);
                }
            }
            TAG_CHAR_SHAPE => {
                if let Ok(cs) = parse_char_shape(&record.payload) {
                    info.char_shapes.push(cs);
                }
            }
            TAG_TAB_DEF => {
                info.tab_defs.push(TabDef::default());
            }
            TAG_NUMBERING => {
                info.numberings.push(Numbering::default());
            }
            TAG_BULLET => {
                info.bullets.push(Bullet::default());
            }
            TAG_PARA_SHAPE => {
                if let Ok(ps) = parse_para_shape(&record.payload) {
                    info.para_shapes.push(ps);
                }
            }
            TAG_STYLE => {
                if let Ok(style) = parse_style(&record.payload) {
                    info.styles.push(style);
                    // After all face names for one script type are read,
                    // increment the script counter. Styles come after face names.
                }
            }
            _ => {
                // Face names are grouped by script type. Detect boundary by counting.
                // The ID_MAPPINGS record tells us how many face names per script.
                // For simplicity, we advance the script counter based on record ordering.
            }
        }
    }

    // Fix font script assignment: face names come in groups per script type.
    // Re-parse to properly distribute them.
    reassign_fonts_by_counts(records, &mut info);

    Ok(info)
}

#[derive(Debug, Default)]
struct IdMappingCounts {
    bin_data: u32,
    font_korean: u32,
    font_english: u32,
    font_hanja: u32,
    font_japanese: u32,
    font_other: u32,
    font_symbol: u32,
    font_user: u32,
    border_fill: u32,
    char_shape: u32,
    tab_def: u32,
    numbering: u32,
    bullet: u32,
    para_shape: u32,
    style: u32,
}

fn parse_id_mappings(data: &[u8]) -> Result<IdMappingCounts> {
    let mut c = Cursor::new(data);
    let mut counts = IdMappingCounts::default();

    let read = |c: &mut Cursor<&[u8]>| -> Result<u32> { Ok(c.read_u32::<LittleEndian>()?) };

    counts.bin_data = read(&mut c)?;
    counts.font_korean = read(&mut c)?;
    counts.font_english = read(&mut c)?;
    counts.font_hanja = read(&mut c)?;
    counts.font_japanese = read(&mut c)?;
    counts.font_other = read(&mut c)?;
    counts.font_symbol = read(&mut c)?;
    counts.font_user = read(&mut c)?;
    counts.border_fill = read(&mut c)?;
    counts.char_shape = read(&mut c)?;
    counts.tab_def = read(&mut c)?;
    counts.numbering = read(&mut c)?;
    counts.bullet = read(&mut c)?;
    counts.para_shape = read(&mut c)?;
    counts.style = read(&mut c)?;

    Ok(counts)
}

/// Reassign font faces to correct script buckets using ID_MAPPINGS counts.
fn reassign_fonts_by_counts(records: &[Record], info: &mut DocInfo) {
    // Find ID_MAPPINGS to get face name counts per script
    let counts = records
        .iter()
        .find(|r| r.header.tag == TAG_ID_MAPPINGS)
        .and_then(|r| parse_id_mappings(&r.payload).ok());

    let counts = match counts {
        Some(c) => c,
        None => return,
    };

    let font_counts = [
        counts.font_korean,
        counts.font_english,
        counts.font_hanja,
        counts.font_japanese,
        counts.font_other,
        counts.font_symbol,
        counts.font_user,
    ];

    // Collect all face names in order
    let all_faces: Vec<FontFace> = records
        .iter()
        .filter(|r| r.header.tag == TAG_FACE_NAME)
        .filter_map(|r| parse_face_name(&r.payload).ok())
        .collect();

    // Distribute into script buckets
    info.fonts = vec![Vec::new(); 7];
    let mut offset = 0usize;
    for (i, &count) in font_counts.iter().enumerate() {
        let end = offset + count as usize;
        if end <= all_faces.len() {
            info.fonts[i] = all_faces[offset..end].to_vec();
        }
        offset = end;
    }
}

fn parse_face_name(data: &[u8]) -> Result<FontFace> {
    let mut c = Cursor::new(data);
    let mut face = FontFace::default();

    // Property flags
    let _flags = c.read_u8()?;
    // Font name: u16 length + UTF-16LE
    let name_len = c.read_u16::<LittleEndian>()? as usize;
    let mut name_buf = vec![0u8; name_len * 2];
    c.read_exact(&mut name_buf)?;
    face.name = utf16le_to_string(&name_buf);

    // Optional: alternate font info (we skip for now)
    // Family type, etc.
    if data.len() > c.position() as usize + 1 {
        // Try to read family type
        face.family = c.read_u8().unwrap_or(0);
    }

    Ok(face)
}

fn parse_bin_data_def(data: &[u8]) -> Result<BinDataDef> {
    let mut c = Cursor::new(data);
    let mut bd = BinDataDef::default();

    let flags = c.read_u16::<LittleEndian>()?;
    bd.storage_type = flags & 0x000F;

    match bd.storage_type {
        0 => {
            // LINK: absolute path + relative path
            let abs_len = c.read_u16::<LittleEndian>()? as usize;
            let mut abs_buf = vec![0u8; abs_len * 2];
            c.read_exact(&mut abs_buf)?;
            bd.abs_path = Some(utf16le_to_string(&abs_buf));

            let rel_len = c.read_u16::<LittleEndian>()? as usize;
            let mut rel_buf = vec![0u8; rel_len * 2];
            c.read_exact(&mut rel_buf)?;
            bd.rel_path = Some(utf16le_to_string(&rel_buf));
        }
        1 => {
            // EMBEDDING: binDataID + extension
            bd.bin_data_id = c.read_u16::<LittleEndian>()?;
            let ext_len = c.read_u16::<LittleEndian>()? as usize;
            let mut ext_buf = vec![0u8; ext_len * 2];
            c.read_exact(&mut ext_buf)?;
            bd.extension = utf16le_to_string(&ext_buf);
        }
        2 => {
            // STORAGE: OLE storage
            bd.bin_data_id = c.read_u16::<LittleEndian>()?;
        }
        _ => {}
    }

    Ok(bd)
}

fn parse_char_shape(data: &[u8]) -> Result<CharShape> {
    if data.len() < 72 {
        return Err(HwpError::InvalidRecord(format!(
            "CharShape too short: {} bytes",
            data.len()
        )));
    }
    let mut c = Cursor::new(data);
    let mut cs = CharShape::default();

    // Font IDs: 7 × u16
    for i in 0..7 {
        cs.font_ids[i] = c.read_u16::<LittleEndian>()?;
    }
    // Font ratios: 7 × u8
    for i in 0..7 {
        cs.font_ratios[i] = c.read_u8()?;
    }
    // Font spacings: 7 × i8
    for i in 0..7 {
        cs.font_spacings[i] = c.read_i8()?;
    }
    // Font relative sizes: 7 × u8
    for i in 0..7 {
        cs.font_rel_sizes[i] = c.read_u8()?;
    }
    // Font offsets: 7 × i8
    for i in 0..7 {
        cs.font_offsets[i] = c.read_i8()?;
    }
    // Height
    cs.height = c.read_u32::<LittleEndian>()?;
    // Property flags u32
    let props = c.read_u32::<LittleEndian>()?;
    cs.italic = props & (1 << 0) != 0;
    cs.bold = props & (1 << 1) != 0;
    cs.underline = UnderlineType::from(((props >> 2) & 0x07) as u8);
    let ul_shape = ((props >> 5) & 0x0F) as u8;
    if ul_shape > 0 {
        cs.underline = UnderlineType::from(ul_shape);
    }
    cs.outline = OutlineType::from(((props >> 9) & 0x07) as u8);
    cs.shadow_type = ShadowType::from(((props >> 12) & 0x03) as u8);
    cs.emboss = props & (1 << 14) != 0;
    cs.engrave = props & (1 << 15) != 0;
    cs.superscript = props & (1 << 16) != 0;
    cs.subscript = props & (1 << 17) != 0;
    cs.strikeout = StrikeoutType::from(((props >> 18) & 0x07) as u8);

    // Shadow offset x, y (i8 each) — skip
    let _shadow_offset_x = c.read_i8()?;
    let _shadow_offset_y = c.read_i8()?;

    // Text color
    cs.text_color = c.read_u32::<LittleEndian>()? & 0x00FFFFFF;
    // Underline color
    cs.underline_color = c.read_u32::<LittleEndian>()? & 0x00FFFFFF;
    // Shade color
    cs.shade_color = c.read_u32::<LittleEndian>()? & 0x00FFFFFF;
    // Shadow color
    cs.shadow_color = c.read_u32::<LittleEndian>()? & 0x00FFFFFF;

    // Border fill ID (if present)
    if data.len() >= c.position() as usize + 2 {
        cs.border_fill_id = c.read_u16::<LittleEndian>()?;
    }

    Ok(cs)
}

fn parse_para_shape(data: &[u8]) -> Result<ParaShape> {
    if data.len() < 54 {
        return Err(HwpError::InvalidRecord(format!(
            "ParaShape too short: {} bytes",
            data.len()
        )));
    }
    let mut c = Cursor::new(data);
    let mut ps = ParaShape::default();

    // Property flags 1 (u32)
    let props1 = c.read_u32::<LittleEndian>()?;
    ps.align = Alignment::from((props1 & 0x07) as u8);
    ps.page_break_before = props1 & (1 << 3) != 0;
    ps.keep_with_next = props1 & (1 << 4) != 0;
    ps.keep_together = props1 & (1 << 5) != 0;
    ps.widow_orphan = props1 & (1 << 6) != 0;
    ps.heading_type = ((props1 >> 7) & 0x03) as u8;
    ps.heading_level = ((props1 >> 9) & 0x07) as u8;

    // Left margin
    ps.margin_left = c.read_i32::<LittleEndian>()?;
    // Right margin
    ps.margin_right = c.read_i32::<LittleEndian>()?;
    // Indent
    ps.indent = c.read_i32::<LittleEndian>()?;
    // Space before
    ps.space_before = c.read_i32::<LittleEndian>()?;
    // Space after
    ps.space_after = c.read_i32::<LittleEndian>()?;

    // Line spacing type + value
    // In some versions this is: lineSpacingType(u32) + lineSpacing(i32)
    // In others it's packed differently. Handle the common case.
    let ls_type = c.read_u32::<LittleEndian>()?;
    ps.line_spacing_type = SpacingType::from((ls_type & 0xFF) as u8);
    ps.line_spacing = c.read_i32::<LittleEndian>()?;

    // Tab def ID
    ps.tab_def_id = c.read_u16::<LittleEndian>()?;
    // Border fill ID
    ps.border_fill_id = c.read_u16::<LittleEndian>()?;

    Ok(ps)
}

fn parse_border_fill(data: &[u8]) -> Result<BorderFill> {
    // Minimal parse — just create a default
    let mut bf = BorderFill::default();
    if data.len() >= 2 {
        let mut c = Cursor::new(data);
        // Flags
        let _flags = c.read_u16::<LittleEndian>()?;
        // Full parsing of border types, widths, colors would follow
        // but we keep it minimal for now
    }
    bf.id = 0;
    Ok(bf)
}

fn parse_style(data: &[u8]) -> Result<Style> {
    let mut c = Cursor::new(data);
    let mut style = Style::default();

    // Name: u16 length + UTF-16LE
    let name_len = c.read_u16::<LittleEndian>()? as usize;
    let mut name_buf = vec![0u8; name_len * 2];
    c.read_exact(&mut name_buf)?;
    style.name = utf16le_to_string(&name_buf);

    // English name: u16 length + UTF-16LE
    let ename_len = c.read_u16::<LittleEndian>()? as usize;
    let mut ename_buf = vec![0u8; ename_len * 2];
    c.read_exact(&mut ename_buf)?;
    style.english_name = utf16le_to_string(&ename_buf);

    // Style type
    style.style_type = c.read_u8()?;
    // Next style
    style.next_style_id = c.read_u16::<LittleEndian>().unwrap_or(0);
    // Char shape ID
    style.char_shape_id = c.read_u16::<LittleEndian>().unwrap_or(0);
    // Para shape ID
    style.para_shape_id = c.read_u16::<LittleEndian>().unwrap_or(0);

    Ok(style)
}

/// Convert UTF-16LE bytes to a Rust String.
pub(crate) fn utf16le_to_string(data: &[u8]) -> String {
    let u16s: Vec<u16> = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf16le_to_string() {
        // "ABC" in UTF-16LE
        let data = [0x41, 0x00, 0x42, 0x00, 0x43, 0x00];
        assert_eq!(utf16le_to_string(&data), "ABC");
    }

    #[test]
    fn test_utf16le_korean() {
        // "한" (U+D55C) in UTF-16LE = 0x5C, 0xD5
        let data = [0x5C, 0xD5];
        assert_eq!(utf16le_to_string(&data), "한");
    }

    #[test]
    fn test_parse_id_mappings() {
        let mut data = Vec::new();
        for i in 0u32..15 {
            data.extend_from_slice(&(i + 1).to_le_bytes());
        }
        let counts = parse_id_mappings(&data).unwrap();
        assert_eq!(counts.bin_data, 1);
        assert_eq!(counts.font_korean, 2);
        assert_eq!(counts.style, 15);
    }
}
