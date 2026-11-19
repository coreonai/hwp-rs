//! HWP document to Markdown conversion.

use crate::model::charshape::{StrikeoutType, UnderlineType};
use crate::model::control::{Control, Field};
use crate::model::shape::ShapeObject;
use crate::model::table::Table;
use crate::model::{DocInfo, HwpDocument, Paragraph, Section};

/// Convert an [`HwpDocument`] into a Markdown string.
///
/// Images are referenced as `/api/image/{bin_data_id}` for serving via HTTP.
pub fn document_to_markdown(doc: &HwpDocument) -> String {
    let mut out = String::new();
    let mut footnotes: Vec<(u16, String)> = Vec::new();

    for (si, section) in doc.sections.iter().enumerate() {
        if si > 0 {
            out.push_str("\n---\n\n");
        }
        convert_section(section, &doc.doc_info, &mut out, &mut footnotes);
    }

    // Append footnotes at document end
    if !footnotes.is_empty() {
        out.push('\n');
        for (num, text) in &footnotes {
            out.push_str(&format!("[^{}]: {}\n", num, text));
        }
    }

    out
}

fn convert_section(
    section: &Section,
    doc_info: &DocInfo,
    out: &mut String,
    footnotes: &mut Vec<(u16, String)>,
) {
    for para in &section.paragraphs {
        convert_paragraph(para, doc_info, out, footnotes);
    }
}

fn convert_paragraph(
    para: &Paragraph,
    doc_info: &DocInfo,
    out: &mut String,
    footnotes: &mut Vec<(u16, String)>,
) {
    // Handle inline controls that replace the paragraph text
    // Check for page/section/column breaks
    for ctrl in &para.controls {
        match ctrl {
            Control::PageBreak | Control::SectionBreak => {
                out.push_str("\n---\n\n");
            }
            Control::ColumnBreak => {
                out.push_str("\n---\n\n");
            }
            _ => {}
        }
    }

    // Handle table controls
    for ctrl in &para.controls {
        if let Control::Table(table) = ctrl {
            convert_table(table, doc_info, out, footnotes);
            return;
        }
    }

    // Get para shape for heading info
    let para_shape = doc_info
        .para_shapes
        .get(para.para_shape_id as usize);

    let heading_type = para_shape.map(|ps| ps.heading_type).unwrap_or(0);
    let heading_level = para_shape.map(|ps| ps.heading_level).unwrap_or(0);

    // Strip HWP control characters (0x00–0x1F) from text, except common whitespace
    let clean_text = strip_hwp_controls(&para.text);

    if clean_text.trim().is_empty() {
        // Process any remaining inline controls even for "empty" paragraphs
        for ctrl in &para.controls {
            match ctrl {
                Control::Picture(shape) => {
                    convert_picture(shape, out);
                }
                Control::Equation(eq) => {
                    out.push_str(&format!("${}$\n\n", eq.script));
                }
                _ => {}
            }
        }
        if clean_text.is_empty() && para.controls.is_empty() {
            out.push('\n');
        }
        return;
    }

    // Build styled text by walking char_runs
    let styled = build_styled_text(&clean_text, &para.char_runs, doc_info);

    // Embed inline controls (pictures, equations, fields, footnotes)
    let mut final_text = styled;
    for ctrl in &para.controls {
        match ctrl {
            Control::Picture(shape) => {
                let img = picture_markdown(shape);
                final_text.push_str(&img);
            }
            Control::Equation(eq) => {
                final_text.push_str(&format!(" ${}$", eq.script));
            }
            Control::Field(field) => match field {
                Field::Hyperlink { url, text } => {
                    // If the link text appears in final_text, wrap it
                    if !text.is_empty() && final_text.contains(text.as_str()) {
                        final_text =
                            final_text.replace(text.as_str(), &format!("[{}]({})", text, url));
                    } else {
                        final_text.push_str(&format!(" [{}]({})", if text.is_empty() { url } else { text }, url));
                    }
                }
                _ => {}
            },
            Control::FootnoteEndnote(fne) => {
                let note_text = fne
                    .paragraphs
                    .iter()
                    .map(|p| strip_hwp_controls(&p.text))
                    .collect::<Vec<_>>()
                    .join(" ");
                final_text.push_str(&format!("[^{}]", fne.number));
                footnotes.push((fne.number, note_text));
            }
            _ => {}
        }
    }

    // Apply heading prefix
    match heading_type {
        1 => {
            // Outline heading
            let level = (heading_level as usize + 1).min(6);
            let prefix: String = "#".repeat(level);
            out.push_str(&format!("{} {}\n\n", prefix, final_text.trim()));
        }
        2 => {
            // Numbered list
            out.push_str(&format!("1. {}\n", final_text.trim()));
        }
        3 => {
            // Bullet list
            out.push_str(&format!("- {}\n", final_text.trim()));
        }
        _ => {
            // Normal paragraph
            out.push_str(&format!("{}\n\n", final_text.trim()));
        }
    }
}

fn strip_hwp_controls(text: &str) -> String {
    text.chars()
        .filter(|&c| {
            // Keep normal whitespace, filter HWP control chars
            c >= ' ' || c == '\t' || c == '\n' || c == '\r'
        })
        .collect()
}

fn build_styled_text(text: &str, char_runs: &[crate::model::CharRun], doc_info: &DocInfo) -> String {
    if char_runs.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let chars: Vec<char> = text.chars().collect();
    let mut result = String::new();

    for (ri, run) in char_runs.iter().enumerate() {
        let start = run.start_pos as usize;
        let end = if ri + 1 < char_runs.len() {
            (char_runs[ri + 1].start_pos as usize).min(chars.len())
        } else {
            chars.len()
        };

        if start >= chars.len() {
            break;
        }
        let end = end.min(chars.len());
        if start >= end {
            continue;
        }

        let segment: String = chars[start..end].iter().collect();
        if segment.trim().is_empty() {
            result.push_str(&segment);
            continue;
        }

        let cs = doc_info.char_shapes.get(run.char_shape_id as usize);
        match cs {
            Some(cs) => {
                let trimmed = segment.trim();
                let leading: String = segment.chars().take_while(|c| c.is_whitespace()).collect();
                let trailing: String = segment.chars().rev().take_while(|c| c.is_whitespace()).collect::<String>().chars().rev().collect();

                let mut styled = trimmed.to_string();

                // Apply strikethrough
                if !matches!(cs.strikeout, StrikeoutType::None) {
                    styled = format!("~~{}~~", styled);
                }

                // Apply bold + italic
                match (cs.bold, cs.italic) {
                    (true, true) => styled = format!("***{}***", styled),
                    (true, false) => styled = format!("**{}**", styled),
                    (false, true) => styled = format!("*{}*", styled),
                    _ => {}
                }

                // Apply underline (HTML tag)
                if !matches!(cs.underline, UnderlineType::None) {
                    styled = format!("<u>{}</u>", styled);
                }

                // Apply superscript/subscript
                if cs.superscript {
                    styled = format!("<sup>{}</sup>", styled);
                }
                if cs.subscript {
                    styled = format!("<sub>{}</sub>", styled);
                }

                result.push_str(&leading);
                result.push_str(&styled);
                result.push_str(&trailing);
            }
            None => {
                result.push_str(&segment);
            }
        }
    }

    result
}

fn convert_table(
    table: &Table,
    doc_info: &DocInfo,
    out: &mut String,
    footnotes: &mut Vec<(u16, String)>,
) {
    // Check for merged cells
    let has_merges = table.cells.iter().any(|c| c.col_span > 1 || c.row_span > 1);

    if has_merges {
        convert_table_html(table, doc_info, out, footnotes);
    } else {
        convert_table_gfm(table, doc_info, out, footnotes);
    }
}

fn convert_table_gfm(
    table: &Table,
    doc_info: &DocInfo,
    out: &mut String,
    _footnotes: &mut Vec<(u16, String)>,
) {
    let rows = table.rows as usize;
    let cols = table.cols as usize;

    // Build grid
    let mut grid: Vec<Vec<String>> = vec![vec![String::new(); cols]; rows];
    for cell in &table.cells {
        let r = cell.row_addr as usize;
        let c = cell.col_addr as usize;
        if r < rows && c < cols {
            let cell_text: Vec<String> = cell
                .paragraphs
                .iter()
                .map(|p| {
                    let clean = strip_hwp_controls(&p.text);
                    build_styled_text(&clean, &p.char_runs, doc_info)
                })
                .collect();
            grid[r][c] = cell_text.join(" ").trim().to_string();
        }
    }

    out.push('\n');
    for (ri, row) in grid.iter().enumerate() {
        out.push('|');
        for cell in row {
            out.push_str(&format!(" {} |", cell));
        }
        out.push('\n');

        // Separator after header row
        if ri == 0 {
            out.push('|');
            for _ in 0..cols {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out.push('\n');
}

fn convert_table_html(
    table: &Table,
    doc_info: &DocInfo,
    out: &mut String,
    _footnotes: &mut Vec<(u16, String)>,
) {
    out.push_str("\n<table>\n");

    let rows = table.rows as usize;
    let cols = table.cols as usize;

    // Track which cells are covered by spans
    let mut covered: Vec<Vec<bool>> = vec![vec![false; cols]; rows];

    for r in 0..rows {
        out.push_str("  <tr>\n");
        for c in 0..cols {
            if covered[r][c] {
                continue;
            }
            // Find the cell at this position
            if let Some(cell) = table
                .cells
                .iter()
                .find(|cell| cell.row_addr as usize == r && cell.col_addr as usize == c)
            {
                let cell_text: Vec<String> = cell
                    .paragraphs
                    .iter()
                    .map(|p| {
                        let clean = strip_hwp_controls(&p.text);
                        build_styled_text(&clean, &p.char_runs, doc_info)
                    })
                    .collect();
                let content = cell_text.join("<br>").trim().to_string();

                let tag = if r == 0 { "th" } else { "td" };
                let mut attrs = String::new();
                if cell.col_span > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", cell.col_span));
                }
                if cell.row_span > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
                }

                out.push_str(&format!("    <{}{}>{}</{}>\n", tag, attrs, content, tag));

                // Mark covered cells
                for dr in 0..cell.row_span as usize {
                    for dc in 0..cell.col_span as usize {
                        if dr == 0 && dc == 0 {
                            continue;
                        }
                        if r + dr < rows && c + dc < cols {
                            covered[r + dr][c + dc] = true;
                        }
                    }
                }
            }
        }
        out.push_str("  </tr>\n");
    }

    out.push_str("</table>\n\n");
}

fn convert_picture(shape: &ShapeObject, out: &mut String) {
    let img = picture_markdown(shape);
    if !img.is_empty() {
        out.push_str(&img);
        out.push('\n');
    }
}

fn picture_markdown(shape: &ShapeObject) -> String {
    match shape {
        ShapeObject::Picture { bin_data_id, .. } => {
            format!("![image](/api/image/{})", bin_data_id)
        }
        ShapeObject::Container { children } => {
            children
                .iter()
                .map(|c| picture_markdown(c))
                .collect::<Vec<_>>()
                .join("")
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use crate::model::charshape::*;
    use crate::model::parashape::*;

    fn minimal_doc() -> HwpDocument {
        HwpDocument {
            header: FileHeader {
                version: (5, 0, 0, 0),
                compressed: false,
                encrypted: false,
                distributed: false,
                script: false,
                drm: false,
                xml_template: false,
                history: false,
                cert_signed: false,
                cert_encrypted: false,
                cert_drm: false,
                ccl: false,
            },
            doc_info: DocInfo {
                char_shapes: vec![CharShape {
                    font_ids: [0; 7],
                    font_ratios: [100; 7],
                    font_spacings: [0; 7],
                    font_rel_sizes: [100; 7],
                    font_offsets: [0; 7],
                    height: 1000,
                    text_color: 0,
                    shade_color: 0xFFFFFF,
                    shadow_color: 0,
                    bold: false,
                    italic: false,
                    underline: UnderlineType::None,
                    underline_color: 0,
                    strikeout: StrikeoutType::None,
                    outline: OutlineType::None,
                    shadow_type: ShadowType::None,
                    emboss: false,
                    engrave: false,
                    superscript: false,
                    subscript: false,
                    border_fill_id: 0,
                }],
                para_shapes: vec![ParaShape {
                    align: Alignment::Left,
                    line_spacing: 160,
                    line_spacing_type: SpacingType::Percent,
                    space_before: 0,
                    space_after: 0,
                    margin_left: 0,
                    margin_right: 0,
                    indent: 0,
                    tab_def_id: 0,
                    heading_type: 0,
                    heading_level: 0,
                    border_fill_id: 0,
                    page_break_before: false,
                    keep_with_next: false,
                    keep_together: false,
                    widow_orphan: false,
                }],
                border_fills: vec![],
                styles: vec![],
                fonts: vec![],
                tab_defs: vec![],
                numberings: vec![],
                bullets: vec![],
                bin_data_defs: vec![],
            },
            sections: vec![Section {
                sec_def: crate::model::SecDef {
                    page_def: crate::model::PageDef {
                        width: 59528,
                        height: 84188,
                        margin_left: 8504,
                        margin_right: 8504,
                        margin_top: 5668,
                        margin_bottom: 4252,
                        margin_header: 4252,
                        margin_footer: 4252,
                        margin_gutter: 0,
                        landscape: false,
                    },
                    columns: 1,
                    column_spacing: 0,
                    page_start_number: 1,
                },
                paragraphs: vec![Paragraph {
                    text: "Hello world".to_string(),
                    char_runs: vec![CharRun {
                        start_pos: 0,
                        char_shape_id: 0,
                    }],
                    para_shape_id: 0,
                    style_id: 0,
                    controls: vec![],
                    char_count: 11,
                }],
            }],
            bin_data: vec![],
        }
    }

    #[test]
    fn test_basic_paragraph() {
        let doc = minimal_doc();
        let md = document_to_markdown(&doc);
        assert!(md.contains("Hello world"));
    }

    #[test]
    fn test_heading() {
        let mut doc = minimal_doc();
        doc.doc_info.para_shapes[0].heading_type = 1;
        doc.doc_info.para_shapes[0].heading_level = 0;
        let md = document_to_markdown(&doc);
        assert!(md.starts_with("# Hello world"));
    }

    #[test]
    fn test_bold_text() {
        let mut doc = minimal_doc();
        doc.doc_info.char_shapes[0].bold = true;
        let md = document_to_markdown(&doc);
        assert!(md.contains("**Hello world**"));
    }
}
