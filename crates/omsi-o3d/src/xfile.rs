//! Minimal DirectX `.x` (text) mesh reader: enough for the OMSI helper meshes and the few
//! scenery objects that still ship `.x` files (frames, Mesh, MeshNormals, MeshTextureCoords,
//! MeshMaterialList, Material, TextureFilename). Binary `.x` is not used by OMSI content.

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
    if &bytes[8..12] != b"txt " {
        return Err(O3dError::XFile("binary .x not supported".into()));
    }
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
    loop {
        let tok = t.next().ok_or_else(|| O3dError::XFile("eof in mesh".into()))?;
        match tok {
            "}" => break,
            "MeshNormals" => {
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
            std::env::var_os("OMSI_ROOT").or_else(|| std::env::var_os("OMSI_TEST_CONTENT"))
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
