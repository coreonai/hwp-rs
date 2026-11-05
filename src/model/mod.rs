pub mod charshape;
pub mod control;
pub mod parashape;
pub mod shape;
pub mod style;
pub mod table;

pub use charshape::CharShape;
pub use control::Control;
pub use parashape::ParaShape;
pub use shape::ShapeObject;
pub use style::Style;
pub use table::{Cell, Table};

use serde::Serialize;

/// Top-level HWP document.
#[derive(Debug, Clone, Default, Serialize)]
pub struct HwpDocument {
    pub header: FileHeader,
    pub doc_info: DocInfo,
    pub sections: Vec<Section>,
    pub bin_data: Vec<BinEntry>,
}

/// File header metadata.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FileHeader {
    /// Major.minor.build.revision
    pub version: (u8, u8, u8, u8),
    pub compressed: bool,
    pub encrypted: bool,
    pub distributed: bool,
    pub script: bool,
    pub drm: bool,
    pub xml_template: bool,
    pub history: bool,
    pub cert_signed: bool,
    pub cert_encrypted: bool,
    pub cert_drm: bool,
    pub ccl: bool,
}

/// Document-wide information (shape tables, fonts, styles).
#[derive(Debug, Clone, Default, Serialize)]
pub struct DocInfo {
    pub char_shapes: Vec<CharShape>,
    pub para_shapes: Vec<ParaShape>,
    pub border_fills: Vec<BorderFill>,
    pub styles: Vec<Style>,
    /// Fonts indexed by script type: Korean=0, English=1, Hanja=2, Japanese=3, Other=4, Symbol=5, User=6
    pub fonts: Vec<Vec<FontFace>>,
    pub tab_defs: Vec<TabDef>,
    pub numberings: Vec<Numbering>,
    pub bullets: Vec<Bullet>,
    pub bin_data_defs: Vec<BinDataDef>,
}

/// A single section of the document.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Section {
    pub sec_def: SecDef,
    pub paragraphs: Vec<Paragraph>,
}

/// Section definition (page layout).
#[derive(Debug, Clone, Default, Serialize)]
pub struct SecDef {
    pub page_def: PageDef,
    pub columns: u16,
    pub column_spacing: i32,
    pub page_start_number: u16,
}

/// Page dimensions and margins (in hwp units = 1/7200 inch).
#[derive(Debug, Clone, Serialize)]
pub struct PageDef {
    pub width: u32,
    pub height: u32,
    pub margin_left: u32,
    pub margin_right: u32,
    pub margin_top: u32,
    pub margin_bottom: u32,
    pub margin_header: u32,
    pub margin_footer: u32,
    pub margin_gutter: u32,
    pub landscape: bool,
}

impl Default for PageDef {
    fn default() -> Self {
        Self {
            width: 59528,  // A4 width
            height: 84188, // A4 height
            margin_left: 8504,
            margin_right: 8504,
            margin_top: 5668,
            margin_bottom: 4252,
            margin_header: 4252,
            margin_footer: 4252,
            margin_gutter: 0,
            landscape: false,
        }
    }
}

/// A paragraph within a section.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Paragraph {
    /// Plain text content (special chars replaced with markers).
    pub text: String,
    /// Character shape runs.
    pub char_runs: Vec<CharRun>,
    /// Index into DocInfo.para_shapes.
    pub para_shape_id: u32,
    /// Index into DocInfo.styles.
    pub style_id: u16,
    /// Inline controls (tables, pictures, etc.).
    pub controls: Vec<Control>,
    /// Number of character position units in the text.
    pub char_count: u32,
}

/// A run of characters sharing the same character shape.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CharRun {
    /// Starting character position in the paragraph.
    pub start_pos: u32,
    /// Index into DocInfo.char_shapes.
    pub char_shape_id: u32,
}

/// Font face entry.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FontFace {
    pub name: String,
    pub family: u8,
    pub alt_name: Option<String>,
}

/// Border/fill style.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BorderFill {
    pub id: u16,
    pub border_type_left: u8,
    pub border_type_right: u8,
    pub border_type_top: u8,
    pub border_type_bottom: u8,
    pub border_width_left: u8,
    pub border_width_right: u8,
    pub border_width_top: u8,
    pub border_width_bottom: u8,
    pub border_color_left: u32,
    pub border_color_right: u32,
    pub border_color_top: u32,
    pub border_color_bottom: u32,
    pub fill_color: Option<u32>,
    pub fill_pattern_type: u8,
}

/// Tab definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TabDef {
    pub auto_tab_left: bool,
    pub auto_tab_right: bool,
    pub tab_items: Vec<TabItem>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TabItem {
    pub position: u32,
    pub kind: u8,
    pub leader: u8,
}

/// Numbering definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Numbering {
    pub levels: Vec<NumberingLevel>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct NumberingLevel {
    pub format: String,
    pub number_type: u8,
    pub align: u8,
}

/// Bullet definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Bullet {
    pub char: char,
    pub image_bullet: bool,
}

/// Binary data descriptor.
#[derive(Debug, Clone, Default, Serialize)]
pub struct BinDataDef {
    pub storage_type: u16,
    pub abs_path: Option<String>,
    pub rel_path: Option<String>,
    pub bin_data_id: u16,
    pub extension: String,
}

/// Actual binary data entry (image bytes etc.).
#[derive(Debug, Clone, Serialize)]
pub struct BinEntry {
    pub name: String,
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
}
