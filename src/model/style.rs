use serde::Serialize;

/// Named style definition.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Style {
    /// Style name (Korean or English).
    pub name: String,
    /// English name.
    pub english_name: String,
    /// Style type: 0=paragraph, 1=character.
    pub style_type: u8,
    /// Next style ID.
    pub next_style_id: u16,
    /// Character shape ID.
    pub char_shape_id: u16,
    /// Paragraph shape ID.
    pub para_shape_id: u16,
}
