use serde::Serialize;

/// Paragraph shape — alignment, spacing, indentation.
#[derive(Debug, Clone, Serialize)]
pub struct ParaShape {
    pub align: Alignment,
    /// Line spacing in HWP units or percentage depending on `line_spacing_type`.
    pub line_spacing: i32,
    pub line_spacing_type: SpacingType,
    /// Space before paragraph (HWP units).
    pub space_before: i32,
    /// Space after paragraph (HWP units).
    pub space_after: i32,
    /// Left margin (HWP units).
    pub margin_left: i32,
    /// Right margin (HWP units).
    pub margin_right: i32,
    /// First line indent (positive) or hanging indent (negative).
    pub indent: i32,
    /// Tab definition ID.
    pub tab_def_id: u16,
    /// Heading type (none, outline, number, bullet).
    pub heading_type: u8,
    /// Heading level (0-based).
    pub heading_level: u8,
    /// Border fill ID.
    pub border_fill_id: u16,
    /// Whether to break page before this paragraph.
    pub page_break_before: bool,
    /// Whether to keep with next paragraph.
    pub keep_with_next: bool,
    /// Whether to keep lines together (no break inside).
    pub keep_together: bool,
    /// Widow/orphan protection.
    pub widow_orphan: bool,
}

impl Default for ParaShape {
    fn default() -> Self {
        Self {
            align: Alignment::Justify,
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    #[default]
    Justify,
    Left,
    Right,
    Center,
    Distribute,
    DistributeSpace,
    Other(u8),
}

impl From<u8> for Alignment {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Justify,
            1 => Self::Left,
            2 => Self::Right,
            3 => Self::Center,
            4 => Self::Distribute,
            5 => Self::DistributeSpace,
            n => Self::Other(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingType {
    #[default]
    Percent,
    Fixed,
    BetweenLines,
    AtLeast,
}

impl From<u8> for SpacingType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Percent,
            1 => Self::Fixed,
            2 => Self::BetweenLines,
            3 => Self::AtLeast,
            _ => Self::Percent,
        }
    }
}
