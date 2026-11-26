//! EditorDoc — JSON interchange format between the browser editor and Rust backend.
//!
//! Provides bidirectional conversion between [`HwpDocument`] and [`EditorDoc`].

use serde::{Deserialize, Serialize};

use crate::model::charshape::{CharShape, StrikeoutType, UnderlineType};
use crate::model::control::Control;
use crate::model::parashape::{Alignment, ParaShape};
use crate::model::shape::ShapeObject;
use crate::model::table::{Cell, Table};
use crate::model::{CharRun, DocInfo, HwpDocument, Paragraph};

// ── JSON types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorDoc {
    pub title: String,
    pub page_def: EditorPageDef,
    pub char_shapes: Vec<EditorCharShape>,
    pub para_shapes: Vec<EditorParaShape>,
    pub sections: Vec<EditorSection>,
    pub images: Vec<EditorImage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorPageDef {
    pub width: u32,
    pub height: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorCharShape {
    pub id: u32,
    pub font_name: String,
    pub height_pt: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub text_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorParaShape {
    pub id: u32,
    pub align: String,
    pub heading_type: u8,
    pub heading_level: u8,
    pub line_spacing: i32,
    pub margin_left: i32,
    pub margin_right: i32,
    pub indent: i32,
    pub space_before: i32,
    pub space_after: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSection {
    pub paragraphs: Vec<EditorParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorParagraph {
    pub para_shape_id: u32,
    pub style_id: u16,
    pub runs: Vec<EditorRun>,
    pub controls: Vec<EditorControl>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorRun {
    pub text: String,
    pub char_shape_id: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EditorControl {
    #[serde(rename = "table")]
    Table {
        rows: u16,
        cols: u16,
        cells: Vec<EditorTableCell>,
    },
    #[serde(rename = "picture")]
    Picture {
        bin_data_id: u16,
        width: u32,
        height: u32,
    },
    #[serde(rename = "equation")]
    Equation { script: String },
    #[serde(rename = "page_break")]
    PageBreak,
    #[serde(rename = "hyperlink")]
    Hyperlink { url: String, text: String },
    #[serde(rename = "footnote")]
    Footnote {
        is_endnote: bool,
        number: u16,
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorTableCell {
    pub col: u16,
    pub row: u16,
    pub col_span: u16,
    pub row_span: u16,
    pub width: u32,
    pub height: u32,
    pub paragraphs: Vec<EditorParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorImage {
    pub id: u16,
    pub url: String,
    pub name: String,
}

// ── HwpDocument → EditorDoc ─────────────────────────────────────────

/// Convert an [`HwpDocument`] into an [`EditorDoc`] for the browser editor.
pub fn hwp_doc_to_editor_doc(doc: &HwpDocument, title: &str) -> EditorDoc {
    let di = &doc.doc_info;
    let page_def = doc
        .sections
        .first()
        .map(|s| &s.sec_def.page_def)
        .cloned()
        .unwrap_or_default();

    EditorDoc {
        title: title.to_string(),
        page_def: EditorPageDef {
            width: page_def.width,
            height: page_def.height,
            margin_left: page_def.margin_left,
            margin_right: page_def.margin_right,
            margin_top: page_def.margin_top,
            margin_bottom: page_def.margin_bottom,
        },
        char_shapes: di
            .char_shapes
            .iter()
            .enumerate()
            .map(|(i, cs)| convert_char_shape(i as u32, cs, di))
            .collect(),
        para_shapes: di
            .para_shapes
            .iter()
            .enumerate()
            .map(|(i, ps)| convert_para_shape(i as u32, ps))
            .collect(),
        sections: doc
            .sections
            .iter()
            .map(|sec| EditorSection {
                paragraphs: sec
                    .paragraphs
                    .iter()
                    .map(|p| convert_paragraph(p, di))
                    .collect(),
            })
            .collect(),
        images: doc
            .bin_data
            .iter()
            .enumerate()
            .map(|(i, entry)| EditorImage {
                id: (i + 1) as u16,
                url: format!("/api/image/{}", i + 1),
                name: entry.name.clone(),
            })
            .collect(),
    }
}

fn convert_char_shape(id: u32, cs: &CharShape, di: &DocInfo) -> EditorCharShape {
    // Resolve font name from Korean font group (index 0)
    let font_name = di
        .fonts
        .first()
        .and_then(|group| group.get(cs.font_ids[0] as usize))
        .map(|f| f.name.clone())
        .unwrap_or_default();

    EditorCharShape {
        id,
        font_name,
        height_pt: cs.height as f32 / 100.0,
        bold: cs.bold,
        italic: cs.italic,
        underline: !matches!(cs.underline, UnderlineType::None),
        strikeout: !matches!(cs.strikeout, StrikeoutType::None),
        superscript: cs.superscript,
        subscript: cs.subscript,
        text_color: format!("#{:06X}", cs.text_color),
    }
}

fn convert_para_shape(id: u32, ps: &ParaShape) -> EditorParaShape {
    EditorParaShape {
        id,
        align: match ps.align {
            Alignment::Left => "left",
            Alignment::Right => "right",
            Alignment::Center => "center",
            Alignment::Justify => "justify",
            Alignment::Distribute | Alignment::DistributeSpace => "justify",
            Alignment::Other(_) => "left",
        }
        .to_string(),
        heading_type: ps.heading_type,
        heading_level: ps.heading_level,
        line_spacing: ps.line_spacing,
        margin_left: ps.margin_left,
        margin_right: ps.margin_right,
        indent: ps.indent,
        space_before: ps.space_before,
        space_after: ps.space_after,
    }
}

fn convert_paragraph(para: &Paragraph, di: &DocInfo) -> EditorParagraph {
    let runs = split_runs(para, di);
    let controls = para
        .controls
        .iter()
        .filter_map(|c| convert_control(c, di))
        .collect();

    EditorParagraph {
        para_shape_id: para.para_shape_id,
        style_id: para.style_id,
        runs,
        controls,
    }
}

/// Split paragraph text into runs according to char_runs boundaries.
fn split_runs(para: &Paragraph, _di: &DocInfo) -> Vec<EditorRun> {
    let chars: Vec<char> = strip_hwp_controls(&para.text).chars().collect();
    if chars.is_empty() {
        return vec![];
    }
    if para.char_runs.is_empty() {
        return vec![EditorRun {
            text: chars.iter().collect(),
            char_shape_id: 0,
        }];
    }

    let mut runs = Vec::new();
    for (ri, run) in para.char_runs.iter().enumerate() {
        let start = run.start_pos as usize;
        let end = if ri + 1 < para.char_runs.len() {
            (para.char_runs[ri + 1].start_pos as usize).min(chars.len())
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
        let text: String = chars[start..end].iter().collect();
        if !text.is_empty() {
            runs.push(EditorRun {
                text,
                char_shape_id: run.char_shape_id,
            });
        }
    }
    runs
}

fn strip_hwp_controls(text: &str) -> String {
    text.chars()
        .filter(|&c| c >= ' ' || c == '\t' || c == '\n' || c == '\r')
        .collect()
}

fn convert_control(ctrl: &Control, di: &DocInfo) -> Option<EditorControl> {
    match ctrl {
        Control::Table(table) => Some(EditorControl::Table {
            rows: table.rows,
            cols: table.cols,
            cells: table
                .cells
                .iter()
                .map(|c| convert_cell(c, di))
                .collect(),
        }),
        Control::Picture(shape) => picture_info(shape).map(|(id, w, h)| EditorControl::Picture {
            bin_data_id: id,
            width: w,
            height: h,
        }),
        Control::Equation(eq) => Some(EditorControl::Equation {
            script: eq.script.clone(),
        }),
        Control::PageBreak | Control::SectionBreak => Some(EditorControl::PageBreak),
        Control::Field(crate::model::control::Field::Hyperlink { url, text }) => {
            Some(EditorControl::Hyperlink {
                url: url.clone(),
                text: text.clone(),
            })
        }
        Control::FootnoteEndnote(fn_en) => {
            let text = fn_en
                .paragraphs
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            Some(EditorControl::Footnote {
                is_endnote: fn_en.is_endnote,
                number: fn_en.number,
                text,
            })
        }
        _ => None,
    }
}

fn convert_cell(cell: &Cell, di: &DocInfo) -> EditorTableCell {
    EditorTableCell {
        col: cell.col_addr,
        row: cell.row_addr,
        col_span: cell.col_span,
        row_span: cell.row_span,
        width: cell.width,
        height: cell.height,
        paragraphs: cell
            .paragraphs
            .iter()
            .map(|p| convert_paragraph(p, di))
            .collect(),
    }
}

fn picture_info(shape: &ShapeObject) -> Option<(u16, u32, u32)> {
    match shape {
        ShapeObject::Picture {
            bin_data_id,
            width,
            height,
            ..
        } => Some((*bin_data_id, *width, *height)),
        ShapeObject::Container { children } => children.iter().find_map(picture_info),
        _ => None,
    }
}

// ── EditorDoc → HwpDocument ─────────────────────────────────────────

/// Merge edited [`EditorDoc`] back into the original [`HwpDocument`].
///
/// Clones the original to preserve metadata not exposed in the editor
/// (border_fills, tab_defs, numberings, etc.), then overwrites paragraphs
/// and shape tables from the editor output.
pub fn editor_doc_to_hwp_doc(edoc: &EditorDoc, original: &HwpDocument) -> HwpDocument {
    let mut doc = original.clone();

    // Rebuild paragraphs from editor sections
    for (si, esec) in edoc.sections.iter().enumerate() {
        if si >= doc.sections.len() {
            break;
        }
        doc.sections[si].paragraphs = esec
            .paragraphs
            .iter()
            .map(|ep| rebuild_paragraph(ep))
            .collect();
    }

    doc
}

fn rebuild_paragraph(ep: &EditorParagraph) -> Paragraph {
    let mut text = String::new();
    let mut char_runs = Vec::new();

    for run in &ep.runs {
        let start = text.chars().count() as u32;
        char_runs.push(CharRun {
            start_pos: start,
            char_shape_id: run.char_shape_id,
        });
        text.push_str(&run.text);
    }

    let controls: Vec<Control> = ep
        .controls
        .iter()
        .filter_map(|ec| rebuild_control(ec))
        .collect();

    Paragraph {
        text,
        char_runs,
        para_shape_id: ep.para_shape_id,
        style_id: ep.style_id,
        controls,
        char_count: 0,
    }
}

fn rebuild_control(ec: &EditorControl) -> Option<Control> {
    match ec {
        EditorControl::Table { rows, cols, cells } => {
            let table = Table {
                rows: *rows,
                cols: *cols,
                border_fill_id: 0,
                cells: cells.iter().map(rebuild_cell).collect(),
                cell_spacing: 0,
                padding: [0; 4],
            };
            Some(Control::Table(table))
        }
        EditorControl::Picture {
            bin_data_id,
            width,
            height,
        } => Some(Control::Picture(ShapeObject::Picture {
            bin_data_id: *bin_data_id,
            x: 0,
            y: 0,
            width: *width,
            height: *height,
            orig_width: *width,
            orig_height: *height,
            crop: [0; 4],
        })),
        EditorControl::PageBreak => Some(Control::PageBreak),
        EditorControl::Equation { script } => {
            Some(Control::Equation(crate::model::control::Equation {
                script: script.clone(),
                font_name: String::new(),
                font_size: 1000,
                color: 0,
            }))
        }
        EditorControl::Hyperlink { url, text } => {
            Some(Control::Field(crate::model::control::Field::Hyperlink {
                url: url.clone(),
                text: text.clone(),
            }))
        }
        EditorControl::Footnote {
            is_endnote,
            number,
            text,
        } => {
            let paragraphs = if text.is_empty() {
                vec![]
            } else {
                text.split('\n')
                    .map(|line| {
                        let mut p = Paragraph::default();
                        p.text = line.to_string();
                        p
                    })
                    .collect()
            };
            Some(Control::FootnoteEndnote(
                crate::model::control::FootnoteEndnote {
                    is_endnote: *is_endnote,
                    number: *number,
                    paragraphs,
                },
            ))
        }
    }
}

fn rebuild_cell(ec: &EditorTableCell) -> Cell {
    Cell {
        col_addr: ec.col,
        row_addr: ec.row,
        col_span: ec.col_span,
        row_span: ec.row_span,
        width: ec.width,
        height: ec.height,
        padding: [0; 4],
        border_fill_id: 0,
        paragraphs: ec.paragraphs.iter().map(rebuild_paragraph).collect(),
    }
}
