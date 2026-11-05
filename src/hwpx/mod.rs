pub mod elements;

use std::io::{Read, Seek};

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{HwpError, Result};
use crate::model::control::Control;
use crate::model::table::{Cell, Table};
use crate::model::{
    BinEntry, CharRun, DocInfo, HwpDocument, PageDef, Paragraph, Section,
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

    // 3. Parse each section
    let mut doc = HwpDocument::default();
    let mut doc_info = DocInfo::default();
    doc_info.fonts = vec![Vec::new(); 7];

    let mut sections = Vec::new();
    for xml in &section_xmls {
        let section = parse_section_xml(xml, &mut doc_info)?;
        sections.push(section);
    }

    // 4. Extract binary data
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

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                let tag = local.as_ref();

                match tag {
                    // DocInfo elements (may appear in section XML in HWPX)
                    b"CHARSHAPE" => {
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
                    b"PARASHAPE" => {
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
                        if let Some(id) = attr_parse::<u32>(e, b"ParaShapeId") {
                            para.para_shape_id = id;
                        }
                        if let Some(id) = attr_parse::<u16>(e, b"StyleId") {
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
                        // <hp:run> — may contain charShapeIdRef attribute
                        if let Some(ref mut para) = current_para {
                            if let Some(cs_id) = attr_parse::<u32>(e, b"charShapeIdRef") {
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

                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_text {
                    if let Some(ref mut para) = current_para {
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
                        flush_para(
                            &mut current_para,
                            &mut current_cell,
                            &mut section,
                        );
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
