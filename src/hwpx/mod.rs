pub mod elements;

use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{HwpError, Result};
use crate::model::charshape::CharShape;
use crate::model::control::Control;
use crate::model::parashape::{Alignment, ParaShape, SpacingType};
use crate::model::style::Style;
use crate::model::table::{Cell, Table};
use crate::model::{
    BinEntry, CharRun, DocInfo, FontFace, HwpDocument, PageDef, Paragraph, Section,
};

use elements::*;

/// Parse an HWPX (ZIP-based XML) document from a reader.
pub fn parse_hwpx<R: Read + Seek>(reader: R) -> Result<HwpDocument> {
    let mut archive = zip::ZipArchive::new(reader)?;

    // 1. Verify mimetype
    if let Ok(mut mimetype_entry) = archive.by_name("mimetype") {
        let mut mimetype = String::new();
        mimetype_entry.read_to_string(&mut mimetype)?;
        let mimetype = mimetype.trim();
        if mimetype != "application/hwp+zip" && mimetype != "hangul/hwpx" {
            return Err(HwpError::InvalidHwpx(format!(
                "unexpected mimetype: {mimetype}"
            )));
        }
    }

    // 2. Find section XML files via content.hpf manifest or by convention
    let section_xmls = find_section_xmls(&mut archive)?;

    // 3. Parse header.xml for doc_info (charPr, paraPr, styles, fonts)
    let mut doc = HwpDocument::default();
    let mut doc_info = DocInfo::default();
    doc_info.fonts = vec![Vec::new(); 7];

    let header_candidates = ["Contents/header.xml", "header.xml"];
    for name in &header_candidates {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut header_xml = String::new();
            entry.read_to_string(&mut header_xml)?;
            parse_header_xml(&header_xml, &mut doc_info)?;
            break;
        }
    }

    // 4. Parse each section
    let mut sections = Vec::new();
    for xml in &section_xmls {
        let section = parse_section_xml(xml, &mut doc_info)?;
        sections.push(section);
    }

    // 5. Extract binary data
    let bin_data = extract_bin_data(&mut archive)?;

    doc.doc_info = doc_info;
    doc.sections = sections;
    doc.bin_data = bin_data;
    doc.header.version = (5, 0, 0, 0);

    Ok(doc)
}

/// Find section XML contents. First try parsing content.hpf as OPF manifest,
/// then fall back to scanning for sectionN.xml files.
fn find_section_xmls<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Vec<String>> {
    // Try to read content.hpf and extract section hrefs
    let hpf_candidates = [
        "Contents/content.hpf",
        "content.hpf",
    ];

    // Read hpf content and prefix (must drop borrow before re-accessing archive)
    let mut hpf_result: Option<(String, String)> = None;
    for hpf_name in &hpf_candidates {
        if let Ok(mut entry) = archive.by_name(hpf_name) {
            let mut hpf_xml = String::new();
            entry.read_to_string(&mut hpf_xml)?;
            let prefix = hpf_name.rsplit_once('/').map(|(p, _)| format!("{p}/")).unwrap_or_default();
            hpf_result = Some((hpf_xml, prefix));
            break;
        }
    }

    if let Some((hpf_xml, prefix)) = hpf_result {
        let section_hrefs = parse_opf_manifest(&hpf_xml);
        if !section_hrefs.is_empty() {
            let mut xmls = Vec::new();
            for href in &section_hrefs {
                let full_path = format!("{prefix}{href}");
                if let Ok(mut sec_entry) = archive.by_name(&full_path) {
                    let mut content = String::new();
                    sec_entry.read_to_string(&mut content)?;
                    xmls.push(content);
                }
            }
            if !xmls.is_empty() {
                return Ok(xmls);
            }
        }
    }

    // Fallback: scan for sectionN.xml
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|e| e.name().to_string()))
        .filter(|n| {
            let lower = n.to_lowercase();
            lower.contains("section") && lower.ends_with(".xml")
        })
        .collect();

    let mut xmls = Vec::new();
    for name in &names {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut content = String::new();
            entry.read_to_string(&mut content)?;
            xmls.push(content);
        }
    }

    if xmls.is_empty() {
        return Err(HwpError::InvalidHwpx("no section XML files found".into()));
    }

    Ok(xmls)
}

/// Parse OPF-style manifest to extract section href paths.
fn parse_opf_manifest(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut hrefs = Vec::new();

    // Collect itemref idrefs from <spine>
    let mut idrefs: Vec<String> = Vec::new();
    // Collect item id→href mapping from <manifest>
    let mut items: Vec<(String, String)> = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let tag = e.local_name();
                match tag.as_ref() {
                    b"item" => {
                        let id = attr_str(e, b"id").unwrap_or_default();
                        let href = attr_str(e, b"href").unwrap_or_default();
                        if !id.is_empty() && !href.is_empty() {
                            items.push((id, href));
                        }
                    }
                    b"itemref" => {
                        if let Some(idref) = attr_str(e, b"idref") {
                            idrefs.push(idref);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    // Return hrefs in spine order
    if !idrefs.is_empty() {
        for idref in &idrefs {
            if let Some((_, href)) = items.iter().find(|(id, _)| id == idref) {
                hrefs.push(href.clone());
            }
        }
    } else {
        // No spine — return all items with xml media type
        for (_, href) in &items {
            hrefs.push(href.clone());
        }
    }

    hrefs
}

/// Parse header.xml for doc_info: charPr, paraPr, style, fontface/font.
fn parse_header_xml(xml: &str, doc_info: &mut DocInfo) -> Result<()> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    let mut font_script_idx: usize = 0;
    // State for multi-element parsing
    let mut current_charpr: Option<CharShape> = None;
    let mut current_parapr: Option<ParaShape> = None;
    let mut in_switch_default = false;
    let mut in_switch_case = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                let tag = local.as_ref();

                match tag {
                    // ── Fonts ──
                    b"fontface" => {
                        // <hh:fontface lang="HANGUL" fontCnt="6">
                        if let Some(lang) = attr_str(e, b"lang") {
                            font_script_idx = match lang.as_str() {
                                "HANGUL" => 0,
                                "LATIN" => 1,
                                "HANJA" => 2,
                                "JAPANESE" => 3,
                                "OTHER" => 4,
                                "SYMBOL" => 5,
                                "USER" => 6,
                                _ => 0,
                            };
                        }
                        // Also handle old-style Type attribute
                        if let Some(script) = attr_str(e, b"Type") {
                            font_script_idx = match script.as_str() {
                                "Korean" => 0,
                                "English" => 1,
                                "Hanja" => 2,
                                "Japanese" => 3,
                                "Other" => 4,
                                "Symbol" => 5,
                                "User" => 6,
                                _ => 0,
                            };
                        }
                    }
                    b"font" => {
                        // <hh:font id="0" face="굴림" type="TTF">
                        let mut face = FontFace::default();
                        if let Some(name) = attr_str(e, b"face") {
                            face.name = name;
                        } else if let Some(name) = attr_str(e, b"Face") {
                            face.name = name;
                        }
                        if font_script_idx < 7 {
                            doc_info.fonts[font_script_idx].push(face);
                        }
                    }

                    // ── CharPr ──
                    b"charPr" => {
                        let mut cs = CharShape::default();
                        if let Some(h) = attr_parse::<u32>(e, b"height") {
                            cs.height = h;
                        }
                        if let Some(c) = attr_str(e, b"textColor") {
                            cs.text_color = parse_hwpx_color(&c);
                        }
                        if let Some(c) = attr_str(e, b"shadeColor") {
                            cs.shade_color = parse_hwpx_color(&c);
                        }
                        if let Some(v) = attr_parse::<u16>(e, b"borderFillIDRef") {
                            cs.border_fill_id = v;
                        }
                        current_charpr = Some(cs);
                    }
                    b"fontRef" => {
                        if let Some(ref mut cs) = current_charpr {
                            // Standard HWPX: <hh:fontRef hangul="2" latin="2" .../>
                            if let Some(v) = attr_parse::<u16>(e, b"hangul") {
                                cs.font_ids[0] = v;
                                cs.font_ids[1] = attr_parse::<u16>(e, b"latin").unwrap_or(0);
                                cs.font_ids[2] = attr_parse::<u16>(e, b"hanja").unwrap_or(0);
                                cs.font_ids[3] = attr_parse::<u16>(e, b"japanese").unwrap_or(0);
                                cs.font_ids[4] = attr_parse::<u16>(e, b"other").unwrap_or(0);
                                cs.font_ids[5] = attr_parse::<u16>(e, b"symbol").unwrap_or(0);
                                cs.font_ids[6] = attr_parse::<u16>(e, b"user").unwrap_or(0);
                            }
                            // Writer format: <hh:fontRef script="Korean" fontId="2"/>
                            if let Some(script) = attr_str(e, b"script") {
                                if let Some(fid) = attr_parse::<u16>(e, b"fontId") {
                                    let idx = match script.as_str() {
                                        "Korean" => 0, "English" => 1, "Hanja" => 2,
                                        "Japanese" => 3, "Other" => 4, "Symbol" => 5, "User" => 6,
                                        _ => 0,
                                    };
                                    cs.font_ids[idx] = fid;
                                }
                            }
                        }
                    }
                    b"ratio" => {
                        if let Some(ref mut cs) = current_charpr {
                            cs.font_ratios[0] = attr_parse::<u8>(e, b"hangul").unwrap_or(100);
                            cs.font_ratios[1] = attr_parse::<u8>(e, b"latin").unwrap_or(100);
                            cs.font_ratios[2] = attr_parse::<u8>(e, b"hanja").unwrap_or(100);
                            cs.font_ratios[3] = attr_parse::<u8>(e, b"japanese").unwrap_or(100);
                            cs.font_ratios[4] = attr_parse::<u8>(e, b"other").unwrap_or(100);
                            cs.font_ratios[5] = attr_parse::<u8>(e, b"symbol").unwrap_or(100);
                            cs.font_ratios[6] = attr_parse::<u8>(e, b"user").unwrap_or(100);
                        }
                    }
                    b"underline" => {
                        if let Some(ref mut cs) = current_charpr {
                            if let Some(t) = attr_str(e, b"type") {
                                cs.underline = match t.as_str() {
                                    "NONE" => crate::model::charshape::UnderlineType::None,
                                    "BOTTOM" | "SINGLE" => crate::model::charshape::UnderlineType::Single,
                                    "DOUBLE" => crate::model::charshape::UnderlineType::Double,
                                    "THICK" => crate::model::charshape::UnderlineType::Thick,
                                    _ => crate::model::charshape::UnderlineType::None,
                                };
                            }
                            if let Some(c) = attr_str(e, b"color") {
                                cs.underline_color = parse_hwpx_color(&c);
                            }
                        }
                    }
                    b"strikeout" => {
                        if let Some(ref mut cs) = current_charpr {
                            if let Some(s) = attr_str(e, b"shape") {
                                cs.strikeout = match s.as_str() {
                                    "NONE" => crate::model::charshape::StrikeoutType::None,
                                    _ => crate::model::charshape::StrikeoutType::Single,
                                };
                            }
                        }
                    }

                    // ── ParaPr ──
                    b"paraPr" => {
                        let mut ps = ParaShape::default();
                        if let Some(v) = attr_parse::<u16>(e, b"tabPrIDRef") {
                            ps.tab_def_id = v;
                        }
                        current_parapr = Some(ps);
                    }
                    b"align" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(h) = attr_str(e, b"horizontal") {
                                ps.align = match h.as_str() {
                                    "JUSTIFY" => Alignment::Justify,
                                    "LEFT" => Alignment::Left,
                                    "RIGHT" => Alignment::Right,
                                    "CENTER" => Alignment::Center,
                                    "DISTRIBUTE" => Alignment::Distribute,
                                    _ => Alignment::Justify,
                                };
                            }
                        }
                    }
                    b"heading" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(t) = attr_str(e, b"type") {
                                ps.heading_type = match t.as_str() {
                                    "NONE" => 0,
                                    "OUTLINE" => 1,
                                    "NUMBER" => 2,
                                    "BULLET" => 3,
                                    _ => 0,
                                };
                            }
                            if let Some(v) = attr_parse::<u8>(e, b"level") {
                                ps.heading_level = v;
                            }
                        }
                    }
                    b"breakSetting" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_str(e, b"pageBreakBefore") {
                                ps.page_break_before = v == "1" || v == "true";
                            }
                        }
                    }
                    b"margin" => {
                        // Margin inside paraPr — parse child elements (intent/left/right/prev/next)
                        // These appear as child <hc:intent>, <hc:left> etc.
                        // We'll handle them via the child element tags below
                    }
                    b"intent" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.indent = v;
                            }
                        }
                    }
                    b"left" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.margin_left = v;
                            }
                        }
                    }
                    b"right" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.margin_right = v;
                            }
                        }
                    }
                    b"prev" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.space_before = v;
                            }
                        }
                    }
                    b"next" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.space_after = v;
                            }
                        }
                    }
                    b"lineSpacing" => {
                        if let Some(ref mut ps) = current_parapr {
                            if let Some(v) = attr_parse::<i32>(e, b"value") {
                                ps.line_spacing = v;
                            }
                            if let Some(t) = attr_str(e, b"type") {
                                ps.line_spacing_type = match t.as_str() {
                                    "PERCENT" => SpacingType::Percent,
                                    "FIXED" => SpacingType::Fixed,
                                    "BETWEEN_LINES" => SpacingType::BetweenLines,
                                    "AT_LEAST" => SpacingType::AtLeast,
                                    _ => SpacingType::Percent,
                                };
                            }
                        }
                    }

                    // ── Style ──
                    b"style" => {
                        let mut style = Style::default();
                        if let Some(n) = attr_str(e, b"name") {
                            style.name = n;
                        }
                        if let Some(n) = attr_str(e, b"engName") {
                            style.english_name = n;
                        }
                        if let Some(t) = attr_str(e, b"type") {
                            style.style_type = match t.as_str() {
                                "PARA" => 0,
                                "CHAR" => 1,
                                _ => 0,
                            };
                        }
                        if let Some(v) = attr_parse::<u16>(e, b"paraPrIDRef") {
                            style.para_shape_id = v;
                        }
                        if let Some(v) = attr_parse::<u16>(e, b"charPrIDRef") {
                            style.char_shape_id = v;
                        }
                        if let Some(v) = attr_parse::<u16>(e, b"nextStyleIDRef") {
                            style.next_style_id = v;
                        }
                        doc_info.styles.push(style);
                    }

                    // Skip hp:switch/hp:case/hp:default — we parse the first occurrence
                    b"case" => { in_switch_case = true; }
                    b"default" => { in_switch_default = true; }

                    // Handle writer-generated tags (charShape/paraShape with flat attrs)
                    b"charShape" | b"CHARSHAPE" => {
                        let cs = parse_charshape_element(e);
                        doc_info.char_shapes.push(cs);
                    }
                    b"paraShape" | b"PARASHAPE" => {
                        let ps = parse_parashape_element(e);
                        doc_info.para_shapes.push(ps);
                    }
                    b"STYLE" => {
                        let style = parse_style_element(e);
                        doc_info.styles.push(style);
                    }
                    b"FONT" => {
                        let face = parse_font_element(e);
                        if font_script_idx < 7 {
                            doc_info.fonts[font_script_idx].push(face);
                        }
                    }
                    b"FONTFACE" => {
                        if let Some(script) = attr_str(e, b"Type") {
                            font_script_idx = match script.as_str() {
                                "Korean" => 0,
                                "English" => 1,
                                "Hanja" => 2,
                                "Japanese" => 3,
                                "Other" => 4,
                                "Symbol" => 5,
                                "User" => 6,
                                _ => 0,
                            };
                        }
                    }

                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let tag = e.local_name();
                match tag.as_ref() {
                    b"charPr" => {
                        if let Some(cs) = current_charpr.take() {
                            doc_info.char_shapes.push(cs);
                        }
                    }
                    b"paraPr" => {
                        if let Some(ps) = current_parapr.take() {
                            doc_info.para_shapes.push(ps);
                        }
                    }
                    b"case" => { in_switch_case = false; }
                    b"default" => { in_switch_default = false; }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(())
}

/// Parse HWPX color string ("#RRGGBB" or "none") to u32.
fn parse_hwpx_color(s: &str) -> u32 {
    if s == "none" || s.is_empty() {
        return 0xFFFFFF;
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    u32::from_str_radix(hex, 16).unwrap_or(0)
}

/// Parse a single section XML into a Section, also collecting doc_info.
fn parse_section_xml(xml: &str, doc_info: &mut DocInfo) -> Result<Section> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut section = Section::default();
    let mut current_para: Option<Paragraph> = None;
    let mut current_table: Option<Table> = None;
    let mut current_cell: Option<Cell> = None;
    let mut current_page_def: Option<PageDef> = None;
    let mut in_text = false;
    let mut font_script_idx: usize = 0;

    // Footnote/endnote parsing state
    let mut footnote_state: Option<FootnoteBuildState> = None;

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                let tag = local.as_ref();

                match tag {
                    // DocInfo elements (may appear in section XML in HWPX)
                    b"charShape" | b"CHARSHAPE" => {
                        let cs = parse_charshape_element(e);
                        doc_info.char_shapes.push(cs);
                    }
                    b"FONT" => {
                        let face = parse_font_element(e);
                        if font_script_idx < 7 {
                            doc_info.fonts[font_script_idx].push(face);
                        }
                    }
                    b"FONTFACE" => {
                        if let Some(script) = attr_str(e, b"Type") {
                            font_script_idx = match script.as_str() {
                                "Korean" => 0,
                                "English" => 1,
                                "Hanja" => 2,
                                "Japanese" => 3,
                                "Other" => 4,
                                "Symbol" => 5,
                                "User" => 6,
                                _ => 0,
                            };
                        }
                    }
                    b"paraShape" | b"PARASHAPE" => {
                        let ps = parse_parashape_element(e);
                        doc_info.para_shapes.push(ps);
                    }
                    b"STYLE" => {
                        let style = parse_style_element(e);
                        doc_info.styles.push(style);
                    }

                    // Section structure
                    b"sec" | b"SECTION" => {
                        // Section root — already created
                    }
                    b"SECDEF" | b"secDef" => {
                        section.sec_def = parse_secdef_element(e);
                    }
                    b"PAGEDEF" | b"pageDef" => {
                        current_page_def = Some(parse_pagedef_element(e));
                    }
                    b"PAGEMARGIN" | b"pageMargin" => {
                        if let Some(ref mut pd) = current_page_def {
                            apply_page_margin(pd, e);
                        }
                    }

                    // Paragraphs: <hp:p> → local name "p"
                    b"p" | b"P" | b"PARA" => {
                        // Flush previous paragraph
                        flush_para(
                            &mut current_para,
                            &mut current_cell,
                            &mut section,
                        );
                        let mut para = Paragraph::default();
                        if let Some(id) = attr_parse::<u32>(e, b"paraPrIDRef")
                            .or_else(|| attr_parse::<u32>(e, b"ParaShapeId")) {
                            para.para_shape_id = id;
                        }
                        if let Some(id) = attr_parse::<u16>(e, b"styleIDRef")
                            .or_else(|| attr_parse::<u16>(e, b"StyleId")) {
                            para.style_id = id;
                        }
                        current_para = Some(para);
                    }

                    // Text: <hp:t> → local name "t"
                    b"t" | b"T" | b"TEXT" => {
                        in_text = true;
                    }

                    // Character shape run
                    b"run" => {
                        // <hp:run> — may contain charPrIDRef or charShapeIdRef attribute
                        if let Some(ref mut para) = current_para {
                            if let Some(cs_id) = attr_parse::<u32>(e, b"charPrIDRef")
                                .or_else(|| attr_parse::<u32>(e, b"charShapeIdRef")) {
                                let pos = para.text.len() as u32;
                                para.char_runs.push(CharRun {
                                    start_pos: pos,
                                    char_shape_id: cs_id,
                                });
                            }
                        }
                    }
                    b"CHARSHAPERUN" | b"CHAR" => {
                        if let Some(ref mut para) = current_para {
                            let run = CharRun {
                                start_pos: attr_parse(e, b"CharPos").unwrap_or(0),
                                char_shape_id: attr_parse(e, b"CharShapeId").unwrap_or(0),
                            };
                            para.char_runs.push(run);
                        }
                    }

                    // Tables: <hp:tbl> → "tbl"
                    b"tbl" | b"TBL" | b"TABLE" => {
                        let table = parse_table_element(e);
                        current_table = Some(table);
                    }
                    // Table row: <hp:tr> → "tr"
                    b"tr" => {
                        // Row start — no action needed, cells track row/col
                    }
                    // Table cell: <hp:tc> → "tc"
                    b"tc" | b"TC" | b"CELL" => {
                        // Flush current cell paragraphs
                        if let Some(cell) = current_cell.take() {
                            if let Some(ref mut table) = current_table {
                                table.cells.push(cell);
                            }
                        }
                        current_cell = Some(parse_cell_element(e));
                        // now inside a cell
                    }

                    // Cell sub-elements: <hp:cellAddr>, <hp:cellSpan>, <hp:cellSz>
                    b"cellAddr" => {
                        if let Some(ref mut cell) = current_cell {
                            if let Some(v) = attr_parse::<u16>(e, b"colAddr") {
                                cell.col_addr = v;
                            }
                            if let Some(v) = attr_parse::<u16>(e, b"rowAddr") {
                                cell.row_addr = v;
                            }
                        }
                    }
                    b"cellSpan" => {
                        if let Some(ref mut cell) = current_cell {
                            if let Some(v) = attr_parse::<u16>(e, b"colSpan") {
                                cell.col_span = v;
                            }
                            if let Some(v) = attr_parse::<u16>(e, b"rowSpan") {
                                cell.row_span = v;
                            }
                        }
                    }
                    b"cellSz" => {
                        if let Some(ref mut cell) = current_cell {
                            if let Some(v) = attr_parse::<u32>(e, b"width") {
                                cell.width = v;
                            }
                            if let Some(v) = attr_parse::<u32>(e, b"height") {
                                cell.height = v;
                            }
                        }
                    }
                    b"cellMargin" => {
                        if let Some(ref mut cell) = current_cell {
                            cell.padding[0] = attr_parse::<u16>(e, b"left").unwrap_or(0);
                            cell.padding[1] = attr_parse::<u16>(e, b"right").unwrap_or(0);
                            cell.padding[2] = attr_parse::<u16>(e, b"top").unwrap_or(0);
                            cell.padding[3] = attr_parse::<u16>(e, b"bottom").unwrap_or(0);
                        }
                    }

                    // Footnote / endnote
                    b"footNote" | b"endNote" => {
                        let is_endnote = tag == b"endNote";
                        let number = attr_parse::<u16>(e, b"number").unwrap_or(0);
                        // Save current paragraph — footnote content will be collected separately
                        let saved = current_para.take();
                        footnote_state = Some(FootnoteBuildState {
                            is_endnote,
                            number,
                            paragraphs: Vec::new(),
                            saved_para: saved,
                            in_sub_list: false,
                        });
                    }
                    b"subList" => {
                        if let Some(ref mut fs) = footnote_state {
                            fs.in_sub_list = true;
                        }
                    }

                    // Hyperlink field via <hp:ctrl><hp:field type="hyperlink">
                    b"field" => {
                        if let Some(ref mut para) = current_para {
                            if let Some(ftype) = attr_str(e, b"type") {
                                if ftype == "hyperlink" {
                                    let url = attr_str(e, b"command")
                                        .unwrap_or_default()
                                        .replace("\\:", ":");
                                    para.controls.push(Control::Field(
                                        crate::model::control::Field::Hyperlink {
                                            url,
                                            text: String::new(),
                                        },
                                    ));
                                }
                            }
                        }
                    }

                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    if let Some(ref mut fs) = footnote_state {
                        // Text inside footnote subList goes to footnote paragraph
                        if fs.in_sub_list {
                            if let Some(ref mut para) = current_para {
                                let text = e.unescape().unwrap_or_default();
                                para.text.push_str(&text);
                            }
                        }
                    } else if let Some(ref mut para) = current_para {
                        let text = e.unescape().unwrap_or_default();
                        para.text.push_str(&text);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                let tag = local.as_ref();

                match tag {
                    b"t" | b"T" | b"TEXT" => {
                        in_text = false;
                    }
                    b"p" | b"P" | b"PARA" => {
                        if let Some(ref mut fs) = footnote_state {
                            // Inside footnote — collect paragraph into footnote
                            if fs.in_sub_list {
                                if let Some(para) = current_para.take() {
                                    fs.paragraphs.push(para);
                                }
                            }
                        } else {
                            flush_para(
                                &mut current_para,
                                &mut current_cell,
                                &mut section,
                            );
                        }
                    }
                    b"subList" => {
                        if let Some(ref mut fs) = footnote_state {
                            fs.in_sub_list = false;
                        }
                    }
                    b"footNote" | b"endNote" => {
                        if let Some(fs) = footnote_state.take() {
                            // Restore saved paragraph and attach footnote as control
                            current_para = fs.saved_para;
                            if let Some(ref mut para) = current_para {
                                para.controls.push(Control::FootnoteEndnote(
                                    crate::model::control::FootnoteEndnote {
                                        is_endnote: fs.is_endnote,
                                        number: fs.number,
                                        paragraphs: fs.paragraphs,
                                    },
                                ));
                            }
                        }
                    }
                    b"tc" | b"TC" | b"CELL" => {
                        // Flush paragraph into cell, then cell will be flushed on next tc or tbl end
                        flush_para(
                            &mut current_para,
                            &mut current_cell,
                            &mut section,
                        );
                        // exited cell/table
                    }
                    b"tbl" | b"TBL" | b"TABLE" => {
                        // Flush last cell
                        flush_para(
                            &mut current_para,
                            &mut current_cell,
                            &mut section,
                        );
                        if let Some(cell) = current_cell.take() {
                            if let Some(ref mut table) = current_table {
                                table.cells.push(cell);
                            }
                        }
                        if let Some(table) = current_table.take() {
                            // Attach table as a control on a paragraph
                            let mut p = Paragraph::default();
                            p.controls.push(Control::Table(table));
                            section.paragraphs.push(p);
                        }
                        // exited cell/table
                    }
                    b"sec" | b"SECTION" => {
                        flush_para(
                            &mut current_para,
                            &mut current_cell,
                            &mut section,
                        );
                        if let Some(pd) = current_page_def.take() {
                            section.sec_def.page_def = pd;
                        }
                    }
                    b"PAGEDEF" | b"pageDef" => {
                        if let Some(pd) = current_page_def.take() {
                            section.sec_def.page_def = pd;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(HwpError::Xml(e)),
            _ => {}
        }
        buf.clear();
    }

    // Flush remaining
    flush_para(
        &mut current_para,
        &mut current_cell,
        &mut section,
    );
    if let Some(pd) = current_page_def.take() {
        section.sec_def.page_def = pd;
    }

    Ok(section)
}

/// State for building a footnote/endnote from HWPX XML.
struct FootnoteBuildState {
    is_endnote: bool,
    number: u16,
    paragraphs: Vec<Paragraph>,
    /// The paragraph that was being built when we entered the footnote.
    /// We save it here and restore it when the footnote ends.
    saved_para: Option<Paragraph>,
    in_sub_list: bool,
}

fn flush_para(
    current_para: &mut Option<Paragraph>,
    current_cell: &mut Option<Cell>,
    section: &mut Section,
) {
    if let Some(para) = current_para.take() {
        if let Some(ref mut cell) = current_cell {
            cell.paragraphs.push(para);
        } else {
            section.paragraphs.push(para);
        }
    }
}

fn extract_bin_data<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Vec<BinEntry>> {
    let mut entries = Vec::new();

    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive
                .by_index(i)
                .ok()
                .map(|e| e.name().to_string())
                .filter(|n| n.starts_with("BinData/") || n.starts_with("bindata/"))
        })
        .collect();

    for name in &names {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut data = Vec::new();
            if entry.read_to_end(&mut data).is_ok() && !data.is_empty() {
                let entry_name = name
                    .strip_prefix("BinData/")
                    .or_else(|| name.strip_prefix("bindata/"))
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
