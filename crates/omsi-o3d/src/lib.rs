//! OMSI mesh files.
//!
//! * `.o3d` - OMSI's own binary format (unit `mc_o3dfiles`), see `docs/FORMATS.md`.
//! * `.x`   - DirectX text meshes, still used by a handful of stock objects and the helper meshes.
//!
//! Both produce the same [`Mesh`]. Coordinates are OMSI's: X right, Y forward, Z up, metres.

use glam::{Mat4, Vec2, Vec3};
use std::path::Path;

pub mod xfile;

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    pub indices: [u32; 3],
    pub material: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    pub diffuse: [f32; 4],
    pub specular: [f32; 3],
    pub emissive: [f32; 3],
    pub specular_power: f32,
    /// Texture file name, relative to the model's texture directory. Empty = untextured.
    pub texture: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoneWeight {
    pub vertex: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bone {
    pub name: String,
    pub weights: Vec<BoneWeight>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
    /// Object transform. Identity when the file has none.
    pub transform: Mat4,
    /// The file gave the transform (an `.o3d` matrix section); an `.x` file or an `.o3d`
    /// without one has the identity, which says nothing about how it was exported.
    pub has_transform: bool,
    pub bones: Vec<Bone>,
    pub version: u8,
}

impl Default for Material {
    fn default() -> Self {
        Self { diffuse: [1.0; 4], specular: [0.0; 3], emissive: [0.0; 3], specular_power: 1.0, texture: String::new() }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum O3dError {
    #[error("not an o3d file (bad magic)")]
    BadMagic,
    #[error("unexpected end of file at offset {0}")]
    Eof(usize),
    #[error("unknown section tag 0x{tag:02x} at offset {offset}")]
    BadTag { tag: u8, offset: usize },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error(".x parse error: {0}")]
    XFile(String),
}

struct Cur<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn need(&self, n: usize) -> Result<(), O3dError> {
        if self.p + n > self.b.len() {
            Err(O3dError::Eof(self.p))
        } else {
            Ok(())
        }
    }
    fn u8(&mut self) -> Result<u8, O3dError> {
        self.need(1)?;
        let v = self.b[self.p];
        self.p += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, O3dError> {
        self.need(2)?;
        let v = u16::from_le_bytes([self.b[self.p], self.b[self.p + 1]]);
        self.p += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, O3dError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.b[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        Ok(v)
    }
    fn f32(&mut self) -> Result<f32, O3dError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn str8(&mut self) -> Result<String, O3dError> {
        let n = self.u8()? as usize;
        self.need(n)?;
        let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&self.b[self.p..self.p + n]);
        self.p += n;
        Ok(s.into_owned())
    }
}

/// Parse an `.o3d` file from memory.
pub fn parse_o3d(bytes: &[u8]) -> Result<Mesh, O3dError> {
    let mut c = Cur { b: bytes, p: 0 };
    if c.u8()? != 0x84 || c.u8()? != 0x19 {
        return Err(O3dError::BadMagic);
    }
    let version = c.u8()?;
    let mut long_indices = false;
    let mut alt = false;
    if version >= 3 {
        let flags = c.u8()?;
        long_indices = flags & 1 != 0;
        alt = flags & 2 != 0;
    }
    let mut key = 0u32;
    if version >= 4 {
        key = c.u32()?;
    }
    // Files of version 4+ carry a key; unless it is 0xFFFFFFFF the vertex records are
    // scrambled (see `unscramble`). The original additionally refuses keys it does not know;
    // that check is deliberately not reproduced.
    let scrambled = version >= 4 && key != 0xFFFF_FFFF;
    let mut state: u16 = if scrambled {
        let k = key as i64 + (version as i64 - 4) + if alt { 0x17D } else { 0 };
        (k.rem_euclid(0xFDE8)) as u16
    } else {
        0
    };
    let wide_count = version >= 3;
    let mut mesh = Mesh { transform: Mat4::IDENTITY, version, ..Default::default() };

    while c.p < bytes.len() {
        let tag = c.u8()?;
        match tag {
            0x17 => {
                let n = if wide_count { c.u32()? as usize } else { c.u16()? as usize };
                c.need(n * 32)?;
                mesh.vertices.reserve(n);
                let n16 = (n as i64 % 0xFDE8) as u32;
                let mut fb: u32 = 0;
                for _ in 0..n {
                    let mut position = Vec3::new(c.f32()?, c.f32()?, c.f32()?);
                    let mut normal = Vec3::new(c.f32()?, c.f32()?, c.f32()?);
                    let mut uv = Vec2::new(c.f32()?, c.f32()?);
                    if scrambled {
                        if key == 0 {
                            state = if alt { 0x130 } else { 0 };
                        }
                        state = ((state as u32 * n16 + n16 * fb) % 8000) as u16;
                        fb = frac_byte(position);
                        unscramble(state, &mut position, &mut normal, &mut uv);
                    }
                    mesh.vertices.push(Vertex { position, normal, uv });
                }
            }
            0x49 => {
                let n = if wide_count { c.u32()? as usize } else { c.u16()? as usize };
                let isz = if long_indices { 4 } else { 2 };
                c.need(n * (3 * isz + 2))?;
                mesh.triangles.reserve(n);
                for _ in 0..n {
                    let mut idx = [0u32; 3];
                    for i in idx.iter_mut() {
                        *i = if long_indices { c.u32()? } else { c.u16()? as u32 };
                    }
                    let material = c.u16()?;
                    mesh.triangles.push(Triangle { indices: idx, material });
                }
            }
            0x26 => {
                let n = c.u16()? as usize;
                for _ in 0..n {
                    let diffuse = [c.f32()?, c.f32()?, c.f32()?, c.f32()?];
                    let specular = [c.f32()?, c.f32()?, c.f32()?];
                    let emissive = [c.f32()?, c.f32()?, c.f32()?];
                    let specular_power = c.f32()?;
                    let texture = c.str8()?;
                    mesh.materials.push(Material { diffuse, specular, emissive, specular_power, texture });
                }
            }
            0x79 => {
                let mut m = [0f32; 16];
                for v in m.iter_mut() {
                    *v = c.f32()?;
                }
                // File stores rows; row 3 is the translation (D3D convention, row vectors).
                mesh.transform = Mat4::from_cols_array(&m);
                mesh.has_transform = true;
            }
            0x54 => {
                let n = c.u16()? as usize;
                for _ in 0..n {
                    let name = c.str8()?;
                    let w = c.u16()? as usize;
                    let mut weights = Vec::with_capacity(w);
                    for _ in 0..w {
                        let vertex = if long_indices { c.u32()? } else { c.u16()? as u32 };
                        let weight = c.f32()?;
                        weights.push(BoneWeight { vertex, weight });
                    }
                    mesh.bones.push(Bone { name, weights });
                }
            }
            // any other byte is passed over, one at a time, as the exe's loader does
            // (`mc_o3dfile`: the tag `case` has no else, the loop reads the next byte).
            // Protected v7 files (flags 3) have three junk bytes inside the transform and
            // a few more after it; refusing them lost the whole body of the MB Sprinter
            // 412D XLWB and 49 other meshes of that pack.
            _ => {}
        }
    }
    mesh.drop_bad_triangles();
    Ok(mesh)
}

/// Mixing byte derived from the raw (still scrambled) position of a vertex; feeds the state
/// of the *next* vertex.
fn frac_byte(p: Vec3) -> u32 {
    let f = |x: f32| (x as f64) - (x as f64).trunc();
    let prod = (f(p.x) * f(p.y) * f(p.z)).abs() * 600.0;
    (prod.trunc() as i64).rem_euclid(256) as u32
}

/// Undo the per-vertex scrambling of version 4+ files.
fn unscramble(s: u16, p: &mut Vec3, n: &mut Vec3, uv: &mut Vec2) {
    if s < 1000 {
        std::mem::swap(&mut p.x, &mut p.y);
    } else if s < 3000 {
        std::mem::swap(&mut p.x, &mut p.z);
    } else if s > 7000 {
        std::mem::swap(&mut p.y, &mut p.z);
    }
    if s & 3 == 0 {
        n.x = -n.x;
    }
    if s % 6 == 0 {
        n.y = -n.y;
    }
    if s % 7 == 0 {
        n.z = -n.z;
    }
    if s < 600 {
        std::mem::swap(&mut n.y, &mut n.z);
    } else if s > 4500 {
        std::mem::swap(&mut n.x, &mut n.y);
    } else if s > 6500 {
        std::mem::swap(&mut n.x, &mut n.z);
    }
    if s % 5 == 0 {
        let m = (s % 100) as f32;
        uv.x -= m * m / 10000.0;
    }
    if s % 3 == 0 {
        let m = (s % 50) as f32;
        uv.y -= m * m / 2500.0;
    }
}

/// Load a mesh by file name, choosing the parser by extension.
pub fn load_mesh(path: &Path) -> Result<Mesh, O3dError> {
    let bytes = omsi_cfg::vfs::read(path)?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    if ext == "x" {
        xfile::parse_x(&bytes)
    } else {
        parse_o3d(&bytes)
    }
}

impl Mesh {
    /// Leave out triangles naming a vertex the file does not have (a damaged or hand-made
    /// file; every user of the mesh indexes the vertices with them).
    pub fn drop_bad_triangles(&mut self) {
        let n = self.vertices.len() as u32;
        let before = self.triangles.len();
        self.triangles.retain(|t| t.indices.iter().all(|&i| i < n));
        if self.triangles.len() != before {
            log::warn!("mesh: {} triangles name vertices beyond the {n} there are; left out", before - self.triangles.len());
        }
    }

    /// Axis-aligned bounds of the untransformed vertices.
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for v in &self.vertices {
            lo = lo.min(v.position);
            hi = hi.max(v.position);
        }
        if self.vertices.is_empty() {
            (Vec3::ZERO, Vec3::ZERO)
        } else {
            (lo, hi)
        }
    }

    /// Translation part of the object transform (D3D row-vector convention: 4th row).
    pub fn origin(&self) -> Vec3 {
        let r = self.transform.col(3);
        // `from_cols_array` interpreted the 16 floats column-wise; the file is row-major with
        // the translation in elements 12..15, which land in column 3. Good enough for the
        // translation; full transform users should call `transform_row_major()`.
        Vec3::new(r.x, r.y, r.z)
    }

    /// The transform as a column-vector matrix usable with glam (`M * v`).
    pub fn transform_row_major(&self) -> Mat4 {
        self.transform.transpose()
    }
}
