//! A map's package drawn under the roads (`omsi_map::package`): the ground, areas of water, green
//! land and built-up land, then rivers, rail and borders, in the package's own colours.

use super::*;
use omsi_map::package::{self, Package};

/// The package's surface as vertices relative to `anchor`, to be drawn under the roads, in the
/// maps' own colours (`pal`). `near` limits it to the chunks within a radius of a point (the
/// small map); `None` builds the whole map.
pub(super) fn build_surface(p: &mut Painter, pkg: &Package, anchor: DVec2, near: Option<(DVec2, f64)>, pal: &super::style::Palette) {
    let rel = |q: (f64, f64)| Vec3::new((q.0 - anchor.x) as f32, (q.1 - anchor.y) as f32, 0.0);
    let area = near.map(|(c, r)| ((c.x - r, c.y - r), (c.x + r, c.y + r)));
    let layers = pkg.layers(area);
    let order = [
        (package::FILL_BUILT, pal.paved_land),
        (package::FILL_FARM, pal.field),
        (package::FILL_GRASS, pal.grass),
        (package::FILL_GREEN, pal.grass.mix(pal.forest, 0.35)),
        (package::FILL_FOREST, pal.forest),
        (package::FILL_SPORT, pal.grass.mix(pal.forest, 0.2)),
        (package::FILL_WATER, pal.water),
    ];
    for (class, c) in order {
        for t in &layers.fills[class as usize] {
            p.world_poly(&[rel(t[0]), rel(t[1]), rel(t[2])], c);
        }
    }
    for want in [package::LINE_RIVER, package::LINE_RAIL, package::LINE_BORDER] {
        for l in layers.lines.iter().filter(|l| l.class == want) {
            let v: Vec<Vec3> = l.points.iter().map(|q| rel(*q)).collect();
            match l.class {
                package::LINE_RIVER if l.width >= 5.0 => p.ribbon(&v, 9.0, 2.6, pal.water, false),
                package::LINE_RIVER => p.ribbon(&v, 3.5, 1.5, pal.water, false),
                package::LINE_RAIL => p.ribbon(&v, 3.6, 3.0, pal.rail, false),
                _ => p.ribbon(&v, 1.4, 1.4, pal.card_dim.alpha(0.5), false),
            }
        }
    }
}

impl Navigator {
    /// The map's package, given at once (an offscreen picture reads it on its own thread).
    pub fn set_surface(&mut self, surface: Option<Package>) {
        if let Some(s) = surface {
            self.place = super::place::PlaceView::of(&s);
            self.surface = Some(std::sync::Arc::new(s));
            self.surface_version += 1;
        }
    }

    /// The data's attribution (the package's licence asks for it), bottom right of the city map.
    pub(super) fn draw_credit(&mut self, ui: &mut Painter, w: f32, h: f32, s: f32) {
        let pal = super::style::palette(self.light);
        if let (true, Some(sf)) = (self.show_surface, self.surface.as_ref().filter(|s| !s.credit.is_empty())) {
            ui.text(&mut self.atlas, &self.fonts, &sf.credit, 10.5 * s, Weight::Medium, Vec2::new(w - 12.0 * s, h - 8.0 * s), Align::Right, pal.card_dim);
        }
    }
}
