//! Minimal DirectX `.x` mesh reader: enough for the OMSI helper meshes and the scenery
//! objects that still ship `.x` files (frames, Mesh, MeshNormals, MeshTextureCoords, DeclData,
//! MeshMaterialList, Material, TextureFilename). A binary file (`xof 0302bin`, 26 PAD-Labs
//! houses of a Steam install) is first written out as the text it stands for.

use super::{Material, Mesh, O3dError, Triangle, Vertex};
use glam::{Mat3, Mat4, Vec2, Vec3};
use std::collections::HashMap;

struct Tok<'a> {
    s: &'a str,
    p: usize,
}

impl<'a> Tok<'a> {
    fn skip_ws(&mut self) {
        let b = self.s.as_bytes();
        loop {
            while self.p < b.len() && (b[self.p] as char).is_whitespace() {
                self.p += 1;
            }
            if self.p + 1 < b.len() && b[self.p] == b'/' && b[self.p + 1] == b'/' {
                while self.p < b.len() && b[self.p] != b'\n' {
                    self.p += 1;
                }
                continue;
            }
            if self.p < b.len() && b[self.p] == b'#' {
                while self.p < b.len() && b[self.p] != b'\n' {
                    self.p += 1;
                }
                continue;
            }
            break;
        }
    }
    fn next(&mut self) -> Option<&'a str> {
        self.skip_ws();
        let b = self.s.as_bytes();
        if self.p >= b.len() {
            return None;
        }
        let start = self.p;
        let c = b[self.p];
        if c == b'"' {
            self.p += 1;
            while self.p < b.len() && b[self.p] != b'"' {
                self.p += 1;
            }
            self.p += 1;
            return Some(&self.s[start..self.p.min(b.len())]);
        }
        if b"{};,;<>".contains(&c) {
            self.p += 1;
            return Some(&self.s[start..self.p]);
        }
        while self.p < b.len() && !(b[self.p] as char).is_whitespace() && !b"{};,<>\"".contains(&b[self.p]) {
            self.p += 1;
        }
        Some(&self.s[start..self.p])
    }
    fn peek(&mut self) -> Option<&'a str> {
        let save = self.p;
        let t = self.next();
        self.p = save;
        t
    }
    fn expect(&mut self, t: &str) -> Result<(), O3dError> {
        match self.next() {
            Some(x) if x == t => Ok(()),
            other => Err(O3dError::XFile(format!("expected '{t}', got {other:?}"))),
        }
    }
    fn number(&mut self) -> Result<f32, O3dError> {
        loop {
            let t = self.next().ok_or_else(|| O3dError::XFile("eof in number".into()))?;
            if t == "," || t == ";" {
                continue;
            }
            return t.parse::<f32>().map_err(|_| O3dError::XFile(format!("bad number {t}")));
        }
    }
    fn int(&mut self) -> Result<usize, O3dError> {
        Ok(self.number()? as usize)
    }
    /// A whole 32-bit word, read exactly: `DeclData` keeps its floats as their bit patterns,
    /// which an `f32` cannot hold to the last bit.
    fn dword(&mut self) -> Result<u32, O3dError> {
        loop {
            let t = self.next().ok_or_else(|| O3dError::XFile("eof in dword".into()))?;
            if t == "," || t == ";" {
                continue;
            }
            return t.parse::<u32>().map_err(|_| O3dError::XFile(format!("bad dword {t}")));
        }
    }
    /// Skip a `{ ... }` block (after the '{' has been consumed).
    fn skip_block(&mut self) -> Result<(), O3dError> {
        let mut depth = 1;
        while depth > 0 {
            match self.next() {
                Some("{") => depth += 1,
                Some("}") => depth -= 1,
                Some(_) => {}
                None => return Err(O3dError::XFile("eof in block".into())),
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct Ctx {
    mesh: Mesh,
    /// Named data objects in an .x file can be referenced from any frame or mesh.
    named_materials: HashMap<String, Material>,
    /// Resolve after the whole file, because definitions may follow their references.
    material_refs: Vec<(usize, String)>,
}

pub fn parse_x(bytes: &[u8]) -> Result<Mesh, O3dError> {
    // (the 16-byte header is checked as bytes: a short file or a non-ASCII byte there made
    // the text slices below panic)
    if bytes.len() < 16 || !bytes[..16].is_ascii() || !bytes.starts_with(b"xof ") {
        return Err(O3dError::XFile("missing xof header".into()));
    }
    let unpacked;
    let bytes = match &bytes[8..12] {
        b"txt " => bytes,
        b"bin " => {
            unpacked = binary_as_text(bytes)?;
            &unpacked[..]
        }
        // (`tzip`/`bzip`: MSZIP-packed, which no OMSI content uses)
        other => return Err(O3dError::XFile(format!(".x format {:?} not supported", String::from_utf8_lossy(other)))),
    };
    let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
    let text = text.into_owned();
    let mut t = Tok { s: &text, p: 16 };
    let mut ctx = Ctx {
        mesh: Mesh {
            transform: Mat4::IDENTITY,
            ..Default::default()
        },
        ..Default::default()
    };
    parse_objects(&mut t, &mut ctx, Mat4::IDENTITY)?;
    for (slot, name) in &ctx.material_refs {
        if let Some(material) = ctx.named_materials.get(name) {
            ctx.mesh.materials[*slot] = material.clone();
        }
    }
    if ctx.mesh.materials.is_empty() {
        ctx.mesh.materials.push(Material::default());
    }
    ctx.mesh.drop_bad_triangles();
    Ok(ctx.mesh)
}

/// A binary `.x` file written out as the text it stands for, so that one reader serves both.
/// The binary form is a stream of little-endian 16-bit tokens: names, strings, integers and
/// GUIDs carry their data, and the integer and float lists hold whole runs of values (a
/// mesh's vertices are one float list). The header's last four characters give the float
/// size, `0032` or `0064`.
fn binary_as_text(bytes: &[u8]) -> Result<Vec<u8>, O3dError> {
    struct Bin<'a> {
        b: &'a [u8],
        p: usize,
    }
    impl Bin<'_> {
        fn take(&mut self, n: usize) -> Result<&[u8], O3dError> {
            let end = self.p.checked_add(n).filter(|&e| e <= self.b.len());
            let end = end.ok_or_else(|| O3dError::XFile(format!("binary .x cut short at byte {}", self.p)))?;
            let s = &self.b[self.p..end];
            self.p = end;
            Ok(s)
        }
        fn word(&mut self) -> Result<u16, O3dError> {
            let s = self.take(2)?;
            Ok(u16::from_le_bytes([s[0], s[1]]))
        }
        fn dword(&mut self) -> Result<u32, O3dError> {
            let s = self.take(4)?;
            Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        }
        /// A count from the file, no larger than what is left to read in `unit`-byte pieces.
        fn count(&mut self, unit: usize) -> Result<usize, O3dError> {
            let n = self.dword()? as usize;
            if n.saturating_mul(unit) > self.b.len() - self.p {
                return Err(O3dError::XFile(format!("binary .x: {n} values do not fit the file")));
            }
            Ok(n)
        }
    }
    use std::io::Write;
    let doubles = &bytes[12..16] == b"0064";
    let mut r = Bin { b: bytes, p: 16 };
    let mut out = Vec::with_capacity(bytes.len() * 3);
    out.extend_from_slice(b"xof 0303txt 0032\n");
    while r.p < r.b.len() {
        let at = r.p;
        match r.word()? {
            // name: written as a word the text reader keeps whole
            1 => {
                let n = r.count(1)?;
                for &c in r.take(n)? {
                    let keep = c.is_ascii_graphic() && !b"{};,<>\"#/".contains(&c);
                    out.push(if keep { c } else { b'_' });
                }
                out.push(b' ');
            }
            // string, then the token that closes it (`;` or `,`)
            2 => {
                let n = r.count(1)?;
                let text: Vec<u8> = r.take(n)?.iter().copied().filter(|&c| c != b'"' && c != 0).collect();
                let close = r.word()?;
                out.push(b'"');
                out.extend_from_slice(&text);
                out.push(b'"');
                out.extend_from_slice(if close == 19 { b",\n" } else { b";\n" });
            }
            3 => {
                let v = r.dword()?;
                let _ = writeln!(out, "{v};");
            }
            // GUID (only templates carry one, and the reader skips them)
            5 => {
                r.take(16)?;
                out.extend_from_slice(b"<guid>\n");
            }
            6 => {
                let n = r.count(4)?;
                for _ in 0..n {
                    let v = r.dword()?;
                    let _ = write!(out, "{v},");
                }
                out.extend_from_slice(b";\n");
            }
            7 => {
                let size = if doubles { 8 } else { 4 };
                let n = r.count(size)?;
                for _ in 0..n {
                    let s = r.take(size)?;
                    // (a broken value must still read as a number)
                    let _ = if doubles {
                        let v = f64::from_le_bytes(s.try_into().expect("8 bytes"));
                        write!(out, "{},", if v.is_finite() { v } else { 0.0 })
                    } else {
                        let v = f32::from_le_bytes(s.try_into().expect("4 bytes"));
                        write!(out, "{},", if v.is_finite() { v } else { 0.0 })
                    };
                }
                out.extend_from_slice(b";\n");
            }
            10 => out.extend_from_slice(b"{\n"),
            11 => out.extend_from_slice(b"}\n"),
            12 => out.push(b'('),
            13 => out.push(b')'),
            14 => out.push(b'['),
            15 => out.push(b']'),
            16 => out.push(b'<'),
            17 => out.push(b'>'),
            18 => out.push(b'.'),
            19 => out.extend_from_slice(b",\n"),
            20 => out.extend_from_slice(b";\n"),
            31 => out.extend_from_slice(b"template "),
            // WORD, DWORD, FLOAT, ... ARRAY: the member types of a template
            40..=52 => out.extend_from_slice(b"type "),
            token => return Err(O3dError::XFile(format!("binary .x: unknown token {token} at byte {at}"))),
        }
    }
    Ok(out)
}

/// The size in 32-bit words of a `D3DDECLTYPE` (FLOAT1 .. FLOAT16_4), None for an unknown one.
fn decl_type_words(ty: u32) -> Option<usize> {
    match ty {
        0 | 4 | 5 | 6 | 8 | 9 | 11 | 13 | 14 | 15 => Some(1),
        1 | 7 | 10 | 12 | 16 => Some(2),
        2 => Some(3),
        3 => Some(4),
        _ => None,
    }
}

/// A half float (`D3DDECLTYPE_FLOAT16_2`) as an `f32`.
fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let frac = (h & 0x3ff) as f32;
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 => 0.0,
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

/// What a `DeclData` block gives a mesh: the vertex data of DirectX 9's vertex declarations,
/// one record per vertex. Exporters that write it (684 `.x` files of a Steam install, among
/// them the PAD-Labs houses and the trees on Köln) keep the texture coordinates only there.
#[derive(Default)]
struct DeclData {
    uvs: Option<Vec<Vec2>>,
    normals: Option<Vec<Vec3>>,
}

/// `DeclData { nElements; Type, Method, Usage, UsageIndex per element; nDWords; data }`, the
/// opening brace consumed. Only the first texture coordinates (FLOAT2 or FLOAT16_2) and the
/// first normal (FLOAT3) are taken; a declaration whose sizes do not add up gives nothing.
fn parse_decl_data(t: &mut Tok, nv: usize) -> Result<DeclData, O3dError> {
    const USAGE_NORMAL: u32 = 3;
    const USAGE_TEXCOORD: u32 = 5;
    let n = t.dword()? as usize;
    let mut stride = Some(0usize);
    let (mut uv, mut normal) = (None, None);
    for _ in 0..n {
        let (ty, _method, usage, index) = (t.dword()?, t.dword()?, t.dword()?, t.dword()?);
        let offset = stride.unwrap_or(0);
        if usage == USAGE_TEXCOORD && index == 0 && (ty == 1 || ty == 15) && uv.is_none() {
            uv = Some((offset, ty));
        }
        if usage == USAGE_NORMAL && index == 0 && ty == 2 && normal.is_none() {
            normal = Some(offset);
        }
        stride = stride.zip(decl_type_words(ty)).map(|(s, w)| s + w);
    }
    let words = t.dword()? as usize;
    let mut data = Vec::with_capacity(words.min(t.s.len().saturating_sub(t.p)));
    for _ in 0..words {
        data.push(t.dword()?);
    }
    skip_to_close(t)?;
    let Some(stride) = stride.filter(|&s| s > 0 && s * nv == data.len()) else {
        return Ok(DeclData::default());
    };
    let float = |w: u32| f32::from_bits(w);
    let uvs = uv.map(|(at, ty)| {
        data.chunks_exact(stride)
            .map(|v| match ty {
                1 => Vec2::new(float(v[at]), float(v[at + 1])),
                _ => Vec2::new(half_to_f32(v[at] as u16), half_to_f32((v[at] >> 16) as u16)),
            })
            .collect()
    });
    let normals = normal.map(|at| data.chunks_exact(stride).map(|v| Vec3::new(float(v[at]), float(v[at + 1]), float(v[at + 2]))).collect());
    Ok(DeclData { uvs, normals })
}

fn parse_objects(t: &mut Tok, ctx: &mut Ctx, xform: Mat4) -> Result<(), O3dError> {
    while let Some(tok) = t.next() {
        match tok {
            "}" => return Ok(()),
            "template" => {
                // template Name { ... }
                t.next();
                t.expect("{")?;
                t.skip_block()?;
            }
            "Frame" => {
                // Frame [name] { ... }
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                parse_frame(t, ctx, xform)?;
            }
            "Mesh" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                parse_mesh(t, ctx, xform)?;
            }
            "Material" => {
                let (name, material) = parse_material(t)?;
                if let Some(name) = name {
                    ctx.named_materials.insert(name, material);
                }
            }
            "AnimationSet" | "Animation" | "AnimTicksPerSecond" | "Header" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                t.skip_block()?;
            }
            _ => {
                // Unknown object: `Name [id] { ... }`
                if t.peek() != Some("{") {
                    t.next();
                }
                if t.peek() == Some("{") {
                    t.next();
                    t.skip_block()?;
                }
            }
        }
    }
    Ok(())
}

fn parse_frame(t: &mut Tok, ctx: &mut Ctx, parent: Mat4) -> Result<(), O3dError> {
    let mut xform = parent;
    loop {
        let tok = t.next().ok_or_else(|| O3dError::XFile("eof in frame".into()))?;
        match tok {
            "}" => return Ok(()),
            "FrameTransformMatrix" => {
                // a data object may carry a name (`FrameTransformMatrix relative {`, the 3ds
                // Max exporter of the Solaris Urbino 18 and the Novi Sad objects)
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                let mut m = [0f32; 16];
                for v in m.iter_mut() {
                    *v = t.number()?;
                }
                // consume trailing ';;' and '}'
                while let Some(x) = t.next() {
                    if x == "}" {
                        break;
                    }
                }
                // Direct3D writes the matrix row by row for row vectors (v' = v·M, the
                // translation in elements 12-14); read column by column that is already the
                // column-vector matrix glam uses. Transposing it as well dropped every
                // translation into the bottom row and turned the rotations the wrong way: the
                // BVG Citaro's Atron terminal and ALMEX (46 meshes 5.5 m forward and 1.3 m up)
                // sat at the bus origin under the floor, and the street name signs hung upside
                // down half way down their poles (their texts were turned by 180° to make up
                // for it; scene.rs no longer does).
                let local = Mat4::from_cols_array(&m);
                xform = parent * local;
            }
            "Frame" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                parse_frame(t, ctx, xform)?;
            }
            "Mesh" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                parse_mesh(t, ctx, xform)?;
            }
            "Material" => {
                let (name, material) = parse_material(t)?;
                if let Some(name) = name {
                    ctx.named_materials.insert(name, material);
                }
            }
            _ => {
                if t.peek() != Some("{") {
                    t.next();
                }
                if t.peek() == Some("{") {
                    t.next();
                    t.skip_block()?;
                }
            }
        }
    }
}

fn parse_mesh(t: &mut Tok, ctx: &mut Ctx, xform: Mat4) -> Result<(), O3dError> {
    let base_v = ctx.mesh.vertices.len() as u32;
    let base_m = ctx.mesh.materials.len() as u16;
    let nv = t.int()?;
    // (counts from the file only reserve what the text can hold)
    let room = t.s.len().saturating_sub(t.p);
    let mut positions = Vec::with_capacity(nv.min(room));
    for _ in 0..nv {
        let p = Vec3::new(t.number()?, t.number()?, t.number()?);
        positions.push(xform.transform_point3(p));
    }
    let nf = t.int()?;
    let mut faces: Vec<Vec<u32>> = Vec::with_capacity(nf.min(room));
    for _ in 0..nf {
        let n = t.int()?;
        let mut f = Vec::with_capacity(n.min(64));
        for _ in 0..n {
            f.push(t.int()? as u32);
        }
        faces.push(f);
    }
    let mut normals: Vec<Vec3> = vec![Vec3::Z; nv];
    let mut uvs: Vec<Vec2> = vec![Vec2::ZERO; nv];
    let mut face_mats: Vec<u16> = vec![0; nf];
    let mut mats: Vec<Material> = Vec::new();
    let mut material_refs: Vec<(usize, String)> = Vec::new();
    let (mut has_normals, mut has_uvs) = (false, false);
    let mut decl = DeclData::default();
    loop {
        let tok = t.next().ok_or_else(|| O3dError::XFile("eof in mesh".into()))?;
        match tok {
            "}" => break,
            "DeclData" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                decl = parse_decl_data(t, nv)?;
            }
            "MeshNormals" => {
                has_normals = true;
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                let nn = t.int()?;
                let mut nrm = Vec::with_capacity(nn.min(t.s.len().saturating_sub(t.p)));
                let linear = Mat3::from_mat4(xform);
                let normal_matrix = if linear.determinant().abs() > 1e-12 { linear.inverse().transpose() } else { linear };
                for _ in 0..nn {
                    let n = Vec3::new(t.number()?, t.number()?, t.number()?);
                    nrm.push((normal_matrix * n).normalize_or_zero());
                }
                let nfn = t.int()?;
                for fi in 0..nfn {
                    let n = t.int()?;
                    for k in 0..n {
                        let ni = t.int()?;
                        if fi < faces.len() && k < faces[fi].len() {
                            let vi = faces[fi][k] as usize;
                            if vi < normals.len() && ni < nrm.len() {
                                normals[vi] = nrm[ni];
                            }
                        }
                    }
                }
                skip_to_close(t)?;
            }
            "MeshTextureCoords" => {
                has_uvs = true;
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                let n = t.int()?;
                for i in 0..n {
                    let uv = Vec2::new(t.number()?, t.number()?);
                    if i < uvs.len() {
                        uvs[i] = uv;
                    }
                }
                skip_to_close(t)?;
            }
            "MeshMaterialList" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                let _nm = t.int()?;
                let nfi = t.int()?;
                for i in 0..nfi {
                    let m = t.int()? as u16;
                    if i < face_mats.len() {
                        face_mats[i] = m;
                    }
                }
                loop {
                    let tk = t.next().ok_or_else(|| O3dError::XFile("eof in matlist".into()))?;
                    match tk {
                        "}" => break,
                        "Material" => {
                            let (name, material) = parse_material(t)?;
                            if let Some(name) = name {
                                ctx.named_materials.insert(name, material.clone());
                            }
                            mats.push(material);
                        }
                        "{" => {
                            // Keep this slot even if the named material is defined later or
                            // absent: dropping it changes every following face material index.
                            let name = t.next().ok_or_else(|| {
                                O3dError::XFile("eof in material reference".into())
                            })?;
                            if matches!(name, "{" | "}") {
                                return Err(O3dError::XFile(
                                    "missing material reference name".into(),
                                ));
                            }
                            material_refs.push((mats.len(), name.to_string()));
                            mats.push(Material::default());
                            t.skip_block()?;
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                if t.peek() != Some("{") {
                    t.next();
                }
                if t.peek() == Some("{") {
                    t.next();
                    t.skip_block()?;
                }
            }
        }
    }
    // (`MeshTextureCoords` and `MeshNormals` win where a file has both)
    if let (Some(d), false) = (decl.uvs, has_uvs) {
        uvs = d;
    }
    if let (Some(d), false) = (decl.normals, has_normals) {
        let linear = Mat3::from_mat4(xform);
        let normal_matrix = if linear.determinant().abs() > 1e-12 { linear.inverse().transpose() } else { linear };
        normals = d.into_iter().map(|n| (normal_matrix * n).normalize_or_zero()).collect();
    }
    if mats.is_empty() {
        mats.push(Material::default());
    }
    for i in 0..nv {
        ctx.mesh.vertices.push(Vertex { position: positions[i], normal: normals[i], uv: uvs[i] });
    }
    for (fi, f) in faces.iter().enumerate() {
        let m = base_m + face_mats[fi].min(mats.len() as u16 - 1);
        for k in 1..f.len().saturating_sub(1) {
            ctx.mesh.triangles.push(Triangle { indices: [base_v + f[0], base_v + f[k], base_v + f[k + 1]], material: m });
        }
    }
    let material_base = ctx.mesh.materials.len();
    ctx.material_refs.extend(
        material_refs
            .into_iter()
            .map(|(slot, name)| (material_base + slot, name)),
    );
    ctx.mesh.materials.extend(mats);
    Ok(())
}

/// A material declaration after its `Material` token, either inline or a named object.
fn parse_material(t: &mut Tok) -> Result<(Option<String>, Material), O3dError> {
    let name = if t.peek() != Some("{") {
        t.next().map(str::to_string)
    } else {
        None
    };
    t.expect("{")?;
    let diffuse = [t.number()?, t.number()?, t.number()?, t.number()?];
    let power = t.number()?;
    let specular = [t.number()?, t.number()?, t.number()?];
    let emissive = [t.number()?, t.number()?, t.number()?];
    let mut texture = String::new();
    loop {
        match t
            .next()
            .ok_or_else(|| O3dError::XFile("eof in material".into()))?
        {
            "}" => break,
            "TextureFilename" => {
                if t.peek() != Some("{") {
                    t.next();
                }
                t.expect("{")?;
                if let Some(q) = t.next() {
                    texture = q.trim_matches('"').to_string();
                }
                skip_to_close(t)?;
            }
            "{" => t.skip_block()?,
            _ => {}
        }
    }
    Ok((
        name,
        Material {
            diffuse,
            specular,
            emissive,
            specular_power: power,
            texture,
        },
    ))
}

fn skip_to_close(t: &mut Tok) -> Result<(), O3dError> {
    let mut depth = 1;
    while depth > 0 {
        match t.next() {
            Some("{") => depth += 1,
            Some("}") => depth -= 1,
            Some(_) => {}
            None => return Err(O3dError::XFile("eof".into())),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    #[ignore = "requires installed traffic-light assets; set OMSI_ROOT or OMSI_TEST_CONTENT"]
    fn installed_traffic_light_material_references() {
        let Some(root) =
            omsi_cfg::flags::OMSI_ROOT.live_os().or_else(|| omsi_cfg::flags::OMSI_TEST_CONTENT.live_os())
        else {
            eprintln!("skipped: set OMSI_ROOT or OMSI_TEST_CONTENT to the installed content root");
            return;
        };
        let object = std::path::PathBuf::from(root).join("Sceneryobjects/D016_semafory");
        let mesh = crate::load_mesh(&object.join("model/semafor_3svetla.X")).unwrap();
        assert!(!mesh.materials.is_empty());
        assert!(!mesh.triangles.is_empty());
        // This housing references PDX01; another globally declared material is unused.
        // Resolving the wrong slot used to leave the housing white.
        for material in &mesh.materials {
            assert!(
                material.texture.eq_ignore_ascii_case("semafor_zaklad.bmp"),
                "unexpected housing texture: {}",
                material.texture
            );
            let texture = omsi_cfg::resolve_path(&object.join("texture"), &material.texture);
            assert!(
                texture.is_file(),
                "missing referenced texture: {}",
                texture.display()
            );
        }
        assert!(mesh
            .triangles
            .iter()
            .all(|triangle| (triangle.material as usize) < mesh.materials.len()));
    }

    /// An exporter that keeps the texture coordinates only in `DeclData` (the PAD-Labs houses):
    /// a normal (FLOAT3) and the first texture coordinates (FLOAT2) per vertex, the floats as
    /// their bit patterns. Read as UV 0,0 before, the houses were one flat colour.
    #[test]
    fn decl_data_gives_texture_coordinates_and_normals() {
        let uv = [(0.25f32, 0.5f32), (1.0, 0.0), (0.0, 1.0)];
        let mut data = Vec::new();
        for (u, v) in uv {
            for w in [0.0f32, 0.0, 1.0, u, v] {
                data.push(w.to_bits().to_string());
            }
        }
        let file = |words: &[String]| {
            format!(
                "xof 0303txt 0032\nMesh {{\n 3;\n 0;0;0;,\n 1;0;0;,\n 0;1;0;;\n 1;\n 3;0,1,2;;\n DeclData {{\n  2;\n  2;0;3;0;,\n  1;0;5;0;;\n  {};\n  {};\n }}\n}}\n",
                words.len(),
                words.join(",")
            )
        };
        let x = file(&data);
        let m = parse_x(x.as_bytes()).unwrap();
        let got: Vec<Vec2> = m.vertices.iter().map(|v| v.uv).collect();
        assert_eq!(got, [Vec2::new(0.25, 0.5), Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)]);
        assert!(m.vertices.iter().all(|v| (v.normal - Vec3::Z).length() < 1e-6));
        // MeshTextureCoords, where a file has both, is what counts
        let both = x.replace(" DeclData", " MeshTextureCoords {\n 3;\n 0.5;0.5;,\n 0.5;0.5;,\n 0.5;0.5;;\n }\n DeclData");
        let m = parse_x(both.as_bytes()).unwrap();
        assert!(m.vertices.iter().all(|v| v.uv == Vec2::splat(0.5)));
        // a declaration whose sizes do not add up to the data gives nothing
        let m = parse_x(file(&data[..14]).as_bytes()).unwrap();
        assert!(m.vertices.iter().all(|v| v.uv == Vec2::ZERO));
    }

    /// A binary file (`xof 0302bin 0032`) reads as the text it stands for: a triangle with
    /// texture coordinates and a textured material, built token by token.
    #[test]
    fn binary_files_are_read() {
        fn word(out: &mut Vec<u8>, w: u16) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        fn name(out: &mut Vec<u8>, s: &str) {
            word(out, 1);
            out.extend_from_slice(&(s.len() as u32).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        }
        fn ints(out: &mut Vec<u8>, v: &[u32]) {
            word(out, 6);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        fn floats(out: &mut Vec<u8>, v: &[f32]) {
            word(out, 7);
            out.extend_from_slice(&(v.len() as u32).to_le_bytes());
            v.iter().for_each(|x| out.extend_from_slice(&x.to_le_bytes()));
        }
        let (open, close) = (10, 11);
        let mut b = b"xof 0302bin 0032".to_vec();
        // a template, as exporters write them first
        word(&mut b, 31);
        name(&mut b, "Header");
        word(&mut b, open);
        word(&mut b, 5);
        b.extend_from_slice(&[0x11; 16]);
        word(&mut b, 40);
        name(&mut b, "major");
        word(&mut b, 20);
        word(&mut b, close);
        name(&mut b, "Mesh");
        name(&mut b, "house");
        word(&mut b, open);
        ints(&mut b, &[3]);
        floats(&mut b, &[0.0, 0.0, 0.0, 2.5, 0.0, 0.0, 0.0, 1.5, 0.0]);
        ints(&mut b, &[1, 3, 0, 1, 2]);
        name(&mut b, "MeshTextureCoords");
        word(&mut b, open);
        ints(&mut b, &[3]);
        floats(&mut b, &[0.0, 0.0, 1.0, 0.0, 0.0, 0.75]);
        word(&mut b, close);
        name(&mut b, "MeshMaterialList");
        word(&mut b, open);
        ints(&mut b, &[1, 1, 0]);
        name(&mut b, "Material");
        word(&mut b, open);
        floats(&mut b, &[1.0, 0.5, 0.25, 1.0, 10.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        name(&mut b, "TextureFilename");
        word(&mut b, open);
        word(&mut b, 2);
        b.extend_from_slice(&8u32.to_le_bytes());
        b.extend_from_slice(b"wall.bmp");
        word(&mut b, 20);
        word(&mut b, close);
        word(&mut b, close);
        word(&mut b, close);
        word(&mut b, close);
        let m = parse_x(&b).unwrap();
        let p: Vec<Vec3> = m.vertices.iter().map(|v| v.position).collect();
        assert_eq!(p, [Vec3::ZERO, Vec3::new(2.5, 0.0, 0.0), Vec3::new(0.0, 1.5, 0.0)]);
        assert_eq!(m.vertices[2].uv, Vec2::new(0.0, 0.75));
        assert_eq!(m.triangles.len(), 1);
        assert_eq!(m.materials.len(), 1);
        assert_eq!(m.materials[0].texture, "wall.bmp");
        assert_eq!(m.materials[0].diffuse, [1.0, 0.5, 0.25, 1.0]);
        // cut short: an error, not a panic
        assert!(parse_x(&b[..b.len() - 9]).is_err());
    }

    fn material_test_mesh(entries: &str) -> String {
        format!(
            "Mesh {{
            3; 0;0;0;, 1;0;0;, 0;1;0;;
            2; 3;0,1,2;, 3;0,2,1;;
            MeshMaterialList {{ 2; 2; 0,1;; {entries} }}
        }}"
        )
    }

    /// Traffic signals exported by 3ds Max put named materials before their frames and
    /// refer to them from each mesh. Ignoring those objects made the housings solid white.
    #[test]
    fn named_materials_keep_textures_colours_and_slot_order() {
        let mesh = material_test_mesh(
            r#"
            { SignalHousing }
            Material { 1;0;0;1;; 2; 0;0;0;; 0;0;0;; }
        "#,
        );
        let x = format!(
            r#"xof 0303txt 0032
            template Material {{ <3d82ab4d-62da-11cf-ab39-0020af71e433> FLOAT ignored; }}
            Material SignalHousing {{
                0.1;0.2;0.3;0.4;; 8; 0.5;0.6;0.7;; 0.2;0.3;0.4;;
                TextureFilename {{ "housing.bmp"; }}
            }}
            Frame Signal {{ {mesh} }}
        "#
        );
        let parsed = parse_x(x.as_bytes()).unwrap();
        assert_eq!(parsed.materials.len(), 2);
        let housing = &parsed.materials[0];
        assert_eq!(housing.texture, "housing.bmp");
        assert_eq!(housing.diffuse, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(housing.specular, [0.5, 0.6, 0.7]);
        assert_eq!(housing.emissive, [0.2, 0.3, 0.4]);
        assert_eq!(housing.specular_power, 8.0);
        assert_eq!(parsed.materials[1].diffuse, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn material_references_resolve_forward_across_frames_and_inline_definitions() {
        let first = material_test_mesh("{ LaterInFrame } { LaterInline }");
        let second = material_test_mesh(
            r#"
            { LaterInFrame }
            Material LaterInline {
                1;1;1;1;; 1; 0;0;0;; 1;1;1;;
                TextureFilename { "lamp.bmp"; }
            }
        "#,
        );
        let x = format!(
            r#"xof 0303txt 0032
            Frame First {{ {first} }}
            Frame Second {{
                Material LaterInFrame {{
                    1;1;1;1;; 1; 0;0;0;; 0;0;0;;
                    TextureFilename {{ "housing.bmp"; }}
                }}
                Frame Child {{ {second} }}
            }}
        "#
        );
        let parsed = parse_x(x.as_bytes()).unwrap();
        let textures: Vec<&str> = parsed
            .materials
            .iter()
            .map(|m| m.texture.as_str())
            .collect();
        assert_eq!(
            textures,
            ["housing.bmp", "lamp.bmp", "housing.bmp", "lamp.bmp"]
        );
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
    }

    #[test]
    fn missing_material_reference_preserves_following_slots() {
        let mesh = material_test_mesh(
            r#"
            { NotDeclared }
            Material { 1;0;0;1;; 1; 0;0;0;; 0;0;0;; }
        "#,
        );
        let parsed = parse_x(format!("xof 0303txt 0032\n{mesh}").as_bytes()).unwrap();
        assert_eq!(parsed.materials[0], Material::default());
        assert_eq!(parsed.materials[1].diffuse, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            parsed
                .triangles
                .iter()
                .map(|t| t.material)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }

    /// A Blender export: the root frame swaps Y and Z, the child frame scales, turns about
    /// its up axis and moves the mesh 5.5 m forward and 1.3 m up (the Atron terminal's frames).
    #[test]
    fn frame_matrices_are_row_major() {
        let x = "xof 0303txt 0032\n\
Frame Root {\n FrameTransformMatrix {\n 1.0, 0.0, 0.0, 0.0,\n 0.0,-0.0, 1.0, 0.0,\n 0.0, 1.0, 0.0, 0.0,\n 0.0, 0.0, 0.0, 1.0;;\n }\n\
 Frame Child {\n  FrameTransformMatrix {\n   0.0, 2.0, 0.0, 0.0,\n  -2.0, 0.0, 0.0, 0.0,\n   0.0, 0.0, 2.0, 0.0,\n  -0.5, 5.5, 1.3, 1.0;;\n  }\n\
  Mesh {\n   3;\n   1.0;0.0;0.0;,\n   0.0;1.0;0.0;,\n   0.0;0.0;1.0;;\n   1;\n   3;0,1,2;;\n\
   MeshNormals {\n    1;\n    1.0;0.0;0.0;;\n    1;\n    3;0,0,0;;\n   }\n  }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let p: Vec<Vec3> = m.vertices.iter().map(|v| v.position).collect();
        // Blender (x, y, z) -> child: (x, y, z)·M = (-2y, 2x, 2z) + (-0.5, 5.5, 1.3), then the
        // root swaps y and z.
        let want = [Vec3::new(-0.5, 1.3, 7.5), Vec3::new(-2.5, 1.3, 5.5), Vec3::new(-0.5, 3.3, 5.5)];
        for (a, b) in p.iter().zip(want) {
            assert!((*a - b).length() < 1e-5, "{p:?}");
        }
        // the +x normal turns to Blender +y, D3D +z
        assert!((m.vertices[0].normal - Vec3::Z).length() < 1e-5, "{:?}", m.vertices[0].normal);
    }

    /// A Blender export: the root frame swaps y and z, the child frame turns, scales and
    /// moves the mesh (Direct3D row-vector matrices).
    #[test]
    fn frames_move_and_turn_like_direct3d() {
        let x = "xof 0303txt 0032\nFrame Root {\n FrameTransformMatrix {\n 1,0,0,0,\n 0,0,1,0,\n 0,1,0,0,\n 0,0,0,1;;\n }\n Frame Box {\n FrameTransformMatrix {\n 2,0,0,0,\n 0,0,2,0,\n 0,-2,0,0,\n 0.5,0.25,1.5,1;;\n }\n Mesh {\n 3;\n 0;0;0;,\n 1;0;0;,\n 0;1;0;;\n 1;\n 3;0,1,2;;\n }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let p: Vec<Vec3> = m.vertices.iter().map(|v| v.position).collect();
        // v * child: (x, y, z) -> (2x + 0.5, -2z + 0.25, 2y + 1.5); then y and z swap
        assert!((p[0] - Vec3::new(0.5, 1.5, 0.25)).length() < 1e-5, "{:?}", p[0]);
        assert!((p[1] - Vec3::new(2.5, 1.5, 0.25)).length() < 1e-5, "{:?}", p[1]);
        assert!((p[2] - Vec3::new(0.5, 3.5, 0.25)).length() < 1e-5, "{:?}", p[2]);
    }

    /// A frame that scales unevenly (the Ruede Trafohaus: 1.5 x 0.75 x 0.85) keeps the
    /// normals on their faces: they go by the inverse transpose (the frame matrix itself put
    /// the transformer house's normals 3.8° off its walls).
    #[test]
    fn uneven_scale_keeps_normals_on_their_faces() {
        let x = "xof 0303txt 0032\nFrame Box {\n FrameTransformMatrix {\n 3,0,0,0,\n 0,1,0,0,\n 0,0,1,0,\n 0,0,0,1;;\n }\n Mesh {\n 3;\n 1;0;0;,\n 0;1;0;,\n 0;0;1;;\n 1;\n 3;0,1,2;;\n MeshNormals {\n 1;\n 0.57735;0.57735;0.57735;;\n 1;\n 3;0,0,0;;\n }\n }\n}\n";
        let m = parse_x(x.as_bytes()).unwrap();
        let [a, b, c] = [0, 1, 2].map(|i| m.vertices[i].position);
        let face = (b - a).cross(c - a).normalize();
        for v in &m.vertices {
            assert!(v.normal.normalize().dot(face).abs() > 0.9999, "{:?} vs {face:?}", v.normal);
        }
    }
}

#[cfg(test)]
mod damaged_tests {
    use super::*;

    #[test]
    fn damaged_files_are_errors_not_panics() {
        assert!(parse_x(b"xof 03").is_err());
        assert!(parse_x(b"xof 0302\xe9\xe9\xe9\xe9 0032 Mesh {").is_err());
        // a face naming vertex 7 of 3
        let m = parse_x(b"xof 0302txt 0032\nMesh m {\n3;\n0;0;0;,\n1;0;0;,\n0;1;0;;\n2;\n3;0,1,2;,\n3;0,1,7;;\n}\n").unwrap();
        assert_eq!(m.triangles.len(), 1);
    }
}
