//! Interface drawing shared by the game's navigator and the launcher.
//!
//! * [`text`]: Roboto (variable weight) laid out and rasterised with `ab_glyph`;
//! * [`icons`]: Google's Material Symbols (Rounded, filled), rasterised with `resvg`;
//! * [`atlas`]: one RGBA texture both live in, filled on demand and uploaded by region;
//! * [`paint`]: a vertex list of anti-aliased shapes - rounded boxes with soft shadows,
//!   circles, rings, lines, text and icons in pixels, and ribbons and markers in a 3D
//!   world whose width can be given in metres, in pixels, or the larger of the two;
//! * [`gpu`]: the wgpu pipeline that draws such lists (MSAA, premultiplied alpha,
//!   rounded clipping), into a window or a texture.

pub mod atlas;
pub mod gpu;
pub mod i18n;
pub mod icons;
pub mod paint;
pub mod text;

pub use atlas::{Atlas, Sprite};
pub use gpu::{Draw, Gpu, Layer};
pub use i18n::tr;
pub use paint::{Color, Painter, Rect, Vertex};
pub use text::{Fonts, Weight};
