use serde::Serialize;

/// Character shape — font, size, color, decorations.
/// `font_ids` is indexed by script type: Korean=0, English=1, Hanja=2, Japanese=3, Other=4, Symbol=5, User=6.
#[derive(Debug, Clone, Serialize)]
pub struct CharShape {
    /// Font face IDs per script type (7 entries).
    pub font_ids: [u16; 7],
    /// Font width ratios per script (%, typically 100).
    pub font_ratios: [u8; 7],
    /// Font spacing per script.
    pub font_spacings: [i8; 7],
    /// Font relative sizes per script (%, typically 100).
    pub font_rel_sizes: [u8; 7],
    /// Font offsets per script.
    pub font_offsets: [i8; 7],
    /// Character height in HWP units (1/7200 inch × 100, so 1000 = 10pt).
    pub height: u32,
    /// Text color (RRGGBB, no alpha).
    pub text_color: u32,
    /// Shade color.
    pub shade_color: u32,
    /// Shadow color.
    pub shadow_color: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: UnderlineType,
    pub underline_color: u32,
    pub strikeout: StrikeoutType,
    pub outline: OutlineType,
    pub shadow_type: ShadowType,
    pub emboss: bool,
    pub engrave: bool,
    pub superscript: bool,
    pub subscript: bool,
    /// Border fill ID (0 = none).
    pub border_fill_id: u16,
}

impl Default for CharShape {
    fn default() -> Self {
        Self {
            font_ids: [0; 7],
            font_ratios: [100; 7],
            font_spacings: [0; 7],
            font_rel_sizes: [100; 7],
            font_offsets: [0; 7],
            height: 1000,
            text_color: 0x000000,
            shade_color: 0xFFFFFF,
            shadow_color: 0xB2B2B2,
            bold: false,
            italic: false,
            underline: UnderlineType::None,
            underline_color: 0x000000,
            strikeout: StrikeoutType::None,
            outline: OutlineType::None,
            shadow_type: ShadowType::None,
            emboss: false,
            engrave: false,
            superscript: false,
            subscript: false,
            border_fill_id: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UnderlineType {
    #[default]
    None,
    Single,
    Double,
    Thick,
    Dash,
    Dot,
    DashDot,
    DashDotDot,
    Long,
    DoubleWave,
    Other(u8),
}

impl From<u8> for UnderlineType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Single,
            2 => Self::Double,
            3 => Self::Thick,
            4 => Self::Dash,
            5 => Self::Dot,
            6 => Self::DashDot,
            7 => Self::DashDotDot,
            8 => Self::Long,
            9 => Self::DoubleWave,
            n => Self::Other(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrikeoutType {
    #[default]
    None,
    Single,
    Other(u8),
}

impl From<u8> for StrikeoutType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Single,
            n => Self::Other(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutlineType {
    #[default]
    None,
    Solid,
    Dot,
    Dash,
    DashDot,
    DashDotDot,
    Other(u8),
}

impl From<u8> for OutlineType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Solid,
            2 => Self::Dot,
            3 => Self::Dash,
            4 => Self::DashDot,
            5 => Self::DashDotDot,
            n => Self::Other(n),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowType {
    #[default]
    None,
    Drop,
    Continuous,
    Other(u8),
}

impl From<u8> for ShadowType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::None,
            1 => Self::Drop,
            2 => Self::Continuous,
            n => Self::Other(n),
        }
    }
}
