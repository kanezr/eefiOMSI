//! `[texttexture]`: textures generated from script string variables with `.oft` fonts.
//!
//! Every text texture is identified by its index in the model's `[texttexture]` list;
//! materials refer to it with `[useTextTexture] n`.

use hashbrown::HashMap;
use omsi_content::font::{Font, FontAtlas};
use omsi_model::TextTexture;
use std::path::Path;
use std::sync::Arc;

pub struct FontLibrary {
    atlases: HashMap<String, Option<Arc<FontAtlas>>>,
    root: std::path::PathBuf,
    /// Every `[newfont]` of every content root's `Fonts/*.oft`, in lookup order; read once.
    index: Option<Vec<Font>>,
}

/// Style words at the end of a font name ("churafont++ 32x8 Bold").
const STYLE_WORDS: &[&str] = &["bold", "heavy", "black", "light", "thin", "medium", "regular", "italic", "narrow", "condensed", "wide"];

/// A font name without its trailing style words, lower case: the family and size.
fn family_of(name: &str) -> String {
    let mut words: Vec<String> = name.split_whitespace().map(|w| w.to_ascii_lowercase()).collect();
    while words.len() > 1 && words.last().map(|w| STYLE_WORDS.contains(&w.as_str())).unwrap_or(false) {
        words.pop();
    }
    words.join(" ")
}

/// The cell size a font's name carries ("Krueger 16x9", "churafont++ Numeric 26x11 Bold"),
/// used to keep a substitute the same size as the font a script asked for.
fn size_of(name: &str) -> Option<(u32, u32)> {
    name.split_whitespace().find_map(|w| {
        let w = w.to_ascii_lowercase();
        let (a, b) = w.split_once('x')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    })
}

impl FontLibrary {
    pub fn new(root: &Path) -> FontLibrary {
        FontLibrary { atlases: HashMap::new(), root: root.to_path_buf(), index: None }
    }

    fn index(&mut self) -> &[Font] {
        if self.index.is_none() {
            // the Fonts folder of every content root (installed mods and archives first, then
            // the installation): a mod's display fonts live in the content folder, and looking
            // only in the installation's `Fonts` left the O530's number plate, matrix and
            // dashboard displays empty. A file of the same name higher up replaces the stock one.
            let mut files = omsi_cfg::read_dir_merged("Fonts");
            if files.is_empty() {
                files = omsi_cfg::vfs::read_dir_paths(&self.root.join("Fonts"));
            }
            // each folder in root order, its files in name order (a listing comes in the file
            // system's order)
            let mut folders: Vec<std::path::PathBuf> = Vec::new();
            for p in &files {
                let d = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                if !folders.contains(&d) {
                    folders.push(d);
                }
            }
            files.sort_by_cached_key(|p| (folders.iter().position(|d| Some(d.as_path()) == p.parent()).unwrap_or(usize::MAX), p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()));
            // Two files of one folder naming the same font: the one read later takes its place,
            // as in the original (the LiAZ's `ANX_S.oft` spells its "ц" as "=", and a
            // "ANX_S - копия.oft" lying beside it, read first, left the letter out). A
            // folder of higher priority keeps its fonts over the folders after it.
            let mut all: Vec<Font> = Vec::new();
            let mut folder_of: Vec<usize> = Vec::new();
            for p in files {
                if !p.extension().map(|x| x.eq_ignore_ascii_case("oft")).unwrap_or(false) {
                    continue;
                }
                let folder = folders.iter().position(|d| Some(d.as_path()) == p.parent()).unwrap_or(usize::MAX);
                // `Font::path` is the .oft (read through the VFS): its bitmaps lie next to it
                if let Ok(list) = Font::load_all(&p) {
                    for f in list {
                        let key = f.name.trim().to_ascii_lowercase();
                        match all.iter().position(|o| o.name.trim().to_ascii_lowercase() == key) {
                            Some(k) if folder_of[k] == folder => all[k] = f,
                            Some(_) => {}
                            None => {
                                all.push(f);
                                folder_of.push(folder);
                            }
                        }
                    }
                }
            }
            self.index = Some(all);
        }
        self.index.as_deref().unwrap_or(&[])
    }

    /// Whether a font of exactly this name is installed.
    pub fn has_exact(&mut self, name: &str) -> bool {
        let wanted = name.trim();
        self.index().iter().any(|f| f.name.trim().eq_ignore_ascii_case(wanted))
    }

    /// The font called `name`, or - when there is none - one of the same family and size in
    /// another weight: mods ask for weights their packs never shipped (the Citaro pack's
    /// Krüger matrix wants "churafont++ Numeric 26x11 Bold" and "churafont++ 32x8 Bold";
    /// the fonts are "churafont++ Numeric 26x11" and "churafont++ 32x8"), and without it
    /// the line number stayed off the destination display. A name of no known family (the
    /// depot strings the matrix tries as custom fonts) is still not found.
    fn find(&mut self, name: &str) -> Option<Font> {
        let wanted = name.trim();
        if wanted.is_empty() {
            return None;
        }
        let index = self.index();
        if let Some(f) = index.iter().find(|f| f.name.trim().eq_ignore_ascii_case(wanted)) {
            return Some(f.clone());
        }
        let family = family_of(wanted);
        // the plain weight first, then the others in file order - but only a font of the same
        // size. A dot-matrix display (the Krüger matrix asks for eight sizes by name, from
        // "Krueger 7x4" to "Krueger 16x9") draws its letters cell by cell: substituting
        // another size there does not make the text wider or narrower, it makes it a soup of
        // letter fragments. Without a font of that size the script must hear "no font" (-1)
        // and pick the next size itself, which is what the original does.
        let sibling = index
            .iter()
            .filter(|f| size_of(&f.name) == size_of(wanted))
            .find(|f| f.name.trim().eq_ignore_ascii_case(&family))
            .or_else(|| index.iter().filter(|f| size_of(&f.name) == size_of(wanted)).find(|f| family_of(&f.name) == family));
        let Some(sibling) = sibling else {
            // a display whose font is missing is worth a line in the log: it is the first
            // thing to look at when letters come out wrong on a matrix or a plate
            log::warn!("font \"{wanted}\" is in no Fonts folder of any content root");
            return None;
        };
        log::warn!("font \"{wanted}\" not found; drawing with \"{}\" (same family and size)", sibling.name.trim());
        Some(sibling.clone())
    }

    /// `get` with the built-in image decoder.
    pub fn load(&mut self, name: &str) -> Option<Arc<FontAtlas>> {
        self.get(name, &|p| omsi_texture::decode_file(p).ok().map(|i| (i.width, i.height, i.rgba)))
    }

    /// Fonts are looked up by their `[newfont]` name across all `Fonts/*.oft` files of every
    /// content root (installed mods first, then the installation): a mod's display fonts
    /// live in the content folder, and looking only in the installation's `Fonts` left the
    /// O530's number plate, matrix and dashboard displays empty.
    pub fn get(&mut self, name: &str, decode: &dyn Fn(&Path) -> Option<(u32, u32, Vec<u8>)>) -> Option<Arc<FontAtlas>> {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.atlases.get(&key) {
            return a.clone();
        }
        let found = self.find(name);
        let atlas = found.and_then(|f| {
            // the bitmaps sit beside the .oft that names them
            let fonts_dir = f.path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.join("Fonts"));
            let alpha_path = omsi_cfg::resolve_path(&fonts_dir, &f.alpha);
            let color_path = omsi_cfg::resolve_path(&fonts_dir, &f.bitmap);
            let (aw, ah, alpha) = decode(&alpha_path)?;
            let color = match (color_path != alpha_path).then(|| decode(&color_path)).flatten() {
                Some((cw, ch, c)) if (cw, ch) == (aw, ah) => c,
                Some((cw, ch, c)) if cw > 0 && ch > 0 && c.len() == (cw * ch * 4) as usize => color_at_alpha_pixels(&c, cw, ch, aw, ah),
                _ => alpha.clone(),
            };
            Some(Arc::new(FontAtlas::new(f, aw, ah, color, alpha)))
        });
        if atlas.is_none() && !name.trim().is_empty() {
            log::warn!("font \"{name}\" not found");
        }
        self.atlases.insert(key, atlas.clone());
        atlas
    }
}

/// A font's colour bitmap laid over its alpha bitmap pixel for pixel. Omsi.exe reads a
/// glyph's colour at the very pixel it reads its coverage at, in the colour bitmap's own
/// rows (0x5d67bc: the same scanline row and byte column in both), whatever size either
/// has - a colour bitmap need not be the alpha's size, and often is a small swatch of the
/// one colour (the stock `EFADfont.bmp` is 128 x 128 under a 128 x 200 alpha). Past its
/// edges, where Omsi.exe reads into other rows or fails, the swatch repeats. (Taken as no
/// colour bitmap at all, a full-colour text came out in the alpha's white, #829.)
fn color_at_alpha_pixels(color: &[u8], cw: u32, ch: u32, aw: u32, ah: u32) -> Vec<u8> {
    let mut out = vec![0u8; (aw * ah * 4) as usize];
    for y in 0..ah {
        for x in 0..aw {
            let s = (((y % ch) * cw + x % cw) * 4) as usize;
            let d = ((y * aw + x) * 4) as usize;
            out[d..d + 4].copy_from_slice(&color[s..s + 4]);
        }
    }
    out
}

/// Runtime state of one `[texttexture]`.
pub struct TextTextureState {
    pub def: TextTexture,
    pub atlas: Option<Arc<FontAtlas>>,
    pub last_text: Option<String>,
    /// Latest rendered RGBA image, present when it changed since the last upload.
    pub pending: Option<Vec<u8>>,
}

impl TextTextureState {
    pub fn new(def: TextTexture, atlas: Option<Arc<FontAtlas>>) -> Self {
        Self { def, atlas, last_text: None, pending: None }
    }

    /// Re-render when the string variable changed. Returns true when a new image is pending.
    pub fn update(&mut self, text: &str) -> bool {
        if self.last_text.as_deref() == Some(text) {
            return false;
        }
        self.last_text = Some(text.to_string());
        self.pending = Some(self.image(text));
        true
    }

    /// The picture of `text` in this texture's font, size, colour and placement.
    pub fn image(&self, text: &str) -> Vec<u8> {
        let (w, h) = (self.def.width.max(1) as u32, self.def.height.max(1) as u32);
        let rgb = [self.def.color[0] as u8, self.def.color[1] as u8, self.def.color[2] as u8];
        let align = omsi_content::font::TextAlign { orientation: self.def.orientation, grid: self.def.grid };
        match &self.atlas {
            Some(a) => a.render_aligned(text, w, h, self.def.full_color, rgb, align),
            None => vec![0u8; (w * h * 4) as usize],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full-colour font whose colour bitmap is not the alpha's size takes each glyph
    /// pixel's colour from the same pixel of the colour bitmap, as Omsi.exe reads it, not
    /// the alpha's white (#829).
    #[test]
    fn a_colour_bitmap_of_another_size_still_colours_the_glyphs() {
        let dir = std::path::PathBuf::from("/fonts");
        let font = Font { path: dir.join("colour.oft"), name: "Colour".into(), bitmap: "colour.bmp".into(), alpha: "colour_alpha.bmp".into(), height: 4, gap: 0, chars: Vec::new() };
        let mut lib = FontLibrary::new(&dir);
        lib.index = Some(vec![font]);
        // a 4 x 4 white alpha under a 2 x 2 swatch: red, green / blue, yellow
        let swatch = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
        let decode = |p: &Path| -> Option<(u32, u32, Vec<u8>)> {
            match p.file_name()?.to_str()? {
                "colour_alpha.bmp" => Some((4, 4, vec![255; 64])),
                "colour.bmp" => Some((2, 2, swatch.concat())),
                _ => None,
            }
        };
        let atlas = lib.get("Colour", &decode).expect("the font loads");
        assert_eq!((atlas.width, atlas.height), (4, 4));
        for (x, y) in [(0usize, 0usize), (1, 0), (0, 1), (1, 1), (3, 2)] {
            let i = (y * 4 + x) * 4;
            assert_eq!(atlas.color[i..i + 4], swatch[(y % 2) * 2 + x % 2], "pixel ({x}, {y})");
        }
    }

    #[test]
    fn font_families() {
        assert_eq!(family_of("churafont++ Numeric 26x11 Bold"), "churafont++ numeric 26x11");
        assert_eq!(family_of("churafont++ 32x10 Heavy"), "churafont++ 32x10");
        assert_eq!(family_of("churafont++ 14x10"), "churafont++ 14x10");
        assert_eq!(family_of("Bold"), "bold");
        assert_eq!(family_of("LEERFELD"), "leerfeld");
    }
}
