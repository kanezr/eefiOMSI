//! Built-in Material Symbols and custom SVG icons from `assets/icons`.

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// The SVG source of an icon by name (`directions_bus`, `stop_request`).
pub fn svg(name: &str) -> Option<&'static str> {
    ICONS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

/// Every icon name there is.
pub fn names() -> impl Iterator<Item = &'static str> {
    ICONS.iter().map(|(n, _)| *n)
}

/// An icon as an anti-aliased alpha mask of `size` x `size` pixels, with the SVG's margins.
pub fn rasterize(name: &str, size: u32) -> Option<Vec<u8>> {
    let src = svg(name)?;
    let opt = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(src, &opt).ok()?;
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size)?;
    let s = tree.size();
    let k = size as f32 / s.width().max(s.height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    Some(pix.pixels().iter().map(|p| p.alpha()).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn stop_request_has_smooth_edges_and_an_open_background() {
        for size in [24, 32, 64] {
            let alpha = super::rasterize("stop_request", size).unwrap();
            assert!(alpha.iter().any(|&a| a > 0 && a < 255), "edges must be anti-aliased");
            assert_eq!(alpha[(size / 4 * size + size / 2) as usize], 0);
            assert_eq!(alpha[(size / 2 * size + size * 3 / 4) as usize], 0, "the bar's interior is transparent");
            assert!(alpha.iter().filter(|&&a| a > 128).count() > (size * size / 12) as usize);
        }
    }

    #[test]
    fn icons_are_built_in_and_draw() {
        assert!(super::names().count() > 50);
        let a = super::rasterize("directions_bus", 32).unwrap();
        assert_eq!(a.len(), 32 * 32);
        let covered = a.iter().filter(|&&v| v > 128).count();
        assert!(covered > 150 && covered < 900, "{covered}");
        assert!(super::rasterize("no_such_icon", 32).is_none());
    }
}
