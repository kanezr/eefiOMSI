/// A broad physical classification of an OMSI road or painted ground surface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum SurfaceKind {
    Asphalt,
    Concrete,
    Paving,
    Kerb,
    SoftGround,
    Snow,
    #[default]
    Unknown,
}

impl SurfaceKind {
    /// Classify the stock OMSI `[surface]` IDs without changing the IDs exposed to scripts.
    pub fn from_id(id: u8) -> Self {
        match id {
            0 => Self::Asphalt,
            1 => Self::Concrete,
            2 => Self::Paving,
            3..=5 => Self::SoftGround,
            6..=7 => Self::Snow,
            _ => Self::Unknown,
        }
    }

    /// Classify texture names that distinguish kerbs from other stock surface IDs.
    pub fn from_texture_name(texture: &str, fallback: u8) -> Self {
        let name = texture.replace('\\', "/").rsplit('/').next().unwrap_or("").to_ascii_lowercase();
        if ["kerb", "curb", "bordstein", "bordure", "randstein"].iter().any(|word| name.contains(word)) {
            Self::Kerb
        } else {
            Self::from_id(fallback)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SurfaceKind;
    use super::SurfaceKind::*;

    #[test]
    fn stock_surface_ids_and_named_kerbs_are_classified() {
        assert_eq!(SurfaceKind::from_id(0), Asphalt);
        assert_eq!(SurfaceKind::from_id(1), Concrete);
        assert_eq!(SurfaceKind::from_id(2), Paving);
        assert_eq!(SurfaceKind::from_id(4), SoftGround);
        assert_eq!(SurfaceKind::from_id(6), Snow);
        assert_eq!(SurfaceKind::from_id(42), Unknown);
        assert_eq!(SurfaceKind::from_texture_name("Bordstein.bmp", 1), Kerb);
        assert_eq!(SurfaceKind::from_texture_name("concrete-road.bmp", 1), Concrete);
    }
}
