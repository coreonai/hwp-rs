use serde::Serialize;

use super::shape::ShapeObject;
use super::table::Table;
use super::Paragraph;

/// Inline control embedded in a paragraph.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Table(Table),
    Picture(ShapeObject),
    Equation(Equation),
    Field(Field),
    Bookmark(Bookmark),
    FootnoteEndnote(FootnoteEndnote),
    AutoNumber(AutoNumber),
    PageBreak,
    ColumnBreak,
    SectionBreak,
    /// Generic/unknown control with its ctrl ID.
    Unknown {
        ctrl_id: u32,
        #[serde(skip_serializing)]
        data: Vec<u8>,
    },
}

/// Equation (OLE-embedded LaTeX-like formula).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Equation {
    pub script: String,
    pub font_name: String,
    pub font_size: u32,
    pub color: u32,
}

/// Field control (text form field, hyperlink, date, etc.).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Hyperlink {
        url: String,
        text: String,
    },
    TextFormField {
        name: String,
        default_value: String,
    },
    Date {
        format: String,
    },
    PageNumber,
    PageCount,
    FileName,
    CrossRef {
        target: String,
    },
    Other {
        field_type: u8,
        command: String,
    },
}

/// Bookmark.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Bookmark {
    pub name: String,
}

/// Footnote or endnote.
#[derive(Debug, Clone, Serialize)]
pub struct FootnoteEndnote {
    pub is_endnote: bool,
    pub number: u16,
    pub paragraphs: Vec<Paragraph>,
}

/// Auto-numbering.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutoNumber {
    pub num_type: u8,
    pub number: u16,
}
