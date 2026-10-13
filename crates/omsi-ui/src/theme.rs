//! The launcher's look: flat and dark (neutral greys, no tint), one amber accent used
//! sparingly - after the calm menus of Euro Truck Simulator 2.

use crate::Color;

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

/// The desktop launcher's calm look (`launcher/ui/styles.css`), which the game's own screens
/// over the picture follow (`Ui::flat`): graphite surfaces one step lighter each, no edges.
pub const BG: Color = Color::rgba(18, 18, 18, 1.0);
pub const SIDEBAR: Color = Color::rgba(23, 23, 23, 1.0);
pub const SURFACE: Color = Color::rgba(28, 28, 28, 1.0);
pub const SURFACE_2: Color = Color::rgba(37, 37, 37, 1.0);
pub const SURFACE_3: Color = Color::rgba(47, 47, 47, 1.0);
pub const INK: Color = Color::rgba(237, 237, 237, 1.0);
pub const INK_2: Color = Color::rgba(184, 184, 184, 1.0);
pub const INK_MUTED: Color = Color::rgba(138, 138, 138, 1.0);
pub const ACCENT_HOVER: Color = Color::rgba(240, 173, 69, 1.0);
pub const ACCENT_INK: Color = Color::rgba(26, 18, 6, 1.0);
pub const DANGER_2: Color = Color::rgba(229, 83, 75, 1.0);

pub const RADIUS: f32 = 8.0;
/// Height of a control row.
pub const ROW: f32 = 36.0;
pub const GAP: f32 = 12.0;
/// Width of the left rail (points).
pub const RAIL_W: f32 = 236.0;
