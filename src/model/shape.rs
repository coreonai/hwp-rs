use serde::Serialize;

/// Drawing shape objects.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeObject {
    Line {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
    },
    Rectangle {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        round_rate: u8,
    },
    Ellipse {
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    Polygon {
        points: Vec<(i32, i32)>,
    },
    Curve {
        points: Vec<(i32, i32)>,
    },
    Picture {
        bin_data_id: u16,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        /// Original image dimensions.
        orig_width: u32,
        orig_height: u32,
        /// Crop rect (left, top, right, bottom) in HWP units.
        crop: [i32; 4],
    },
    Ole {
        bin_data_id: u16,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    },
    Container {
        children: Vec<ShapeObject>,
    },
}
