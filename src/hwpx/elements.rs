use quick_xml::events::BytesStart;

use crate::model::charshape::{CharShape, UnderlineType, StrikeoutType};
use crate::model::parashape::{Alignment, ParaShape, SpacingType};
use crate::model::style::Style;
use crate::model::table::{Cell, Table};
use crate::model::{
    FontFace, PageDef, SecDef,
};

/// Helper to get an attribute value as a string.
pub fn attr_str(e: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .map(|a| String::from_utf8_lossy(&a.value).to_string())
}

/// Helper to get an attribute value as a parsed type.
pub fn attr_parse<T: std::str::FromStr>(e: &BytesStart<'_>, name: &[u8]) -> Option<T> {
    attr_str(e, name).and_then(|s| s.parse().ok())
}

/// Parse a `<CHARSHAPE>` element.
pub fn parse_charshape_element(e: &BytesStart<'_>) -> CharShape {
    let mut cs = CharShape::default();

    if let Some(h) = attr_parse::<u32>(e, b"Height") {
        cs.height = h;
    }
    if let Some(c) = attr_parse::<u32>(e, b"TextColor") {
        cs.text_color = c & 0x00FFFFFF;
    }
    if let Some(c) = attr_parse::<u32>(e, b"ShadeColor") {
        cs.shade_color = c & 0x00FFFFFF;
    }

    cs.bold = attr_str(e, b"Bold").map_or(false, |v| v == "1" || v == "true");
    cs.italic = attr_str(e, b"Italic").map_or(false, |v| v == "1" || v == "true");

    if let Some(u) = attr_parse::<u8>(e, b"UnderlineType") {
        cs.underline = UnderlineType::from(u);
    }
    if let Some(s) = attr_parse::<u8>(e, b"StrikeoutType") {
        cs.strikeout = StrikeoutType::from(s);
    }

    cs
}

/// Parse `<FONTFACE>` children (`<FONT>` elements).
pub fn parse_font_element(e: &BytesStart<'_>) -> FontFace {
    let mut face = FontFace::default();
    if let Some(name) = attr_str(e, b"Face") {
        face.name = name;
    }
    if let Some(f) = attr_parse::<u8>(e, b"Family") {
        face.family = f;
    }
    face
}

/// Parse a `<PARASHAPE>` element.
pub fn parse_parashape_element(e: &BytesStart<'_>) -> ParaShape {
    let mut ps = ParaShape::default();

    if let Some(a) = attr_str(e, b"Align") {
        ps.align = match a.as_str() {
            "Justify" => Alignment::Justify,
            "Left" => Alignment::Left,
            "Right" => Alignment::Right,
            "Center" => Alignment::Center,
            "Distribute" => Alignment::Distribute,
            _ => Alignment::Justify,
        };
    }
    if let Some(v) = attr_parse::<i32>(e, b"LineSpacing") {
        ps.line_spacing = v;
    }
    if let Some(v) = attr_str(e, b"LineSpacingType") {
        ps.line_spacing_type = match v.as_str() {
            "Percent" => SpacingType::Percent,
            "Fixed" => SpacingType::Fixed,
            "BetweenLines" => SpacingType::BetweenLines,
            "AtLeast" => SpacingType::AtLeast,
            _ => SpacingType::Percent,
        };
    }
    if let Some(v) = attr_parse::<i32>(e, b"SpaceBefore") {
        ps.space_before = v;
    }
    if let Some(v) = attr_parse::<i32>(e, b"SpaceAfter") {
        ps.space_after = v;
    }
    if let Some(v) = attr_parse::<i32>(e, b"MarginLeft") {
        ps.margin_left = v;
    }
    if let Some(v) = attr_parse::<i32>(e, b"MarginRight") {
        ps.margin_right = v;
    }
    if let Some(v) = attr_parse::<i32>(e, b"Indent") {
        ps.indent = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"TabDefId") {
        ps.tab_def_id = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"BorderFillId") {
        ps.border_fill_id = v;
    }

    ps
}

/// Parse a `<STYLE>` element.
pub fn parse_style_element(e: &BytesStart<'_>) -> Style {
    let mut style = Style::default();
    if let Some(n) = attr_str(e, b"Name") {
        style.name = n;
    }
    if let Some(n) = attr_str(e, b"EngName") {
        style.english_name = n;
    }
    if let Some(t) = attr_parse::<u8>(e, b"Type") {
        style.style_type = t;
    }
    if let Some(v) = attr_parse::<u16>(e, b"NextStyleId") {
        style.next_style_id = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"CharShapeId") {
        style.char_shape_id = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"ParaShapeId") {
        style.para_shape_id = v;
    }
    style
}

/// Parse `<SECDEF>` element.
pub fn parse_secdef_element(e: &BytesStart<'_>) -> SecDef {
    let mut sd = SecDef::default();
    if let Some(v) = attr_parse::<u16>(e, b"Columns") {
        sd.columns = v;
    }
    sd
}

/// Parse `<PAGEDEF>` element.
pub fn parse_pagedef_element(e: &BytesStart<'_>) -> PageDef {
    let mut pd = PageDef::default();
    if let Some(v) = attr_parse::<u32>(e, b"Width") {
        pd.width = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Height") {
        pd.height = v;
    }
    if let Some(v) = attr_str(e, b"Landscape") {
        pd.landscape = v == "1" || v == "true";
    }
    pd
}

/// Parse `<PAGEMARGIN>` element (updates a PageDef).
pub fn apply_page_margin(pd: &mut PageDef, e: &BytesStart<'_>) {
    if let Some(v) = attr_parse::<u32>(e, b"Left") {
        pd.margin_left = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Right") {
        pd.margin_right = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Top") {
        pd.margin_top = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Bottom") {
        pd.margin_bottom = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Header") {
        pd.margin_header = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Footer") {
        pd.margin_footer = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Gutter") {
        pd.margin_gutter = v;
    }
}

/// Parse a `<TABLE>` element.
pub fn parse_table_element(e: &BytesStart<'_>) -> Table {
    let mut table = Table::default();
    if let Some(v) = attr_parse::<u16>(e, b"Rows") {
        table.rows = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"Cols") {
        table.cols = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"CellSpacing") {
        table.cell_spacing = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"BorderFillId") {
        table.border_fill_id = v;
    }
    table
}

/// Parse a `<CELL>` element.
pub fn parse_cell_element(e: &BytesStart<'_>) -> Cell {
    let mut cell = Cell::default();
    if let Some(v) = attr_parse::<u16>(e, b"ColAddr") {
        cell.col_addr = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"RowAddr") {
        cell.row_addr = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"ColSpan") {
        cell.col_span = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"RowSpan") {
        cell.row_span = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Width") {
        cell.width = v;
    }
    if let Some(v) = attr_parse::<u32>(e, b"Height") {
        cell.height = v;
    }
    if let Some(v) = attr_parse::<u16>(e, b"BorderFillId") {
        cell.border_fill_id = v;
    }
    cell
}
