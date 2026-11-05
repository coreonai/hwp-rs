use super::Paragraph;

/// Table control.
#[derive(Debug, Clone, Default)]
pub struct Table {
    pub rows: u16,
    pub cols: u16,
    pub border_fill_id: u16,
    pub cells: Vec<Cell>,
    /// Cell spacing in HWP units.
    pub cell_spacing: u16,
    /// Left/right/top/bottom padding.
    pub padding: [u16; 4],
}

/// A single cell in a table.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    /// Column address (0-based).
    pub col_addr: u16,
    /// Row address (0-based).
    pub row_addr: u16,
    /// Column span.
    pub col_span: u16,
    /// Row span.
    pub row_span: u16,
    /// Cell width in HWP units.
    pub width: u32,
    /// Cell height in HWP units.
    pub height: u32,
    /// Cell padding (left, right, top, bottom).
    pub padding: [u16; 4],
    /// Border fill ID.
    pub border_fill_id: u16,
    /// Paragraphs within the cell.
    pub paragraphs: Vec<Paragraph>,
}
