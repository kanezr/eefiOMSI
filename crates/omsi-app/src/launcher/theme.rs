//! The launcher's look: flat and dark (neutral greys, no tint), one amber accent used
//! sparingly - after the calm menus of Euro Truck Simulator 2.

use omsi_ui::Color;

pub const ACCENT: Color = Color::rgba(232, 160, 48, 1.0);
pub const ACCENT_2: Color = Color::rgba(96, 160, 232, 1.0);
pub const DANGER: Color = Color::rgba(222, 78, 68, 1.0);
pub const OK: Color = Color::rgba(104, 190, 118, 1.0);
pub const WARN: Color = Color::rgba(232, 170, 70, 1.0);

pub const TEXT: Color = Color::rgba(236, 236, 236, 1.0);
pub const TEXT_SOFT: Color = Color::rgba(200, 200, 200, 1.0);
pub const TEXT_DIM: Color = Color::rgba(142, 142, 142, 1.0);
pub const TEXT_FAINT: Color = Color::rgba(96, 96, 96, 1.0);

/// Rail, panels, fields.
pub const RAIL: Color = Color::rgba(18, 18, 18, 1.0);
pub const PANEL: Color = Color::rgba(22, 22, 22, 1.0);
pub const FIELD: Color = Color::rgba(31, 31, 31, 1.0);
pub const HOVER: Color = Color::rgba(38, 38, 38, 1.0);
pub const SELECTED: Color = Color::rgba(44, 44, 44, 1.0);
pub const EDGE: Color = Color::rgba(255, 255, 255, 0.06);

pub const RADIUS: f32 = 8.0;
/// Height of a control row.
pub const ROW: f32 = 36.0;
pub const GAP: f32 = 12.0;
