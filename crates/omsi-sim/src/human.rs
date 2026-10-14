//! Humans (`.hum`): skinned models posed procedurally from the joint positions of the
//! `[links]` block, like the original's `mc_human`. Bones are addressed by the engine ids
//! given by `[setbone]` (-2 … -14): thighs, shins, upper arms, forearms, hip, torso, head, hands.
//!
//! OMSI ships no animation clips, so every pose is made here. Every stock and add-on model
//! is stored in a T-pose (arms straight out at shoulder height) with the feet part of the
//! shin bones; the rig is measured from the `[links]` joints and the mesh itself (where the
//! soles, heels, toes and ankles are), and the feet get a bone of their own split off the
//! shins so that they can stay flat on the floor, strike with the heel and push off with
//! the toes.
//!
//! [`Pose`] is the per-person animation state, advanced every frame by [`Pose::advance`]
//! from a [`PoseInput`] (where the person stands, how fast they move, whether they sit,
//! pay, hold on or look at something); [`Pose::bones`] turns it into bone transforms and
//! [`skin`] deforms a mesh with them. Walking is a speed-driven gait with planted feet: a
//! foot on the floor stays where it was put (in the frame of the floor it stands on - the
//! ground or a bus) and the legs reach it by two-bone IK, so a stride always matches the
//! ground speed and nothing slides; the swinging foot flies to where the body will be when
//! it lands. The same stepping carries standing people through turns and shuffles, and the
//! arms reach the cash desk or a handrail by IK as well.

use anyhow::{Context, Result};
use glam::{Affine3A, DVec2, DVec3, Mat3A, Quat, Vec2, Vec3, Vec3A};
use omsi_content::Human;
use omsi_geometry::{mesh_from_o3d, MeshData};
use omsi_model::Model;
use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};

/// Engine bone ids of `[setbone]`.
pub const BONE_OS_L: i32 = -2;
pub const BONE_OS_R: i32 = -3;
pub const BONE_US_L: i32 = -4;
pub const BONE_US_R: i32 = -5;
pub const BONE_OA_L: i32 = -6;
pub const BONE_OA_R: i32 = -7;
pub const BONE_UA_L: i32 = -8;
pub const BONE_UA_R: i32 = -9;
pub const BONE_HIP: i32 = -10;
pub const BONE_MAIN: i32 = -11;
pub const BONE_HEAD: i32 = -12;
pub const BONE_HAND_L: i32 = -13;
pub const BONE_HAND_R: i32 = -14;

/// Bone transform slots: the thirteen engine bones (id -2 … -14 → slot 0 … 12), the two
/// feet split off the shins and the toes split off the feet.
pub const SLOTS: usize = 17;
const THIGH: [usize; 2] = [0, 1];
const SHIN: [usize; 2] = [2, 3];
const UPPER: [usize; 2] = [4, 5];
const FORE: [usize; 2] = [6, 7];
const HIP: usize = 8;
const MAIN: usize = 9;
const HEAD: usize = 10;
const HAND: [usize; 2] = [11, 12];
const FOOT: [usize; 2] = [13, 14];
const TOE: [usize; 2] = [15, 16];
/// Index 0 is the left side, 1 the right; this is the sign of x on that side.
const SIDE: [f32; 2] = [-1.0, 1.0];

/// The transform slot of an engine bone id.
pub fn slot_of(id: i32) -> Option<usize> {
    (BONE_HAND_R..=BONE_OS_L)
        .contains(&id)
        .then(|| (-id - 2) as usize)
}

fn bone_id_by_name(name: &str) -> Option<i32> {
    match name.trim().to_ascii_lowercase().replace('_', "").as_str() {
        "osl" => Some(BONE_OS_L),
        "osr" => Some(BONE_OS_R),
        "usl" => Some(BONE_US_L),
        "usr" => Some(BONE_US_R),
        "oal" => Some(BONE_OA_L),
        "oar" => Some(BONE_OA_R),
        "ual" => Some(BONE_UA_L),
        "uar" => Some(BONE_UA_R),
        "hip" => Some(BONE_HIP),
        "main" => Some(BONE_MAIN),
        "head" => Some(BONE_HEAD),
        "handl" => Some(BONE_HAND_L),
        "handr" => Some(BONE_HAND_R),
        _ => None,
    }
}

/// Joint positions in the model frame (x right, y forward, z up) of the right side.
#[derive(Debug, Clone, Copy)]
pub struct Joints {
    pub hip: Vec3,
    pub knee: Vec3,
    pub waist: Vec3,
    pub shoulder: Vec3,
    pub elbow: Vec3,
    pub neck: Vec3,
    pub hand: Vec3,
    pub finger: Vec3,
}

impl Joints {
    pub fn from_links(l: &[f32]) -> Joints {
        let g = |i: usize| l.get(i).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
        Joints {
            hip: Vec3::new(g(0), g(1), g(2)),
            knee: Vec3::new(g(3), g(4), g(5)),
            waist: Vec3::new(0.0, g(6), g(7)),
            shoulder: Vec3::new(g(8), g(9), g(10)),
            elbow: Vec3::new(g(11), g(12), g(13)),
            neck: Vec3::new(0.0, g(14), g(15)),
            hand: Vec3::new(g(16), g(17), g(18)),
            finger: Vec3::new(g(19), g(20), g(21)),
        }
    }
}

/// One vertex's bone influences: up to four (slot, weight) pairs, normalised.
#[derive(Debug, Clone, Copy, Default)]
pub struct Influence {
    pub n: u8,
    pub slot: [u8; 4],
    pub weight: [f32; 4],
}

/// A skinned mesh of a human with its bone → engine id table.
pub struct HumanMesh {
    pub data: MeshData,
    pub materials: Vec<omsi_o3d::Material>,
    /// Per bone of the file: engine id and (vertex, weight) list, as stored.
    pub bones: Vec<(i32, Vec<(u32, f32)>)>,
    /// Per vertex: the influences the skinning uses. The files list a vertex once per
    /// face corner (up to 24 times, always with the same weight), so the weights are taken
    /// once per bone and normalised; vertices near the sole also follow the foot.
    pub skin: Vec<Influence>,
    /// Per material: the model's `[matl_alpha]` for it (0 opaque, 1 alpha test, 2 blend) -
    /// the hair of the stock women and of man02 is an alpha-tested texture.
    pub alpha: Vec<i32>,
}

/// The skeleton of one human type, measured from its `[links]` and its mesh. Index 0 of the
/// pairs is the left side, 1 the right.
#[derive(Debug, Clone)]
pub struct Rig {
    pub hip: [Vec3; 2],
    pub knee: [Vec3; 2],
    pub ankle: [Vec3; 2],
    pub shoulder: [Vec3; 2],
    pub elbow: [Vec3; 2],
    pub wrist: [Vec3; 2],
    pub waist: Vec3,
    pub neck: Vec3,
    /// What the head turns about: at the neck's height, under the middle of the head. The
    /// `[links]` neck point lies at the back of the neck (13 cm behind the middle of the
    /// head of aXYZ man02), and the head turned about it swung off the collar - a broken
    /// neck whenever the passenger looked to the side.
    pub head_pivot: Vec3,
    /// Between the hip joints.
    pub pelvis: Vec3,
    pub thigh: f32,
    pub shin: f32,
    pub upper_arm: f32,
    pub forearm: f32,
    /// Lowest point of the soles.
    pub sole: f32,
    /// Ankle joint above the sole.
    pub ankle_h: f32,
    /// Heel, ball and toe tip along the foot from the ankle (m, heel negative), and the
    /// height of the toe joint above the sole.
    pub heel: f32,
    pub ball: f32,
    pub toe: f32,
    pub ball_h: f32,
    pub head_top: f32,
    /// Hip joint above the seat surface when sitting (`[seatheight]` names the rest).
    pub seat_lift: f32,
    /// Size relative to a 1.75 m adult.
    pub scale: f32,
    /// From `[walk_param]`: the step length at full stride (m) - half the file's first
    /// line, the stride ({schrittweite}, 1.4 by default) -, the arm swing and the hip sway.
    pub walk_step: f32,
    pub arm_swing: f32,
    pub hip_sway: f32,
}

impl Rig {
    fn measure(def: &Human, j: &Joints, meshes: &[HumanMesh]) -> Rig {
        let scale = if def.height > 0.5 {
            (def.height / 1.75).clamp(0.6, 1.3)
        } else {
            (j.neck.z / 1.55).clamp(0.6, 1.3)
        };
        let mirror = |v: Vec3| Vec3::new(-v.x, v.y, v.z);
        // the right shin's vertices (the foot is part of it)
        let mut shin: Vec<Vec3> = Vec::new();
        let mut top = f32::MIN;
        let (mut head_sum, mut head_n) = (Vec3::ZERO, 0u32);
        for m in meshes {
            for p in &m.data.positions {
                top = top.max(p.z);
            }
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize).any(|k| inf.slot[k] as usize == HEAD && inf.weight[k] > 0.5) {
                    head_sum += m.data.positions[i];
                    head_n += 1;
                }
            }
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize)
                    .any(|k| inf.slot[k] as usize == SHIN[1] && inf.weight[k] > 0.5)
                {
                    shin.push(m.data.positions[i]);
                }
            }
        }
        let knee = j.knee;
        let sole = shin.iter().map(|p| p.z).fold(f32::MAX, f32::min);
        let sole = if sole.is_finite() && sole < knee.z - 0.1 {
            sole
        } else {
            0.0
        };
        let ankle_h = (0.075 * scale).clamp(0.05, 0.1);
        let foot: Vec<&Vec3> = shin.iter().filter(|p| p.z < sole + 0.035 * scale).collect();
        let (mut heel_y, mut toe_y) = (knee.y - 0.07 * scale, knee.y + 0.19 * scale);
        if foot.len() >= 4 {
            let lo = foot.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            let hi = foot.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            if hi - lo > 0.12 * scale && hi - lo < 0.45 {
                heel_y = lo;
                toe_y = hi;
            }
        }
        // the ankle: the middle of the leg just above the foot
        let ring: Vec<&Vec3> = shin
            .iter()
            .filter(|p| (p.z - (sole + ankle_h + 0.02)).abs() < 0.025)
            .collect();
        let (mut ax, mut ay) = (knee.x, heel_y + 0.28 * (toe_y - heel_y));
        if ring.len() >= 4 {
            let n = ring.len() as f32;
            ax = ring.iter().map(|p| p.x).sum::<f32>() / n;
            ay = ring.iter().map(|p| p.y).sum::<f32>() / n;
        }
        // (a foot shorter than 13 cm - a child, a small model - has the ankle as far back as
        // it goes: the bounds crossed and the game stopped on the clamp, #138)
        let ay = ay.clamp(heel_y + 0.03, (toe_y - 0.1).max(heel_y + 0.03));
        let ax = if (ax - knee.x).abs() < 0.1 {
            ax
        } else {
            knee.x
        };
        let ankle = Vec3::new(ax, ay, sole + ankle_h);
        let foot_len = toe_y - heel_y;
        let hip = j.hip;
        let shoulder = j.shoulder;
        let elbow = if (j.elbow - j.shoulder).length() > 0.1 {
            j.elbow
        } else {
            j.shoulder + Vec3::new(0.26 * scale, 0.0, 0.0)
        };
        let wrist = if (j.hand - elbow).length() > 0.1 {
            j.hand
        } else {
            elbow + (elbow - shoulder).normalize() * 0.25 * scale
        };
        let head_top = if top.is_finite() && top > j.neck.z {
            top
        } else {
            j.neck.z + 0.2 * scale
        };
        let seat_lift = if def.seat_height > 0.2 {
            (hip.z - def.seat_height).clamp(0.05, 0.2)
        } else {
            0.1 * scale
        };
        let wp = def.walk_param;
        Rig {
            hip: [mirror(hip), hip],
            knee: [mirror(knee), knee],
            ankle: [mirror(ankle), ankle],
            shoulder: [mirror(shoulder), shoulder],
            elbow: [mirror(elbow), elbow],
            wrist: [mirror(wrist), wrist],
            waist: j.waist,
            neck: j.neck,
            head_pivot: if head_n > 20 {
                // the middle of the neck itself at the linked height: the vertices of the
                // mesh there (the neck's cross-section, under the head, not the collar)
                let c = head_sum / head_n as f32;
                let (mut sum, mut n) = (Vec3::ZERO, 0u32);
                for m in meshes {
                    for p in &m.data.positions {
                        if (p.z - j.neck.z).abs() < 0.03 && Vec2::new(p.x - c.x, p.y - c.y).length() < 0.09 {
                            sum += *p;
                            n += 1;
                        }
                    }
                }
                let at = if n >= 8 { sum / n as f32 } else { Vec3::new(c.x, j.neck.y + (c.y - j.neck.y) * 0.75, j.neck.z) };
                // (never further than 12 cm from the linked point)
                Vec3::new(j.neck.x, j.neck.y + (at.y - j.neck.y).clamp(-0.12, 0.12), j.neck.z)
            } else {
                j.neck
            },
            pelvis: Vec3::new(0.0, hip.y, hip.z),
            thigh: (knee - hip).length().max(0.2),
            shin: (ankle - knee).length().max(0.2),
            upper_arm: (elbow - shoulder).length(),
            forearm: (wrist - elbow).length(),
            sole,
            ankle_h,
            heel: heel_y - ay,
            ball: heel_y + 0.72 * foot_len - ay,
            toe: toe_y - ay,
            ball_h: (0.022 * scale).clamp(0.015, 0.03),
            head_top,
            seat_lift,
            scale,
            // (line 1 is the stride, hum+0x2d8: Omsi.exe's walk phase 0x626ae8 runs 2.0 per
            // two strides and sets a foot down at 0.2, 0.7, 1.2 and 1.7, one step per half a
            // stride; line 2, {upper_arm_beta}, is an angle of the arm)
            walk_step: 0.5 * if wp[0] > 0.3 { wp[0] } else { 1.4 },
            arm_swing: if wp[2] > 0.0 {
                wp[2].clamp(0.2, 1.6)
            } else {
                1.0
            },
            hip_sway: if wp[3] > 0.0 {
                wp[3].clamp(0.4, 2.2)
            } else {
                1.0
            },
        }
    }

    /// The toe joint of a foot in the rest pose.
    fn ball_joint(&self, side: usize) -> Vec3 {
        self.ankle[side] + Vec3::new(0.0, self.ball, self.ball_h - self.ankle_h)
    }

    /// Hip joint height above the sole with the legs straight.
    pub fn leg(&self) -> f32 {
        self.hip[1].z - self.sole
    }

    /// Where a standing foot rests (the floor point under its ankle), in the model frame.
    fn rest_foot(&self, side: usize) -> Vec3 {
        Vec3::new(self.ankle[side].x, self.ankle[side].y, self.sole)
    }

    /// How far in front of a seat's `[passpos]` (the hip) a person stands before sitting
    /// down, which is also where the feet stay while seated.
    pub fn seat_front(&self) -> f32 {
        (self.thigh * 0.85).clamp(0.25, 0.4)
    }

    /// Steps per second at `speed`, as Omsi.exe times the walk (0x626ae8): the stride is
    /// the full one from 1.2 m/s on and shortens with the speed below that, so the steps
    /// keep one pace when walking slowly; never with a step longer than the legs allow.
    pub fn cadence(&self, speed: f32) -> f32 {
        let v = speed.max(0.05);
        let f = v / (self.walk_step * (v / 1.2).min(1.0));
        f.max(v / (0.8 * self.leg())).min(3.4)
    }
}

pub struct HumanType {
    pub def: Human,
    /// The skeleton as Omsi.exe animates it (see [`crate::human_omsi`]).
    pub omsi: crate::human_omsi::OmsiRig,
    pub model: Model,
    pub model_dir: PathBuf,
    pub meshes: Vec<HumanMesh>,
    pub joints: Joints,
    pub rig: Rig,
    /// Clothing variants: the `[item]`s of the `.cti` files in the model's `[CTC]` folder
    /// (the stock people have two or three each), which replace the `[CTCTexture]`
    /// default. Variant 0 is the default texture, variant n the scheme n-1.
    pub variants: Vec<crate::vehicle::PaintScheme>,
    /// Folders the textures may lie in besides the usual ones: the `[CTC]` folders and
    /// the sub-folders of the human's own folder (see [`HumanType::texture_dirs`]).
    extra_dirs: Vec<PathBuf>,
}

impl HumanType {
    pub fn load(path: &Path) -> Result<HumanType> {
        let def = Human::load(path).with_context(|| format!("loading {}", path.display()))?;
        let dir = path.parent().unwrap_or(Path::new(""));
        let model_path = omsi_cfg::resolve_path(dir, &def.model);
        let model = Model::load(&model_path)
            .with_context(|| format!("loading {}", model_path.display()))?;
        let model_dir = model_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let mut meshes = Vec::new();
        if !model.lods.is_empty() {
            for md in model.lod_meshes(0) {
                let p = omsi_cfg::resolve_path(&model_dir, &md.file);
                let m =
                    omsi_o3d::load_mesh(&p).with_context(|| format!("loading {}", p.display()))?;
                let bones: Vec<(i32, Vec<(u32, f32)>)> = m
                    .bones
                    .iter()
                    .map(|b| {
                        let id = md
                            .bones
                            .iter()
                            .find(|(n, _)| n.eq_ignore_ascii_case(&b.name))
                            .map(|(_, id)| *id)
                            .or_else(|| bone_id_by_name(&b.name))
                            .unwrap_or(0);
                        (id, b.weights.iter().map(|w| (w.vertex, w.weight)).collect())
                    })
                    .collect();
                let data = mesh_from_o3d(&m);
                let skin = influences(&data, &bones);
                // `[matl] <texture> <n>` names the n-th material with that texture
                let mut seen: Vec<String> = Vec::new();
                let alpha = m
                    .materials
                    .iter()
                    .map(|mat| {
                        let name = mat.texture.trim().to_ascii_lowercase();
                        let n = seen.iter().filter(|t| **t == name).count() as i32;
                        seen.push(name);
                        md.materials
                            .iter()
                            .find(|d| d.texture.trim().eq_ignore_ascii_case(mat.texture.trim()) && d.index == n)
                            .map(|d| d.alpha)
                            .unwrap_or(0)
                    })
                    .collect();
                meshes.push(HumanMesh {
                    data,
                    materials: m.materials.clone(),
                    bones,
                    skin,
                    alpha,
                });
            }
        }
        let mut joints = Joints::from_links(&def.links);
        fit_leg_joints(&mut joints, &meshes, path);
        let rig = Rig::measure(&def, &joints, &meshes);
        for m in meshes.iter_mut() {
            split_feet(m, &rig);
        }
        // The `[CTC]` folder is relative to the .hum file's folder (`Texture\man02` is
        // Humans/Other/texture/man02). An add-on that put that folder straight into its own
        // folder instead (GSPNS: Humans/GSPNS/man02, with the default texture in it too) is
        // found by the folder's last name; OMSI would show that person untextured.
        let mut variants = Vec::new();
        let mut extra_dirs: Vec<PathBuf> = Vec::new();
        for c in &model.ctc {
            let mut d = omsi_cfg::resolve_path(dir, &c.path);
            if !omsi_cfg::vfs::is_dir(&d) {
                let last = c.path.trim().trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or("");
                if !last.is_empty() {
                    let alt = omsi_cfg::resolve_path(dir, last);
                    if omsi_cfg::vfs::is_dir(&alt) {
                        d = alt;
                    }
                }
            }
            variants.extend(crate::vehicle::load_paint_schemes(&d));
            extra_dirs.push(d);
        }
        // the sub-folders of the human's folder and of its texture folder, last
        for base in [dir.to_path_buf(), omsi_cfg::resolve_path(dir, "texture")] {
            for (n, is_dir) in omsi_cfg::vfs::list_dir(&base).unwrap_or_default() {
                if is_dir && !n.to_string_lossy().eq_ignore_ascii_case("model") {
                    let d = base.join(n);
                    if !extra_dirs.contains(&d) {
                        extra_dirs.push(d);
                    }
                }
            }
        }
        Ok(HumanType {
            omsi: crate::human_omsi::OmsiRig::new(&def),
            joints,
            rig,
            def,
            model,
            model_dir,
            meshes,
            variants,
            extra_dirs,
        })
    }

    pub fn texture_dirs(&self, root: &Path) -> Vec<PathBuf> {
        let human_dir = self.model_dir.parent().unwrap_or(&self.model_dir);
        let hum_dir = self.def.path.parent().unwrap_or(human_dir);
        let mut dirs = vec![
            omsi_cfg::resolve_path(&self.model_dir, "texture"),
            self.model_dir.clone(),
            omsi_cfg::resolve_path(human_dir, "texture"),
            human_dir.to_path_buf(),
            omsi_cfg::resolve_path(hum_dir, "texture"),
            hum_dir.to_path_buf(),
            omsi_cfg::resolve_path(&self.model_dir, "..\\texture"),
            omsi_cfg::resolve_path(root, "Texture"),
        ];
        dirs.dedup();
        dirs.extend(self.extra_dirs.iter().cloned());
        dirs
    }

    /// The texture a material shows in clothing variant `variant` (0 = the default):
    /// the file name and the folder to look in first.
    pub fn variant_texture<'a>(&'a self, texture: &'a str, variant: usize) -> (&'a str, Option<&'a Path>) {
        let Some(s) = variant.checked_sub(1).and_then(|k| self.variants.get(k)) else {
            return (texture, None);
        };
        for (ctc_name, file) in &s.textures {
            for (name, default) in &self.model.ctc_textures {
                if name.eq_ignore_ascii_case(ctc_name)
                    && default.trim().eq_ignore_ascii_case(texture.trim())
                {
                    return (file.as_str(), Some(s.dir.as_path()));
                }
            }
        }
        (texture, None)
    }
}

/// A leg joint of `[links]` that lies outside the leg it belongs to is the author's slip:
/// the GSPNS man04 and man041 give the right hip at x = 0.6 m and the knee at 0.83 m, where
/// the mesh's thigh is at 0.08. OMSI's rotations about those points hardly show it (the
/// legs swing about the x axis, along which the error lies), but a leg rig measured from
/// them reaches its foot 0.8 m out to the side and crosses the legs. Such a coordinate is
/// taken from the mesh instead: the middle of the limb's vertices at the joint's height.
fn fit_leg_joints(j: &mut Joints, meshes: &[HumanMesh], path: &Path) {
    let verts = |slots: &[usize]| -> Vec<Vec3> {
        let mut out = Vec::new();
        for m in meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                if (0..inf.n as usize).any(|k| slots.contains(&(inf.slot[k] as usize)) && inf.weight[k] > 0.5) {
                    out.push(m.data.positions[i]);
                }
            }
        }
        out
    };
    let thigh = verts(&[THIGH[1]]);
    let knee_region = verts(&[THIGH[1], SHIN[1]]);
    let fit = |joint: &mut Vec3, limb: &[Vec3], what: &str| {
        if limb.len() < 8 {
            return;
        }
        let near: Vec<&Vec3> = limb.iter().filter(|p| (p.z - joint.z).abs() < 0.06).collect();
        let near: Vec<&Vec3> = if near.len() >= 4 { near } else { limb.iter().collect() };
        let n = near.len() as f32;
        let (lo, hi) = limb.iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        let mut fixed = false;
        if joint.x < lo.x - 0.05 || joint.x > hi.x + 0.05 {
            joint.x = near.iter().map(|p| p.x).sum::<f32>() / n;
            fixed = true;
        }
        if joint.y < lo.y - 0.05 || joint.y > hi.y + 0.05 {
            joint.y = near.iter().map(|p| p.y).sum::<f32>() / n;
            fixed = true;
        }
        if fixed {
            log::info!(
                "{}: the {what} of [links] lies outside the leg; using ({:.2}, {:.2}, {:.2}) from the mesh",
                path.display(),
                joint.x,
                joint.y,
                joint.z
            );
        }
    };
    fit(&mut j.hip, &thigh, "hip");
    fit(&mut j.knee, &knee_region, "knee");
}

/// Per-vertex influences from the file's bone lists: each bone counted once per vertex,
/// the strongest four kept, normalised. A vertex no bone claims follows the torso.
fn influences(data: &MeshData, bones: &[(i32, Vec<(u32, f32)>)]) -> Vec<Influence> {
    let n = data.positions.len();
    let mut per: Vec<Vec<(u8, f32)>> = vec![Vec::new(); n];
    for (id, weights) in bones {
        let Some(slot) = slot_of(*id) else { continue };
        for (v, w) in weights {
            let Some(list) = per.get_mut(*v as usize) else {
                continue;
            };
            if !w.is_finite() || *w <= 0.0 {
                continue;
            }
            match list.iter_mut().find(|e| e.0 as usize == slot) {
                Some(e) => e.1 = e.1.max(*w),
                None => list.push((slot as u8, *w)),
            }
        }
    }
    per.into_iter()
        .map(|mut list| {
            list.sort_by(|a, b| b.1.total_cmp(&a.1));
            list.truncate(4);
            let total: f32 = list.iter().map(|e| e.1).sum();
            if total < 1e-4 {
                return Influence {
                    n: 1,
                    slot: [MAIN as u8, 0, 0, 0],
                    weight: [1.0, 0.0, 0.0, 0.0],
                };
            }
            let mut inf = Influence {
                n: list.len() as u8,
                ..Default::default()
            };
            for (k, e) in list.iter().enumerate() {
                inf.slot[k] = e.0;
                inf.weight[k] = e.1 / total;
            }
            inf
        })
        .collect()
}

/// Move `share` of a vertex's weight on slot `from` to slot `to`.
fn move_weight(inf: &mut Influence, from: usize, to: usize, share: f32) {
    let Some(k) = (0..inf.n as usize).find(|&k| inf.slot[k] as usize == from) else {
        return;
    };
    let moved = inf.weight[k] * share;
    if moved < 1e-3 {
        return;
    }
    if let Some(e) = (0..inf.n as usize).find(|&e| inf.slot[e] as usize == to) {
        inf.weight[k] -= moved;
        inf.weight[e] += moved;
    } else if (inf.n as usize) < 4 {
        inf.weight[k] -= moved;
        let e = inf.n as usize;
        inf.slot[e] = to as u8;
        inf.weight[e] = moved;
        inf.n += 1;
    } else if share > 0.5 {
        // no room: all of it
        inf.slot[k] = to as u8;
    }
}

/// Give the feet and the toes bones of their own: shin weight below the ankle moves to the
/// foot and foot weight beyond the ball to the toes, with short blends around the joints.
/// A skirt is weighted to the thighs in the files and would fly up with a swinging leg like
/// a lap: its cloth (thigh vertices well outside the leg) follows the hips in part.
fn split_feet(m: &mut HumanMesh, rig: &Rig) {
    let top = rig.sole + rig.ankle_h + 0.025;
    let bottom = rig.sole + rig.ankle_h - 0.03;
    for (i, inf) in m.skin.iter_mut().enumerate() {
        let v = m.data.positions[i];
        if v.z >= top {
            continue;
        }
        let to_foot = smoothstep(top, bottom, v.z);
        for side in 0..2 {
            move_weight(inf, SHIN[side], FOOT[side], to_foot);
            let along = v.y - rig.ankle[side].y;
            let to_toe = smoothstep(rig.ball - 0.012, rig.ball + 0.02, along);
            if to_toe > 0.0 {
                move_weight(inf, FOOT[side], TOE[side], to_toe);
            }
        }
    }
}

/// What a human is doing; drives the pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    Stand,
    Walk,
    Sit,
    /// Standing at the cash desk with the right hand held out.
    Pay,
}

/// What the animation needs to know about a person this frame. Points and directions are
/// in the person's model frame (x right, y forward, z up, origin at the feet) unless they
/// say otherwise.
#[derive(Clone, Copy)]
pub struct PoseInput<'a> {
    pub activity: Activity,
    /// Where the person stands and which way they face, in the frame of the floor they are
    /// on (the world, or a bus): position of the feet, heading in degrees (OMSI's, 0 = +y,
    /// clockwise).
    pub origin: DVec3,
    pub heading: f64,
    /// Which floor frame `origin` is in (0 the ground, else a bus); a change keeps the feet
    /// where they are and re-expresses them in the new frame.
    pub frame: u64,
    /// Ground velocity in that frame (m/s).
    pub velocity: DVec2,
    /// While sitting: the seat point (`[passpos]`, the hip) in the model frame.
    pub seat: Option<Vec3>,
    /// Something to look at.
    pub look: Option<Vec3>,
    /// Where the right hand reaches (the cash desk).
    pub reach: Option<Vec3>,
    /// Where both hands hold on (left, right): the driver's hands on the steering wheel.
    /// Followed as given every frame, no easing (the caller moves them).
    pub grips: Option<[Vec3; 2]>,
    /// How each gripping hand lies (left, right): the direction from the wrist to the
    /// knuckles and the direction the palm faces, model frame. Without it the hand goes on
    /// straight from the forearm.
    pub grip_frames: Option<[(Vec3, Vec3); 2]>,
    /// Extra forward lean towards the grips (degrees): a driver whose wheel is far off leans
    /// to it from the seat instead of leaving it.
    pub grip_lean: f32,
    /// Hold on to a handrail (0 … 1).
    pub hold: f32,
    /// Acceleration of the floor under the person (a moving bus), model frame, m/s².
    pub sway: Vec2,
    /// Floor height at a point of the floor frame, when the floor is not flat.
    pub floor: Option<&'a dyn Fn(DVec2) -> Option<f64>>,
}

impl Default for PoseInput<'_> {
    fn default() -> Self {
        PoseInput {
            activity: Activity::Stand,
            origin: DVec3::ZERO,
            heading: 0.0,
            frame: 0,
            velocity: DVec2::ZERO,
            seat: None,
            look: None,
            reach: None,
            grips: None,
            grip_frames: None,
            grip_lean: 0.0,
            hold: 0.0,
            sway: Vec2::ZERO,
            floor: None,
        }
    }
}

/// One foot: planted on the floor (frame coordinates of the floor point under the ankle
/// and the foot's heading), or in the air on its way to the next place.
#[derive(Debug, Clone, Copy)]
struct Foot {
    planted: bool,
    /// Floor point under the ankle, and heading (degrees), in the floor frame.
    pos: DVec3,
    yaw: f64,
    /// Swing: where it left (ankle position and pitch at lift-off) and where it goes.
    from_ankle: DVec3,
    from_yaw: f64,
    from_pitch: f32,
    from_floor: f64,
    to: DVec3,
    to_yaw: f64,
    /// Floor height sampled at `to` (and where that was).
    to_floor: f64,
    to_sampled: DVec2,
    /// The landing height the swing aims at: follows `to_floor` quickly but never in a
    /// jump (the target crossing the edge of a step changed it by 30-40 cm mid-swing and
    /// the foot snapped up with it), and how long a landing has waited for it.
    land_z: f64,
    land_wait: f32,
    /// Swing progress 0 … 1 and length (s) of a step not driven by the gait.
    t: f32,
    /// A walking step: the gait's swing progress when the foot left (the swing's own
    /// progress runs from there to the landing).
    t0: f32,
    dur: f32,
    /// This step belongs to the walk (heel strike and push-off) rather than a shuffle.
    walk: bool,
    lift: f32,
}

impl Foot {
    fn new() -> Foot {
        Foot {
            planted: true,
            pos: DVec3::ZERO,
            yaw: 0.0,
            from_ankle: DVec3::ZERO,
            t0: 0.0,
            from_yaw: 0.0,
            from_pitch: 0.0,
            from_floor: 0.0,
            to: DVec3::ZERO,
            to_yaw: 0.0,
            to_floor: 0.0,
            to_sampled: DVec2::splat(f64::MAX),
            land_z: 0.0,
            land_wait: 0.0,
            t: 0.0,
            dur: 0.4,
            walk: false,
            lift: 0.05,
        }
    }
}

/// A person's animation state.
#[derive(Debug, Clone)]
pub struct Pose {
    rng: u32,
    init: bool,
    frame: u64,
    origin: DVec3,
    heading: f64,
    /// Gait cycle (0 … 1, the left heel strikes at 0) and whether the gait drives the feet.
    phase: f32,
    gait: bool,
    /// Walking weight for the upper body (0 … 1), smoothed ground speed (m/s), forward
    /// acceleration (m/s²) and turning rate (deg/s).
    walk: f32,
    speed: f32,
    accel: f32,
    turn: f32,
    vel: DVec2,
    feet: [Foot; 2],
    /// Standing offsets of the feet (a little different each time somebody settles).
    fidget: [Vec2; 2],
    fidget_t: f32,
    /// 0 standing … 1 seated, and the seat.
    sit: f32,
    seat: Vec3,
    reach: f32,
    reach_at: Vec3,
    /// Both hands on a steering wheel: how far (0 … 1) and where.
    grip: f32,
    grip_at: [Vec3; 2],
    /// How the gripping hands lie (see `PoseInput::grip_frames`).
    grip_frame: Option<[(Vec3, Vec3); 2]>,
    /// Extra lean towards the wheel (degrees, eased).
    grip_extra: f32,
    hold: f32,
    /// Head direction (degrees: right, up) and where a glance goes and for how long.
    head: Vec2,
    glance: Vec2,
    glance_t: f32,
    /// Weight shift: where the pelvis leans (m) and until when.
    shift: f32,
    shift_to: f32,
    shift_t: f32,
    breath: f32,
    /// Balance against the floor's acceleration: lean (m/s² equivalent) and its rate.
    lean: Vec2,
    lean_v: Vec2,
    /// Pelvis height offset kept from the last frame (rises are smoothed).
    drop: f32,
    /// Floor under the body relative to the origin (smoothed), from the planted feet.
    body_floor: f32,
    /// Per-person style: arm hang, head tilt, stance width.
    style: [f32; 4],
    /// Steps taken to catch up with a foot left behind, and frames a foot had to be pulled
    /// in (for the tests).
    catch_ups: u32,
    shuffles: u32,
    /// A foot was put down in the last [`Pose::advance`] - one footstep sound.
    landed: bool,
}

/// Bone transforms and the joint positions they put the limbs at (model frame).
#[derive(Debug, Clone)]
pub struct Posed {
    pub bones: [Affine3A; SLOTS],
    pub hip: [Vec3; 2],
    pub knee: [Vec3; 2],
    pub ankle: [Vec3; 2],
    pub elbow: [Vec3; 2],
    pub wrist: [Vec3; 2],
    /// Lowest point of each sole (heel, ball or toe).
    pub sole: [f32; 2],
    /// Knee flexion and ankle flexion (degrees, toes up positive) per side.
    pub knee_flex: [f32; 2],
    pub ankle_flex: [f32; 2],
    /// How far each ankle stayed from where it should be (unreachable targets).
    pub leg_miss: [f32; 2],
    pub neck: Vec3,
    /// All transforms are finite (else `bones` is the rest pose and the caller should keep
    /// the last good mesh).
    pub ok: bool,
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Move `x` towards `to` by at most `step`.
fn approach(x: f32, to: f32, step: f32) -> f32 {
    if (to - x).abs() <= step {
        to
    } else {
        x + step * (to - x).signum()
    }
}

/// Exponential smoothing factor for time constant `tau`.
fn ease_k(dt: f32, tau: f32) -> f32 {
    1.0 - (-dt / tau.max(1e-3)).exp()
}

fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

/// Rotation about z turning +y towards +x by `deg` (OMSI's heading sense).
fn yaw_quat(deg: f32) -> Quat {
    Quat::from_rotation_z(-deg.to_radians())
}

/// A frame from a bone axis and a second direction (made perpendicular to it).
fn basis(axis: Vec3, front: Vec3) -> Mat3A {
    let a = axis.normalize_or(Vec3::Z);
    let mut f = front - a * a.dot(front);
    if f.length_squared() < 1e-8 {
        f = a.any_orthonormal_vector();
    }
    let f = f.normalize();
    Mat3A::from_cols(a.into(), f.into(), a.cross(f).into())
}

/// The rotation taking the rest frame (axis0, front0) onto (axis1, front1).
fn bone_rot(axis0: Vec3, front0: Vec3, axis1: Vec3, front1: Vec3) -> Mat3A {
    basis(axis1, front1) * basis(axis0, front0).transpose()
}

/// A rigid bone transform: rotation `r` about the rest joint `rest`, which moves to `at`.
fn joint_xf(rest: Vec3, at: Vec3, r: Mat3A) -> Affine3A {
    Affine3A::from_mat3_translation(r.into(), at - Vec3::from(r * Vec3A::from(rest)))
}

fn about(pivot: Vec3, q: Quat) -> Affine3A {
    Affine3A::from_translation(pivot) * Affine3A::from_quat(q) * Affine3A::from_translation(-pivot)
}

/// Two-bone IK: from `root` with bone lengths `l1`, `l2` to `target`, bending towards
/// `pole`. Returns the middle joint, the end joint actually reached, and the bend-plane
/// normal (the hinge).
fn two_bone(root: Vec3, l1: f32, l2: f32, target: Vec3, pole: Vec3) -> (Vec3, Vec3, Vec3) {
    let d = target - root;
    let len = d.length();
    let dir = if len > 1e-5 { d / len } else { -Vec3::Z };
    let reach = (l1 + l2) * 0.9995;
    // (a bone next to nothing puts the floor over the reach: clamp would panic)
    let dist = len.clamp(((l1 - l2).abs() + 1e-3).min(reach), reach);
    let mut n = dir.cross(pole);
    if n.length_squared() < 1e-8 {
        n = dir.cross(Vec3::Y);
        if n.length_squared() < 1e-8 {
            n = Vec3::X;
        }
    }
    let n = n.normalize();
    let cos1 = ((l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)).clamp(-1.0, 1.0);
    let a1 = cos1.acos();
    let mid = root + Quat::from_axis_angle(n, a1) * dir * l1;
    let end = root + dir * dist;
    (mid, end, n)
}

impl Pose {
    pub fn new(seed: u32) -> Pose {
        let mut p = Pose {
            rng: seed.wrapping_mul(0x9E37_79B9) | 1,
            init: false,
            frame: 0,
            origin: DVec3::ZERO,
            heading: 0.0,
            phase: 0.0,
            gait: false,
            walk: 0.0,
            speed: 0.0,
            accel: 0.0,
            turn: 0.0,
            vel: DVec2::ZERO,
            feet: [Foot::new(), Foot::new()],
            fidget: [Vec2::ZERO; 2],
            fidget_t: 0.0,
            sit: 0.0,
            seat: Vec3::ZERO,
            reach: 0.0,
            reach_at: Vec3::new(0.3, 0.5, 1.1),
            grip: 0.0,
            grip_at: [Vec3::new(-0.2, 0.4, 1.0), Vec3::new(0.2, 0.4, 1.0)],
            grip_frame: None,
            grip_extra: 0.0,
            hold: 0.0,
            head: Vec2::ZERO,
            glance: Vec2::ZERO,
            glance_t: 0.0,
            shift: 0.0,
            shift_to: 0.0,
            shift_t: 0.0,
            breath: 0.0,
            lean: Vec2::ZERO,
            lean_v: Vec2::ZERO,
            drop: 0.0,
            body_floor: 0.0,
            style: [0.0; 4],
            catch_ups: 0,
            shuffles: 0,
            landed: false,
        };
        for k in 0..4 {
            p.style[k] = p.rand() * 2.0 - 1.0;
        }
        p.breath = p.rand() * TAU;
        p.glance_t = p.rand() * 3.0;
        p.shift_t = p.rand() * 5.0;
        p.fidget_t = 8.0 + p.rand() * 10.0;
        p
    }

    fn rand(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }

    /// 0 standing … 1 seated.
    pub fn sit_amount(&self) -> f32 {
        self.sit
    }

    /// Sitting down or getting up is under way: the person should not walk off yet.
    pub fn settling(&self) -> bool {
        self.sit > 0.02 && self.sit < 0.98
    }

    /// The state in a line, for `OMSI_DEBUG_POSE`.
    pub fn describe(&self) -> String {
        let foot = |f: &Foot| {
            if f.planted {
                format!("down at ({:.2}, {:.2}, {:.2})", f.pos.x, f.pos.y, f.pos.z)
            } else {
                format!(
                    "in the air {:.0}%{}",
                    f.t * 100.0,
                    if f.walk { " (walk)" } else { "" }
                )
            }
        };
        format!(
            "origin ({:.2}, {:.2}, {:.2}) heading {:.0} turning {:.0}/s frame {} speed {:.2} gait {} walk {:.2} phase {:.2} sit {:.2} reach {:.2} hold {:.2} head ({:.0}, {:.0}) floor {:.2} drop {:.3} catch-ups {} shuffles {}; left {}; right {}",
            self.origin.x, self.origin.y, self.origin.z, self.heading, self.turn, self.frame, self.speed, self.gait, self.walk, self.phase, self.sit, self.reach, self.hold, self.head.x, self.head.y, self.body_floor, self.drop, self.catch_ups, self.shuffles, foot(&self.feet[0]), foot(&self.feet[1])
        )
    }

    /// Whether foot `side` (0 left, 1 right) is on the floor.
    pub fn planted(&self, side: usize) -> bool {
        self.feet[side.min(1)].planted
    }

    /// A foot was put down during the last [`Pose::advance`]: one footfall, for the step
    /// sound. (The gait plants a foot only while walking, so standing still is silent.)
    pub fn landed(&self) -> bool {
        self.landed
    }

    /// Nothing quick is going on (standing or sitting still: breathing, shifting, glancing),
    /// so a lower update rate will not show.
    pub fn calm(&self) -> bool {
        !self.gait
            && self.walk < 0.05
            && self.feet.iter().all(|f| f.planted)
            && (self.sit == 0.0 || self.sit == 1.0)
            && (self.reach == 0.0 || self.reach == 1.0)
            && (self.hold == 0.0 || self.hold == 1.0)
    }

    /// Quick steps taken so far to catch up with a foot left behind.
    pub fn catch_ups(&self) -> u32 {
        self.catch_ups
    }

    fn to_local(&self, p: DVec3) -> Vec3 {
        let d = p - self.origin;
        let h = self.heading.to_radians();
        let (s, c) = (h.sin(), h.cos());
        // inverse of local → frame (x' = x cos + y sin, y' = -x sin + y cos)
        Vec3::new(
            (d.x * c - d.y * s) as f32,
            (d.x * s + d.y * c) as f32,
            d.z as f32,
        )
    }

    fn place(origin: DVec3, heading: f64, l: Vec3) -> DVec3 {
        let h = heading.to_radians();
        let (s, c) = (h.sin(), h.cos());
        let (x, y) = (l.x as f64, l.y as f64);
        origin + DVec3::new(x * c + y * s, -x * s + y * c, l.z as f64)
    }

    fn sample_floor(input: &PoseInput, at: DVec2, fallback: f64) -> f64 {
        match input.floor {
            Some(f) => f(at)
                .filter(|z| (z - fallback).abs() < 1.2)
                .unwrap_or(fallback),
            None => fallback,
        }
    }

    /// Where a foot's resting place is in the floor frame for the body at `origin`/`heading`.
    fn rest_target(
        &self,
        rig: &Rig,
        side: usize,
        origin: DVec3,
        heading: f64,
        walking: bool,
    ) -> (DVec3, f64) {
        let r = rig.rest_foot(side);
        let (x, y) = if walking {
            let stride = self.speed / (rig.cadence(self.speed) * 0.5);
            let beta = stance_fraction(self.speed);
            (r.x * 0.62, r.y + 0.4 * beta * stride)
        } else {
            (
                r.x + SIDE[side] * 0.012 * self.style[2] + self.fidget[side].x,
                r.y + self.fidget[side].y,
            )
        };
        let toe_out = SIDE[side] as f64 * if walking { 5.0 } else { 9.0 };
        (
            Self::place(origin, heading, Vec3::new(x, y, 0.0)),
            heading + toe_out,
        )
    }

    fn reset_feet(&mut self, rig: &Rig, input: &PoseInput) {
        for side in 0..2 {
            let (p, yaw) = self.rest_target(rig, side, input.origin, input.heading, false);
            let f = &mut self.feet[side];
            *f = Foot::new();
            f.pos = DVec3::new(p.x, p.y, input.origin.z);
            f.yaw = yaw;
        }
        self.body_floor = 0.0;
    }

    /// Ankle of a planted foot pitched by `pitch` degrees (toes up positive), floor frame.
    fn stance_ankle(rig: &Rig, f: &Foot, pitch: f32) -> DVec3 {
        let h = f.yaw.to_radians();
        let (s, c) = (h.sin(), h.cos());
        let dir = DVec3::new(s, c, 0.0);
        let p = pitch.to_radians();
        let ah = rig.ankle_h as f64;
        // pivot on the heel when the toes are up, on the toe joint when the heel is up (the
        // toes stay flat)
        let (pivot, ph) = if p > 0.0 {
            (rig.heel as f64, 0.0)
        } else {
            (rig.ball as f64, rig.ball_h as f64)
        };
        let (sp, cp) = (p.sin() as f64, p.cos() as f64);
        // the ankle seen from the pivot, turned by the pitch
        let (a, u) = (-pivot, ah - ph);
        let along = a * cp - u * sp;
        let up = a * sp + u * cp;
        f.pos + dir * (pivot + along) + DVec3::Z * (ph + up)
    }

    /// Advance the animation by `dt` seconds.
    pub fn advance(&mut self, rig: &Rig, input: &PoseInput, dt: f32) {
        // nothing sensible to follow: keep the last state
        if !input.origin.is_finite() || !input.heading.is_finite() || !dt.is_finite() {
            return;
        }
        let mut clean = *input;
        if !clean.velocity.is_finite() {
            clean.velocity = DVec2::ZERO;
        }
        if !clean.sway.is_finite() {
            clean.sway = Vec2::ZERO;
        }
        clean.seat = clean.seat.filter(|v| v.is_finite());
        clean.look = clean.look.filter(|v| v.is_finite());
        clean.reach = clean.reach.filter(|v| v.is_finite());
        clean.grips = clean.grips.filter(|g| g[0].is_finite() && g[1].is_finite());
        if !clean.hold.is_finite() {
            clean.hold = 0.0;
        }
        let input = &clean;
        let dt = dt.clamp(0.0, 0.25);
        self.landed = false;
        // a new floor frame (boarding, getting off): keep the feet where they are
        if self.init && input.frame != self.frame {
            let dyaw = angle_diff(self.heading, input.heading);
            for k in 0..2 {
                let f = self.feet[k];
                let (lp, la, lt) = (
                    self.to_local(f.pos),
                    self.to_local(f.from_ankle),
                    self.to_local(f.to),
                );
                let floor = f.to_floor - self.origin.z + input.origin.z;
                let from_floor = f.from_floor - self.origin.z + input.origin.z;
                let land_z = f.land_z - self.origin.z + input.origin.z;
                let g = &mut self.feet[k];
                g.pos = Self::place(input.origin, input.heading, lp);
                g.from_ankle = Self::place(input.origin, input.heading, la);
                g.to = Self::place(input.origin, input.heading, lt);
                g.to_floor = floor;
                g.from_floor = from_floor;
                g.land_z = land_z;
                g.yaw += dyaw;
                g.from_yaw += dyaw;
                g.to_yaw += dyaw;
                g.to_sampled = DVec2::splat(f64::MAX);
            }
            self.frame = input.frame;
            self.origin = input.origin;
            self.heading = input.heading;
            self.vel = input.velocity;
        }
        // first frame, or a jump no step could follow: stand where they are
        let jumped = self.init
            && ((input.origin - self.origin).truncate().length() > 1.5
                || (input.origin.z - self.origin.z).abs() > 1.0);
        if !self.init || jumped {
            self.init = true;
            self.frame = input.frame;
            self.origin = input.origin;
            self.heading = input.heading;
            self.vel = input.velocity;
            self.reset_feet(rig, input);
            if input.activity == Activity::Sit && input.seat.is_some() {
                self.sit = 1.0;
            }
            self.gait = false;
        }
        let turned = angle_diff(self.heading, input.heading) as f32;
        if dt > 1e-5 {
            self.turn += (turned / dt - self.turn) * ease_k(dt, 0.15);
            let v_now = input.velocity.length() as f32;
            let fwd = {
                let h = input.heading.to_radians();
                DVec2::new(h.sin(), h.cos())
            };
            let a = ((input.velocity - self.vel).dot(fwd) as f32) / dt;
            self.accel += (a.clamp(-4.0, 4.0) - self.accel) * ease_k(dt, 0.25);
            self.speed += (v_now - self.speed) * ease_k(dt, 0.12);
        }
        self.vel = input.velocity;
        self.origin = input.origin;
        self.heading = input.heading;
        let v_in = input.velocity.length() as f32;

        // sitting down and getting up
        let wants_sit = input.activity == Activity::Sit && input.seat.is_some();
        if let Some(s) = input.seat {
            self.seat = s;
        }
        if wants_sit {
            self.sit = approach(self.sit, 1.0, dt / 1.3);
        } else {
            self.sit = approach(self.sit, 0.0, dt / 1.1);
        }
        let seated = self.sit > 0.0;

        // reaching for the cash desk, holding on
        match input.reach {
            Some(r) if !seated => {
                // the hand moves on to a new target, it does not jump there
                if self.reach <= 0.0 {
                    self.reach_at = r;
                } else {
                    self.reach_at += (r - self.reach_at) * ease_k(dt, 0.22);
                }
                self.reach = approach(self.reach, 1.0, dt / 0.55);
            }
            _ => self.reach = approach(self.reach, 0.0, dt / 0.6),
        }
        match input.grips {
            Some(g) => {
                self.grip_at = g;
                self.grip_frame = input.grip_frames;
                self.grip_extra += (input.grip_lean - self.grip_extra) * ease_k(dt, 0.3);
                self.grip = approach(self.grip, 1.0, dt / 0.8);
            }
            None => self.grip = approach(self.grip, 0.0, dt / 0.8),
        }
        let hold_to = if seated {
            0.0
        } else {
            input.hold.clamp(0.0, 1.0)
        };
        self.hold = approach(
            self.hold,
            hold_to,
            dt / if hold_to > self.hold { 0.7 } else { 1.3 },
        );

        // balance against the bus: a damped spring pulled by the floor's acceleration
        let pull = -input.sway.clamp(Vec2::splat(-4.0), Vec2::splat(4.0));
        let k = 38.0;
        let damp = 7.5;
        let acc = (pull - self.lean) * k - self.lean_v * damp;
        self.lean_v += acc * dt;
        self.lean += self.lean_v * dt;

        // breathing, weight shifts, fidgeting, glances
        self.breath = (self.breath + dt * TAU / (3.6 + 0.6 * self.style[0])) % (TAU * 64.0);
        self.shift_t -= dt;
        if self.shift_t <= 0.0 {
            self.shift_t = 4.0 + self.rand() * 7.0;
            self.shift_to = (self.rand() * 2.0 - 1.0) * 0.03 * rig.scale;
        }
        self.shift += (self.shift_to - self.shift) * ease_k(dt, 1.2);
        self.fidget_t -= dt;
        if self.fidget_t <= 0.0 {
            self.fidget_t = 9.0 + self.rand() * 14.0;
            let side = (self.rand() * 2.0) as usize % 2;
            self.fidget[side] =
                Vec2::new((self.rand() - 0.5) * 0.05, (self.rand() - 0.5) * 0.12) * rig.scale;
        }
        self.glance_t -= dt;
        if self.glance_t <= 0.0 {
            let walking = self.walk > 0.3;
            self.glance_t = if walking {
                2.0 + self.rand() * 4.0
            } else {
                1.5 + self.rand() * 5.0
            };
            self.glance = if self.rand() < 0.35 {
                Vec2::ZERO
            } else if walking {
                Vec2::new((self.rand() - 0.5) * 50.0, -4.0 - self.rand() * 8.0)
            } else {
                Vec2::new((self.rand() - 0.5) * 110.0, (self.rand() - 0.6) * 18.0)
            };
        }
        let look_to = match input.look {
            Some(p) => {
                let from = rig.neck + Vec3::Z * 0.08;
                let d = p - from;
                let yaw = d.x.atan2(d.y).to_degrees();
                let pitch = d.z.atan2(d.truncate().length()).to_degrees();
                if yaw.abs() > 125.0 {
                    Vec2::new(0.0, pitch.clamp(-35.0, 25.0))
                } else {
                    Vec2::new(yaw.clamp(-80.0, 80.0), pitch.clamp(-38.0, 28.0))
                }
            }
            None => self.glance,
        };
        let step = 220.0 * dt;
        let k_head = ease_k(dt, 0.14);
        let want = self.head + (look_to - self.head) * k_head;
        self.head.x = approach(self.head.x, want.x, step);
        self.head.y = approach(self.head.y, want.y, step);

        // the gait: on while moving, off when stopped or sitting
        let moving = if self.gait {
            self.speed > 0.06 || v_in > 0.08
        } else {
            self.speed > 0.12 || v_in > 0.2
        };
        let gait_now = moving && !seated;
        let walk_to = if gait_now {
            (self.speed / 1.1).clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.walk += (walk_to - self.walk) * ease_k(dt, 0.2);
        let speed = self.speed.max(0.08);
        let cadence = rig.cadence(speed) * 0.5;
        let beta = stance_fraction(speed);
        if gait_now && !self.gait {
            // start with the foot that is further back (or the one already in the air)
            let back = |f: &Foot| {
                if f.planted {
                    self.to_local(f.pos).y
                } else {
                    f32::MIN
                }
            };
            let first = if back(&self.feet[0]) <= back(&self.feet[1]) {
                0
            } else {
                1
            };
            self.phase = (beta - 0.001 + if first == 1 { 0.5 } else { 0.0 }) % 1.0;
        }
        if !gait_now && self.gait {
            // stopping: a foot in the air lands beside the other one, in the time its
            // swing had left
            for f in self.feet.iter_mut() {
                if !f.planted && f.walk {
                    f.walk = false;
                    f.dur = (1.0 - beta) / cadence;
                }
            }
        }
        self.gait = gait_now;
        let prev_phase = self.phase;
        if self.gait {
            self.phase = (self.phase + dt * cadence) % 1.0;
            // a foot left too far behind (the body was pushed on or sped up faster than the
            // stride): a quick step catches up rather than the hips sinking to reach it. A
            // foot left on another floor (a stair the body has gone down from, the bus floor
            // after stepping off) goes first: it lifts the body's floor, so the other foot
            // could never reach down and stepped again and again while this one was dragged
            // along a flight up.
            if self.feet.iter().all(|f| f.planted) {
                let reach = (rig.thigh + rig.shin) * 0.997;
                let mut worst: Option<(usize, f32)> = None;
                for side in 0..2 {
                    let f = &self.feet[side];
                    let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
                    if f.walk && u >= beta - 0.04 {
                        // lifts now anyway
                        continue;
                    }
                    let pitch = if f.walk {
                        stance_pitch(u, beta, self.walk)
                    } else {
                        0.0
                    };
                    let a = self.to_local(Self::stance_ankle(rig, f, pitch));
                    let hip = rig.hip[side] + Vec3::Z * self.body_floor;
                    let horiz = (a - hip).truncate().length();
                    let stranded = (f.pos.z - self.origin.z).abs() as f32;
                    let need = if stranded > STRANDED {
                        1.0 + stranded
                    } else {
                        (hip.z - a.z) - (reach * reach - horiz * horiz).max(0.0).sqrt()
                    };
                    if need > CATCH_UP && worst.map(|w| need > w.1).unwrap_or(true) {
                        worst = Some((side, need));
                    }
                }
                if let Some((side, _)) = worst {
                    let f = &mut self.feet[side];
                    f.from_ankle = Self::stance_ankle(rig, f, 0.0);
                    f.from_yaw = f.yaw;
                    f.from_pitch = 0.0;
                    f.from_floor = f.pos.z;
                    f.planted = false;
                    f.walk = false;
                    f.t = 0.0;
                    f.dur = 0.26;
                    f.lift = 0.05 * rig.scale;
                    f.to_sampled = DVec2::splat(f64::MAX);
                    f.land_z = f.pos.z;
                    f.land_wait = 0.0;
                    self.catch_ups += 1;
                }
            }
        }
        for side in 0..2 {
            let offset = if side == 0 { 0.0 } else { 0.5 };
            let u0 = (prev_phase + offset) % 1.0;
            let u1 = (self.phase + offset) % 1.0;
            let (origin, heading) = (self.origin, self.heading);
            if !self.gait {
                if !self.feet[side].planted {
                    self.fly(rig, input, side, dt, origin, heading);
                }
                continue;
            }
            // lift off at the end of the stance (or a little late, when the other foot
            // was still in the air), from the push-off pitch
            if self.feet[side].planted
                && u1 >= beta
                && u1 < beta + 0.3
                && (self.feet[1 - side].planted || run_factor(speed) > 0.5)
            {
                let from_pitch = if self.feet[side].walk {
                    toe_pitch(beta, beta, self.walk)
                } else {
                    0.0
                };
                let f = &mut self.feet[side];
                f.from_ankle = Self::stance_ankle(rig, f, from_pitch);
                f.from_yaw = f.yaw;
                f.from_pitch = from_pitch;
                f.from_floor = f.pos.z;
                f.planted = false;
                f.walk = true;
                // the swing starts at its beginning even when the lift-off came late (the
                // other foot still in the air): begun at the phase's own progress, up to
                // 0.3 of the way, the foot jumped 15-22 cm towards its landing in a frame
                f.t0 = ((u1 - beta) / (1.0 - beta)).clamp(0.0, 0.9);
                f.t = 0.0;
                f.to_sampled = DVec2::splat(f64::MAX);
                f.land_z = f.pos.z;
                f.land_wait = 0.0;
                f.lift = (0.035 + 0.035 * (speed / 1.3).min(1.3) + 0.11 * run_factor(speed)) * rig.scale;
            }
            if self.feet[side].planted {
                continue;
            }
            if !self.feet[side].walk {
                // a shuffle step still in the air when the walk began
                self.fly(rig, input, side, dt, origin, heading);
                continue;
            }
            if u1 < beta || u1 < u0 {
                self.land(input, side);
                continue;
            }
            // on its way: to where the body will be when it lands
            let tp = ((u1 - beta) / (1.0 - beta)).clamp(0.0, 1.0);
            let remain = (1.0 - tp) * (1.0 - beta) / cadence;
            let t0 = self.feet[side].t0;
            let t = ((tp - t0) / (1.0 - t0).max(0.1)).clamp(0.0, 1.0);
            let pred_origin = origin + (self.vel * remain as f64).extend(0.0);
            // (the turn to come, but not more than a step's worth: at a quick turn the whole
            // remaining swing's turn rate put the landing half a metre to the side, and the
            // legs were flung out sideways)
            let pred_heading = heading + (self.turn * remain).clamp(-25.0, 25.0) as f64;
            let (to, yaw) = self.rest_target(rig, side, pred_origin, pred_heading, true);
            let f = &mut self.feet[side];
            f.to = to;
            f.to_yaw = yaw;
            f.t = f.t.max(t);
        }
        // standing: step when a foot is left too far from where it belongs
        if !self.gait && !seated && self.feet.iter().all(|f| f.planted) {
            let mut worst: Option<(usize, f32)> = None;
            for side in 0..2 {
                let (rp, ryaw) = self.rest_target(rig, side, self.origin, self.heading, false);
                let off = (self.feet[side].pos - rp).truncate().length() as f32;
                let yaw_off = angle_diff(self.feet[side].yaw, ryaw).abs() as f32;
                let reach = self.to_local(self.feet[side].pos).truncate().length();
                let stranded = (self.feet[side].pos.z - self.origin.z).abs() as f32;
                let bad = (off / 0.13)
                    .max(yaw_off / 12.0)
                    .max(reach / (0.6 * rig.leg()))
                    .max(stranded / STRANDED);
                if bad > 1.0 && worst.map(|w| bad > w.1).unwrap_or(true) {
                    worst = Some((side, bad));
                }
            }
            if let Some((side, bad)) = worst {
                let f = &mut self.feet[side];
                f.from_ankle = Self::stance_ankle(rig, f, 0.0);
                f.from_yaw = f.yaw;
                f.from_pitch = 0.0;
                f.from_floor = f.pos.z;
                f.planted = false;
                f.walk = false;
                f.t = 0.0;
                f.dur = if bad > 2.5 { 0.28 } else { 0.38 };
                f.lift = 0.045 * rig.scale;
                f.to_sampled = DVec2::splat(f64::MAX);
                f.land_z = f.pos.z;
                f.land_wait = 0.0;
            }
        }
        // last resort: a planted foot the legs cannot reach (pushed away while the other
        // one was in the air) shuffles along the floor until they can
        if !seated {
            let reach = (rig.thigh + rig.shin) * 0.997;
            for side in 0..2 {
                let f = self.feet[side];
                if !f.planted {
                    continue;
                }
                let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
                let pitch = if f.walk && self.gait && u < beta + 0.3 {
                    stance_pitch(u.min(beta), beta, self.walk)
                } else {
                    0.0
                };
                let a = self.to_local(Self::stance_ankle(rig, &f, pitch));
                let hip = rig.hip[side] + Vec3::Z * self.body_floor;
                let off = (a - hip).truncate();
                let down = hip.z - a.z - MAX_DROP;
                let horiz_max = (reach * reach - down * down).max(0.0).sqrt();
                if off.length() > horiz_max + 0.01 && off.length() > 1e-3 {
                    let pull = off.normalize() * (off.length() - horiz_max);
                    let h = self.heading.to_radians();
                    let (sn, cs) = (h.sin(), h.cos());
                    let (px, py) = (pull.x as f64, pull.y as f64);
                    self.feet[side].pos -= DVec3::new(px * cs + py * sn, -px * sn + py * cs, 0.0);
                    self.shuffles += 1;
                }
            }
        }
        // the floor under the body follows the planted feet
        let mut floor_sum = 0.0;
        let mut floor_n = 0.0;
        for f in &self.feet {
            let z = if f.planted {
                f.pos.z
            } else {
                f.land_z.max(f.pos.z.min(f.land_z))
            };
            floor_sum += (z - self.origin.z) as f32;
            floor_n += 1.0;
        }
        let floor_to = if floor_n > 0.0 {
            floor_sum / floor_n
        } else {
            0.0
        };
        let floor_to = floor_to.clamp(-0.6, 0.6);
        self.body_floor += (floor_to - self.body_floor) * ease_k(dt, 0.18);
        // the pelvis may rise again only gradually
        self.drop = approach(self.drop, 0.0, dt * 0.4);
        self.swing_floor(input, dt);
    }

    fn fly(
        &mut self,
        rig: &Rig,
        input: &PoseInput,
        side: usize,
        dt: f32,
        origin: DVec3,
        heading: f64,
    ) {
        // to beside the other foot, or in the walk to where the body will be
        let remain = ((1.0 - self.feet[side].t) * self.feet[side].dur) as f64;
        let ahead = if self.gait { remain } else { 0.15 };
        let (to, yaw) = self.rest_target(
            rig,
            side,
            origin + (self.vel * ahead).extend(0.0),
            heading + (self.turn * 0.25) as f64,
            self.gait,
        );
        let f = &mut self.feet[side];
        f.to = to;
        f.to_yaw = yaw;
        f.t = (f.t + dt / f.dur.max(0.05)).min(1.0);
        if f.t >= 1.0 {
            // a foot whose landing height is still on its way stays up a moment longer
            // (put down at once it would drop or jump the rest)
            if (f.to_floor - f.land_z).abs() > 0.01 && f.land_wait < 0.2 {
                f.land_wait += dt;
                return;
            }
            self.land(input, side);
        }
    }

    fn land(&mut self, input: &PoseInput, side: usize) {
        let fallback = self.origin.z + self.body_floor as f64;
        let f = &mut self.feet[side];
        let floor = if (f.to.truncate() - f.to_sampled).length() < 0.05 {
            f.to_floor
        } else {
            Self::sample_floor(input, f.to.truncate(), fallback)
        };
        // (within a centimetre of where the swing brought it, or it gave up waiting)
        let floor = if (floor - f.land_z).abs() < 0.05 { floor } else { f.land_z + (floor - f.land_z).clamp(-0.05, 0.05) };
        f.pos = DVec3::new(f.to.x, f.to.y, floor);
        f.yaw = f.to_yaw;
        f.planted = true;
        f.t = 0.0;
        self.landed = true;
    }

    /// Update the landing floor height of the swinging feet.
    fn swing_floor(&mut self, input: &PoseInput, dt: f32) {
        let fallback = self.origin.z + self.body_floor as f64;
        for f in self.feet.iter_mut() {
            if !f.planted && (f.to.truncate() - f.to_sampled).length() > 0.05 {
                f.to_floor = Self::sample_floor(input, f.to.truncate(), fallback);
                f.to_sampled = f.to.truncate();
            }
            if !f.planted {
                let gap = f.to_floor - f.land_z;
                let step = (gap * ease_k(dt, 0.05) as f64).clamp(-1.8 * dt as f64, 1.8 * dt as f64);
                f.land_z = if gap.abs() < 0.002 { f.to_floor } else { f.land_z + step };
            }
        }
    }

    /// Bone transforms for the current state.
    pub fn bones(&mut self, rig: &Rig) -> Posed {
        let d = |deg: f32| deg.to_radians();
        let walk = self.walk;
        let speed = self.speed.max(0.08);
        let beta = stance_fraction(speed);
        let ph = self.phase * TAU;
        let sit = self.sit;
        let s_ease = smoothstep(0.0, 1.0, sit);
        // hips move back first and come down later (and the other way round getting up)
        let s_back = smoothstep(0.0, 0.75, sit);
        let s_down = smoothstep(0.2, 1.0, sit);
        let bump = (PI * sit).sin().max(0.0);
        let breath = self.breath.sin();
        let intensity = ((speed - 0.15) / 1.1).clamp(0.0, 1.0) * walk;
        let still = 1.0 - walk;

        // --- feet targets (model frame): ankle positions, foot yaw and pitch ---
        let mut ankle_t = [Vec3::ZERO; 2];
        let mut foot_rot = [Mat3A::IDENTITY; 2];
        // the toes bend up against the foot while the heel is up, and straighten in the air
        let mut toe_bend = [0f32; 2];
        let mut foot_fwd = [Vec3::Y; 2];
        let mut foot_yaw = [0f32; 2];
        // a foot in the air: (progress, pitch at lift-off, pitch to land with); its pitch
        // follows the shin once the leg is solved
        let mut swing: [Option<(f32, f32, f32)>; 2] = [None; 2];
        let mut swing_floor = [0f32; 2];
        for side in 0..2 {
            let f = self.feet[side];
            let u = (self.phase + if side == 0 { 0.0 } else { 0.5 }) % 1.0;
            let (ankle, yaw, pitch) = if f.planted {
                // (a lift-off held back while the other foot is still in the air keeps the
                // push-off pitch: drawn flat meanwhile, the ankle leapt 7 cm up and forward
                // the moment the foot left)
                let pitch = if f.walk && self.gait && u < beta + 0.3 {
                    stance_pitch(u.min(beta), beta, walk)
                } else {
                    0.0
                };
                toe_bend[side] = (-pitch).max(0.0);
                (Self::stance_ankle(rig, &f, pitch), f.yaw, pitch)
            } else {
                let t = f.t.clamp(0.0, 1.0);
                let land_pitch = if f.walk { heel_pitch(walk) } else { 0.0 };
                let land = {
                    let g = Foot {
                        pos: DVec3::new(f.to.x, f.to.y, f.land_z),
                        yaw: f.to_yaw,
                        ..f
                    };
                    Self::stance_ankle(rig, &g, land_pitch)
                };
                let s = smoothstep(0.0, 1.0, t) as f64;
                let mut a = f.from_ankle + (land - f.from_ankle) * s;
                // up a step the foot rises early, down one it drops late
                // (blended by the height, not switched: the curve changing mid-swing moved
                // the foot as much as the floor did)
                let rise = (land.z - f.from_ankle.z) as f32;
                let up = smoothstep(0.0, 0.06, rise);
                let down = smoothstep(0.0, 0.06, -rise);
                let zt = t * (1.0 - up - down) + smoothstep(0.0, 0.55, t) * up + smoothstep(0.35, 1.0, t) * down;
                let bumpf = (PI * t.powf(0.75)).sin().max(0.0);
                a.z = f.from_ankle.z
                    + (land.z - f.from_ankle.z) * zt as f64
                    + (f.lift * bumpf) as f64;
                let yaw = f.from_yaw + angle_diff(f.from_yaw, f.to_yaw) * s;
                toe_bend[side] = (-f.from_pitch).max(0.0) * (1.0 - smoothstep(0.0, 0.35, t));
                if f.walk {
                    swing[side] = Some((t, f.from_pitch, land_pitch));
                }
                swing_floor[side] =
                    (f.from_floor + (f.land_z - f.from_floor) * zt as f64 - self.origin.z) as f32;
                (a, yaw, 0.0)
            };
            let local = self.to_local(ankle);
            let yaw_l = angle_diff(self.heading, yaw) as f32;
            ankle_t[side] = local;
            foot_rot[side] = Mat3A::from_quat(yaw_quat(yaw_l) * Quat::from_rotation_x(d(pitch)));
            foot_fwd[side] = yaw_quat(yaw_l) * Vec3::Y;
            foot_yaw[side] = yaw_l;
        }

        // --- pelvis ---
        let sway = rig.hip_sway.sqrt();
        // towards the leg that carries the weight, dipping on the swinging side
        let lat_walk = -0.022 * rig.scale * sway.min(1.3) * (ph - 0.25).sin() * intensity;
        let lat_idle = self.shift * still * (1.0 - s_ease);
        let run = run_factor(self.speed);
        let bob = -0.018 * rig.scale * intensity * (1.0 + 1.8 * run) * (0.5 + 0.5 * (2.0 * ph - 0.35).cos());
        let lean_acc = (self.accel * 1.8).clamp(-6.0, 7.0) * walk;
        let pelvis_roll = d(3.5) * sway * intensity * (ph - 0.35).sin()
            - d(2.2) * (self.shift / (0.03 * rig.scale)) * still * (1.0 - s_ease);
        // the hip of the leg in front leads (about z, counter-clockwise positive)
        let pelvis_yaw = -d(5.0) * intensity * ph.cos();
        // reaching for something far: bend at the hips and bring the right shoulder round
        let (reach_lean, reach_twist) = if self.reach > 0.0 {
            let v = self.reach_at - rig.shoulder[1];
            let excess = (v.length() - 0.75 * (rig.upper_arm + rig.forearm)).max(0.0);
            let r = smoothstep(0.0, 1.0, self.reach);
            let yaw = v.x.atan2(v.y.max(0.05)).to_degrees();
            (
                d(32.0) * (excess / 0.35).min(1.0) * r * if v.y > 0.0 { 1.0 } else { 0.3 },
                d((-yaw * 0.5).clamp(-30.0, 12.0)) * r,
            )
        } else {
            (0.0, 0.0)
        };
        // a driver sits up, leaning towards the wheel
        let grip_lean = d(16.0 + self.grip_extra.clamp(0.0, 30.0)) * smoothstep(0.0, 1.0, self.grip);
        // forward tilt walking, backwards on a seat
        let pelvis_tilt = d(2.0) * walk + d(11.0) * run - d(12.0) * s_ease + reach_lean * 0.5 + grip_lean * 0.7;
        // swaying with the bus: the body goes with the pull (forwards when it brakes, outwards
        // in a bend), less when holding on or sitting
        let lean_bus = self.lean * (1.0 - 0.4 * self.hold) * (1.0 - 0.4 * s_ease);
        let bus_shift = Vec3::new(lean_bus.x * 0.012, lean_bus.y * 0.012, 0.0) * (1.0 - s_ease);
        let stand_z = rig.pelvis.z - 0.004 * rig.scale - 0.012 * walk - 0.035 * rig.scale * run + 0.002 * breath * still;
        let stand_c = Vec3::new(
            lat_walk + lat_idle,
            rig.pelvis.y + 0.02 * walk,
            stand_z + bob + self.body_floor,
        ) + bus_shift;
        // `self.seat` puts the pelvis a fixed distance (`SEAT_FRONT` in humans.rs) in
        // front of the hip point, the same for every human type because the shared cabin
        // that computed it has no rig to ask. Redone here with this rig's own
        // `seat_front()` (from its actual thigh length), the knee lands where this body's
        // legs naturally put it instead of at the average distance - on a long bench with
        // no footwell to hide a mismatch in, a longer-legged rig forced to the average
        // distance bent its knee enough to poke through the seat ahead.
        let seated_c = Vec3::new(
            self.seat.x,
            -rig.seat_front() + 0.03,
            self.seat.z + rig.seat_lift,
        );
        let mut pc = Vec3::new(
            stand_c.x + (seated_c.x - stand_c.x) * s_back,
            stand_c.y + (seated_c.y - stand_c.y) * s_back,
            stand_c.z + (seated_c.z - stand_c.z) * s_down,
        );
        // over the feet while getting up or down: the hips go a little lower and further
        pc.z -= 0.05 * bump * rig.scale;
        // the pelvis never higher than the legs can reach
        let pelvis_q = Quat::from_rotation_z(pelvis_yaw)
            * Quat::from_rotation_y(pelvis_roll)
            * Quat::from_rotation_x(-pelvis_tilt);
        let reach_max = (rig.thigh + rig.shin) * 0.997;
        let mut need = 0.0f32;
        if sit < 0.5 {
            // (a foot in the air need not be reached)
            for side in (0..2).filter(|&k| self.feet[k].planted) {
                let hip_at = pc + pelvis_q * (rig.hip[side] - rig.pelvis);
                let dv = ankle_t[side] - hip_at;
                let horiz = dv.truncate().length();
                if horiz < reach_max {
                    let max_up = (reach_max * reach_max - horiz * horiz).sqrt();
                    let over = -dv.z - max_up;
                    need = need.max(over);
                }
            }
        }
        let drop = need.clamp(0.0, MAX_DROP).max(self.drop);
        self.drop = drop;
        pc.z -= drop * (1.0 - s_ease);
        let pelvis_m = Affine3A::from_translation(pc - rig.pelvis) * about(rig.pelvis, pelvis_q);

        // --- trunk and head ---
        // shoulders against the hips, and a little towards what the head looks at
        // (the shoulders take a good part of a look to the side - up to 30 degrees - so that
        // the head turns no further on the trunk than a neck can: the people of OMSI have
        // no neck bone, and the skin between collar and head, stretched by a head turned
        // 60 degrees on still shoulders, made a twisted, broken neck of every passenger
        // who looked at the driver)
        let trunk_yaw = -pelvis_yaw * 1.7
            - d((self.head.x.clamp(-90.0, 90.0) * 0.42).clamp(-30.0, 30.0)) * (1.0 - walk) * (1.0 - self.reach)
            + reach_twist;
        let trunk_lean = d(3.0 * walk + lean_acc)
            + d(34.0) * bump
            + d(12.0 - 4.0) * s_ease
            + d(0.7) * breath
            + d(lean_bus.y * 2.4)
            + reach_lean * 0.6
            + grip_lean * 0.5;
        let trunk_side = -pelvis_roll * 0.8 + d(lean_bus.x * 2.4);
        let trunk_q = Quat::from_rotation_z(trunk_yaw)
            * Quat::from_rotation_y(trunk_side)
            * Quat::from_rotation_x(-trunk_lean);
        let trunk_m = pelvis_m * about(rig.waist, trunk_q);
        let trunk_rot = pelvis_q * trunk_q;
        // the head: level, turned towards what it looks at, within its limits
        // (it follows half of a deep forward lean)
        let head_pitch = self.head.y.clamp(-38.0, 28.0) + 2.0 * self.style[1]
            - 0.5 * trunk_lean.to_degrees().max(0.0);
        let head_world =
            yaw_quat(self.head.x.clamp(-72.0, 72.0)) * Quat::from_rotation_x(d(head_pitch));
        let head_rel = limit_quat(trunk_rot.inverse() * head_world, d(45.0));
        let head_m = trunk_m * about(rig.head_pivot, head_rel);

        // --- legs ---
        let mut out_bones = [Affine3A::IDENTITY; SLOTS];
        out_bones[HIP] = pelvis_m;
        out_bones[MAIN] = trunk_m;
        out_bones[HEAD] = head_m;
        let mut posed = Posed {
            bones: [Affine3A::IDENTITY; SLOTS],
            hip: [Vec3::ZERO; 2],
            knee: [Vec3::ZERO; 2],
            ankle: [Vec3::ZERO; 2],
            elbow: [Vec3::ZERO; 2],
            wrist: [Vec3::ZERO; 2],
            sole: [0.0; 2],
            knee_flex: [0.0; 2],
            ankle_flex: [0.0; 2],
            leg_miss: [0.0; 2],
            neck: head_m.transform_point3(rig.neck),
            ok: true,
        };
        let pelvis_fwd = pelvis_q * Vec3::Y;
        for side in 0..2 {
            let hip_at = pelvis_m.transform_point3(rig.hip[side]);
            let fwd = (foot_fwd[side] + pelvis_fwd).normalize_or(Vec3::Y);
            let pole = fwd + Vec3::Z * 0.25;
            // Seated, the feet go where a sitting body puts them: the thigh along the seat,
            // the shin hanging down. A floor further down than that (a seat on a podium or
            // over a wheel arch) is not reached by stretching the leg straight at it - the
            // leg ran diagonally through the seat's front - the feet hang above it instead.
            if sit > 0.5 {
                let flat = Vec3::new(pelvis_fwd.x, pelvis_fwd.y, 0.0).normalize_or(Vec3::Y);
                let knee_n = hip_at + flat * rig.thigh * 0.95;
                let hang = knee_n - Vec3::Z * rig.shin * 0.97;
                if ankle_t[side].z < hang.z - 0.12 || (ankle_t[side] - hip_at).length() > (rig.thigh + rig.shin) * 0.99 {
                    let blend = ((sit - 0.5) * 2.0).clamp(0.0, 1.0);
                    let floor_z = ankle_t[side].z.max(hang.z);
                    let target = Vec3::new(hang.x, hang.y, floor_z);
                    ankle_t[side] = ankle_t[side] + (target - ankle_t[side]) * blend;
                }
            }
            let (knee_at, ankle_at, hinge) =
                two_bone(hip_at, rig.thigh, rig.shin, ankle_t[side], pole);
            let side_v = hinge;
            if let Some((t, from, land)) = swing[side] {
                // in the air the ankle goes from pointed (push-off) through neutral to a
                // little flexed; the pitch is that against the shin, blended from the
                // push-off and into the heel strike
                let s_dir = (ankle_at - knee_at).normalize_or(-Vec3::Z);
                let neutral = side_v.cross(s_dir).normalize_or(Vec3::Y);
                let shin_pitch = neutral.z.clamp(-1.0, 1.0).asin().to_degrees();
                let ankle = -14.0 + 14.0 * smoothstep(0.0, 0.5, t) + 3.0 * smoothstep(0.5, 0.85, t);
                let rel = shin_pitch + ankle;
                let from_w = 1.0 - smoothstep(0.0, 0.3, t);
                let land_w = smoothstep(0.65, 1.0, t);
                let mut pitch = (rel + (from - rel) * from_w) * (1.0 - land_w) + land * land_w;
                // the toes clear the floor, the heel too
                let above = ankle_at.z - swing_floor[side] - 0.012;
                let ah = rig.ankle_h;
                let (rt, pt) = ((rig.toe * rig.toe + ah * ah).sqrt(), ah.atan2(rig.toe));
                if -above / rt > -1.0 {
                    pitch = pitch.max((pt + (-above / rt).min(1.0).asin()).to_degrees());
                }
                let (rh, phh) = ((rig.heel * rig.heel + ah * ah).sqrt(), ah.atan2(-rig.heel));
                if -above / rh > -1.0 {
                    pitch = pitch.min(-(phh + (-above / rh).min(1.0).asin()).to_degrees());
                }
                foot_rot[side] =
                    Mat3A::from_quat(yaw_quat(foot_yaw[side]) * Quat::from_rotation_x(d(pitch)));
            }
            let r_thigh = bone_rot(
                rig.knee[side] - rig.hip[side],
                Vec3::X,
                knee_at - hip_at,
                side_v,
            );
            let r_shin = bone_rot(
                rig.ankle[side] - rig.knee[side],
                Vec3::X,
                ankle_at - knee_at,
                side_v,
            );
            out_bones[THIGH[side]] = joint_xf(rig.hip[side], hip_at, r_thigh);
            out_bones[SHIN[side]] = joint_xf(rig.knee[side], knee_at, r_shin);
            out_bones[FOOT[side]] = joint_xf(rig.ankle[side], ankle_at, foot_rot[side]);
            let ball_rest = rig.ball_joint(side);
            let ball_at = out_bones[FOOT[side]].transform_point3(ball_rest);
            let toe_rot = foot_rot[side] * Mat3A::from_rotation_x(d(toe_bend[side]));
            out_bones[TOE[side]] = joint_xf(ball_rest, ball_at, toe_rot);
            let t_dir = (knee_at - hip_at).normalize_or(-Vec3::Z);
            let s_dir = (ankle_at - knee_at).normalize_or(-Vec3::Z);
            posed.hip[side] = hip_at;
            posed.knee[side] = knee_at;
            posed.ankle[side] = ankle_at;
            posed.knee_flex[side] = t_dir.dot(s_dir).clamp(-1.0, 1.0).acos().to_degrees();
            let foot_dir = Vec3::from(foot_rot[side] * Vec3A::Y);
            // toes towards the shin positive
            posed.ankle_flex[side] =
                90.0 - foot_dir.dot(-s_dir).clamp(-1.0, 1.0).acos().to_degrees();
            posed.leg_miss[side] = (ankle_at - ankle_t[side]).length();
            let heel =
                ankle_at + Vec3::from(foot_rot[side] * Vec3A::new(0.0, rig.heel, -rig.ankle_h));
            let ball = ball_at + Vec3::from(foot_rot[side] * Vec3A::new(0.0, 0.0, -rig.ball_h));
            let toe =
                ball_at + Vec3::from(toe_rot * Vec3A::new(0.0, rig.toe - rig.ball, -rig.ball_h));
            posed.sole[side] = heel.z.min(toe.z).min(ball.z);
        }

        // --- arms ---
        let run = run_factor(self.speed);
        let arm_amp = d(17.0) * rig.arm_swing.powf(0.7) * intensity * (1.0 + 1.1 * run);
        for side in 0..2 {
            let s = SIDE[side];
            let sh_at = trunk_m.transform_point3(rig.shoulder[side]);
            // the hanging arm: swings against the legs (left arm forward when the right
            // leg is), a little out from the body, the elbow bending more going forward
            let arm_fwd = if side == 0 { -ph.cos() } else { ph.cos() };
            let swing = arm_amp * arm_fwd - d(3.0) * walk
                + d(20.0) * bump * (1.0 - s_down)
                + d(2.0 + 1.5 * self.style[3]);
            let abd = d(9.0 + 1.5 * self.style[0] + 2.0 * walk) + d(0.6) * breath * still;
            // (running: the elbows held bent near a right angle)
            let flex = d(13.0 + 3.0 * self.style[3])
                + d(16.0) * intensity * (0.5 + 0.5 * arm_fwd)
                + d(8.0) * bump
                + d(62.0) * run;
            let inward = d(16.0);
            let a = Quat::from_rotation_x(swing) * Vec3::new(s * abd.sin(), 0.0, -abd.cos());
            let front0 =
                Quat::from_rotation_x(swing) * Vec3::new(-s * inward.sin(), inward.cos(), 0.0);
            let front = (front0 - a * a.dot(front0)).normalize_or(Vec3::Y);
            let c = a * flex.cos() + front * flex.sin();
            let elbow_l = a * rig.upper_arm;
            let wrist_l = elbow_l + c * rig.forearm;
            let hang_wrist = sh_at + trunk_rot * wrist_l;
            let hang_elbow = sh_at + trunk_rot * elbow_l;
            let hang_pole = hang_elbow - (sh_at + hang_wrist) * 0.5;
            // seated: hands on the thighs
            let hip_at = posed.hip[side];
            let knee_at = posed.knee[side];
            let lap =
                hip_at + (knee_at - hip_at) * 0.55 + Vec3::new(-s * 0.02, 0.0, 0.11 * rig.scale);
            let lap_pole = pelvis_q * Vec3::new(s * 0.5, -1.0, -0.2);
            let sit_w = smoothstep(0.35, 1.0, sit);
            let mut target = hang_wrist.lerp(lap, sit_w);
            let mut pole = hang_pole.normalize_or(-Vec3::Y).lerp(lap_pole, sit_w);
            if side == 1 && self.reach > 0.0 {
                let r = smoothstep(0.0, 1.0, self.reach);
                let reach_to = self.reach_at;
                // not further than the arm, and not through the body
                let v = reach_to - sh_at;
                let reach_to = sh_at
                    + v.normalize_or(Vec3::Y)
                        * v.length().min((rig.upper_arm + rig.forearm) * 0.96);
                target = target.lerp(reach_to, r);
                pole = pole.lerp(Vec3::new(0.6, -0.3, -1.0), r);
            }
            if self.hold > 0.0 && !(side == 1 && self.reach > 0.3) {
                let h = smoothstep(0.0, 1.0, self.hold);
                let hold_side = if self.style[2] > 0.0 { 1 } else { 0 };
                if side == hold_side {
                    let reach = rig.upper_arm + rig.forearm;
                    // the handrail over the aisle, or a pole for those who cannot reach it
                    let rail = 1.8;
                    let grip = if rig.shoulder[side].z + reach * 0.97 >= rail {
                        Vec3::new(
                            s * (rig.shoulder[1].x + 0.05),
                            rig.shoulder[1].y + 0.1,
                            rail + self.body_floor,
                        )
                    } else {
                        Vec3::new(
                            s * 0.2 * rig.scale,
                            0.28 * rig.scale,
                            rig.shoulder[1].z - 0.2 * rig.scale + self.body_floor,
                        )
                    };
                    target = target.lerp(grip, h);
                    pole = pole.lerp(Vec3::new(s, -0.2, -0.5), h);
                }
            }
            if self.grip > 0.0 {
                // on the steering wheel: the elbows down and a little out
                let g = smoothstep(0.0, 1.0, self.grip);
                let v = self.grip_at[side] - sh_at;
                let to = sh_at
                    + v.normalize_or(Vec3::Y) * v.length().min((rig.upper_arm + rig.forearm) * 0.97);
                target = target.lerp(to, g);
                pole = pole.lerp(Vec3::new(s * 0.7, -0.2, -1.0), g);
            }
            let (el_at, wr_at, _) = two_bone(sh_at, rig.upper_arm, rig.forearm, target, pole);
            let a1 = (el_at - sh_at).normalize_or(-Vec3::Z);
            let c1 = (wr_at - el_at).normalize_or(-Vec3::Z);
            let mut f1 = c1 - a1 * a1.dot(c1);
            if f1.length_squared() < 1e-6 {
                f1 = pole.cross(a1).cross(a1) * -1.0;
            }
            let f1 = f1.normalize_or(Vec3::Y);
            let bend = a1.dot(c1).clamp(-1.0, 1.0).acos();
            let f1_fore = f1 * bend.cos() - a1 * bend.sin();
            let a0 = rig.elbow[side] - rig.shoulder[side];
            let c0 = rig.wrist[side] - rig.elbow[side];
            let r_upper = bone_rot(a0, Vec3::Y, a1, f1);
            let r_fore = bone_rot(c0, Vec3::Y, c1, f1_fore);
            // the wrist: relaxed, a little flexed towards the palm (down in the T-pose);
            // flat for the desk
            let hinge0 = c0
                .normalize_or(Vec3::X)
                .cross(-Vec3::Z)
                .normalize_or(Vec3::Y);
            let wrist_flex = d(10.0) * (1.0 - if side == 1 { self.reach } else { 0.0 });
            let mut r_hand = r_fore * Mat3A::from_axis_angle(hinge0, wrist_flex);
            if let (true, Some(frames)) = (self.grip > 0.0, self.grip_frame) {
                // round the rim: the knuckles along it, the palm against it (the rest hand
                // lies palm down along the forearm); the wrist bends 80 degrees at most
                let (dir, palm) = frames[side];
                if dir.length_squared() > 1e-6 && palm.length_squared() > 1e-6 {
                    let want = bone_rot(c0, -Vec3::Z, dir.normalize(), palm.normalize());
                    let q_fore = Quat::from_mat3a(&r_hand);
                    let rel = limit_quat(q_fore.inverse() * Quat::from_mat3a(&want), d(80.0));
                    let q = q_fore.slerp(q_fore * rel, smoothstep(0.0, 1.0, self.grip));
                    r_hand = Mat3A::from_quat(q);
                }
            }
            out_bones[UPPER[side]] = joint_xf(rig.shoulder[side], sh_at, r_upper);
            out_bones[FORE[side]] = joint_xf(rig.elbow[side], el_at, r_fore);
            out_bones[HAND[side]] = joint_xf(rig.wrist[side], wr_at, r_hand);
            posed.elbow[side] = el_at;
            posed.wrist[side] = wr_at;
        }
        posed.ok = out_bones.iter().all(|b| b.is_finite());
        posed.bones = if posed.ok {
            out_bones
        } else {
            [Affine3A::IDENTITY; SLOTS]
        };
        if !posed.ok {
            // start over from standing rather than carry the bad state on
            let seed = self.rng;
            *self = Pose::new(seed);
        }
        posed
    }
}

/// How far (m) the hips would have to sink to reach a planted foot before it steps up.
const CATCH_UP: f32 = 0.13;

/// The most the hips sink to reach a planted foot (m).
const MAX_DROP: f32 = 0.12;

/// A planted foot this far (m) above or below the floor the body stands on is on another
/// floor and steps over first.
const STRANDED: f32 = 0.5;

/// Share of a gait cycle a foot is on the floor.
fn stance_fraction(speed: f32) -> f32 {
    let walk = (0.64 - 0.05 * (speed - 0.8)).clamp(0.57, 0.7);
    // running: the foot is down for well under half the stride, with a flight between
    walk + (0.38 - walk) * run_factor(speed)
}

/// How much of a run the gait is (0 walking … 1 running), by the speed (m/s).
pub fn run_factor(speed: f32) -> f32 {
    smoothstep(2.0, 3.4, speed)
}

/// Foot pitch while planted during the walk (degrees, toes up positive): the heel strikes
/// with the toes up and rolls down, the foot lies flat, then the heel rises until the toes
/// push off.
fn stance_pitch(u: f32, beta: f32, walk: f32) -> f32 {
    let heel = heel_pitch(walk) * (1.0 - smoothstep(0.0, 0.11, u));
    heel + toe_pitch(u, beta, walk)
}

/// Foot pitch at heel strike (degrees, toes up).
fn heel_pitch(walk: f32) -> f32 {
    16.0 * walk.clamp(0.3, 1.0)
}

fn toe_pitch(u: f32, beta: f32, walk: f32) -> f32 {
    let from = beta - 0.28;
    let t = ((u - from) / (beta - from)).clamp(0.0, 1.0);
    -48.0 * walk.clamp(0.25, 1.0) * t * t
}

/// Limit a rotation to `max` radians.
fn limit_quat(q: Quat, max: f32) -> Quat {
    let q = if q.w < 0.0 { -q } else { q };
    let (axis, angle) = q.to_axis_angle();
    if angle.abs() > max && axis.is_finite() {
        Quat::from_axis_angle(axis, max * angle.signum())
    } else {
        q
    }
}

/// The bone transforms of [`skin`] from Omsi.exe's thirteen bones
/// ([`crate::human_omsi::OmsiAnim::bones`]): the feet and toes, which the original does not
/// have, go with the shins they were split off.
pub fn slots_from_omsi(b: &[Affine3A; crate::human_omsi::BONES]) -> [Affine3A; SLOTS] {
    let mut out = [Affine3A::IDENTITY; SLOTS];
    out[..crate::human_omsi::BONES].copy_from_slice(b);
    for side in 0..2 {
        out[FOOT[side]] = b[SHIN[side]];
        out[TOE[side]] = b[SHIN[side]];
    }
    out
}

/// Deform `mesh` with the bone transforms (linear blend skinning).
pub fn skin(
    mesh: &HumanMesh,
    bones: &[Affine3A; SLOTS],
    out_pos: &mut Vec<Vec3>,
    out_nrm: &mut Vec<Vec3>,
) {
    let n = mesh.data.positions.len();
    out_pos.clear();
    out_nrm.clear();
    out_pos.reserve(n);
    out_nrm.reserve(n);
    for (i, inf) in mesh.skin.iter().enumerate().take(n) {
        let p = Vec3A::from(mesh.data.positions[i]);
        let nr = Vec3A::from(mesh.data.normals[i]);
        if inf.n <= 1 {
            let m = &bones[inf.slot[0] as usize];
            out_pos.push(m.transform_point3a(p).into());
            out_nrm.push(m.transform_vector3a(nr).into());
            continue;
        }
        let mut mat = Mat3A::ZERO;
        let mut tr = Vec3A::ZERO;
        for k in 0..inf.n as usize {
            let m = &bones[inf.slot[k] as usize];
            let w = inf.weight[k];
            mat += m.matrix3 * w;
            tr += m.translation * w;
        }
        out_pos.push((mat * p + tr).into());
        out_nrm.push((mat * nr).normalize_or_zero().into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_bone_names_have_engine_ids() {
        assert_eq!(bone_id_by_name("OS_L"), Some(BONE_OS_L));
        assert_eq!(bone_id_by_name("hand_r"), Some(BONE_HAND_R));
        assert_eq!(bone_id_by_name("unknown"), None);
    }

    /// A rig like the stock adults', without a mesh.
    fn rig() -> Rig {
        let def = Human {
            height: 1.77,
            seat_height: 0.82,
            links: vec![
                0.09, 0.0, 0.92, 0.09, -0.03, 0.53, 0.02, 1.17, 0.18, -0.05, 1.43, 0.44, -0.04,
                1.41, -0.02, 1.55, 0.69, -0.03, 1.43, 0.9, -0.03, 1.43,
            ],
            walk_param: [1.4, 80.0, 1.0, 1.0, 0.0],
            ..Default::default()
        };
        let j = Joints::from_links(&def.links);
        Rig::measure(&def, &j, &[])
    }

    fn walk(
        rig: &Rig,
        pose: &mut Pose,
        speed: f64,
        secs: f32,
        dt: f32,
        pos: &mut DVec3,
        mut check: impl FnMut(&Pose, &Posed, DVec3),
    ) {
        let steps = (secs / dt) as usize;
        for _ in 0..steps {
            *pos += DVec3::new(0.0, speed * dt as f64, 0.0);
            let input = PoseInput {
                activity: Activity::Walk,
                origin: *pos,
                velocity: DVec2::new(0.0, speed),
                ..Default::default()
            };
            pose.advance(rig, &input, dt);
            let posed = pose.bones(rig);
            check(pose, &posed, *pos);
        }
    }

    #[test]
    fn rig_is_sane() {
        let r = rig();
        assert!((r.thigh - 0.39).abs() < 0.01, "{}", r.thigh);
        assert!(r.shin > 0.35 && r.shin < 0.5, "{}", r.shin);
        assert!(r.heel < 0.0 && r.ball > 0.0 && r.toe > r.ball);
        assert!(r.seat_lift > 0.05 && r.seat_lift < 0.15);
        let c = r.cadence(1.35);
        assert!(c > 1.7 && c < 2.1, "cadence {c} steps/s at 1.35 m/s");
        // `[walk_param]` 1.4 (the stride) / 80 (an arm angle): 0.7 m steps
        assert!((r.walk_step - 0.7).abs() < 1e-6, "{}", r.walk_step);
        // slower than 1.2 m/s the stride shortens and the pace stays: 2.4 / 1.4 steps a second
        assert!((r.cadence(0.6) - 2.4 / 1.4).abs() < 1e-4, "{}", r.cadence(0.6));
    }

    #[test]
    fn standing_is_rest_like_and_finite() {
        let r = rig();
        let mut p = Pose::new(7);
        let input = PoseInput::default();
        for _ in 0..200 {
            p.advance(&r, &input, 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        for b in &posed.bones {
            assert!(b.is_finite());
        }
        for side in 0..2 {
            assert!(
                posed.sole[side].abs() < 0.01,
                "sole {} at {}",
                side,
                posed.sole[side]
            );
            assert!(
                posed.knee_flex[side] < 20.0,
                "knee {}",
                posed.knee_flex[side]
            );
            // the arms hang down beside the body
            assert!(posed.wrist[side].z < 1.0, "wrist z {}", posed.wrist[side].z);
            assert!(
                posed.wrist[side].x * SIDE[side] > 0.2,
                "wrist x {}",
                posed.wrist[side].x
            );
        }
    }

    #[test]
    fn walking_plants_the_feet() {
        let r = rig();
        let mut p = Pose::new(3);
        let mut pos = DVec3::ZERO;
        let dt = 1.0 / 60.0;
        let speed = 1.35;
        // get going
        walk(&r, &mut p, speed, 2.0, dt, &mut pos, |_, _, _| {});
        let mut last: [Option<(bool, DVec3)>; 2] = [None, None];
        let mut max_slide = 0.0f64;
        let mut knee_max = [0f32; 2];
        let mut knee_min = [180f32; 2];
        let mut lowest = 0f32;
        let mut miss = 0f32;
        let mut lifts = 0;
        let mut max_drop = 0f32;
        let (catch_ups, shuffles) = (p.catch_ups, p.shuffles);
        walk(&r, &mut p, speed, 4.0, dt, &mut pos, |pose, posed, _| {
            max_drop = max_drop.max(pose.drop);
            for side in 0..2 {
                let f = pose.feet[side];
                if let Some((was, at)) = last[side] {
                    if was && f.planted {
                        max_slide = max_slide.max((f.pos - at).length());
                    }
                    if was && !f.planted {
                        lifts += 1;
                    }
                }
                last[side] = Some((f.planted, f.pos));
                knee_max[side] = knee_max[side].max(posed.knee_flex[side]);
                knee_min[side] = knee_min[side].min(posed.knee_flex[side]);
                lowest = lowest.min(posed.sole[side]);
                if f.planted {
                    miss = miss.max(posed.leg_miss[side]);
                }
                assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()));
            }
        });
        assert!(max_slide < 1e-9, "a planted foot moved {max_slide} m");
        assert!(
            max_drop < 0.12,
            "the hips sank {max_drop} m in a steady walk"
        );
        assert_eq!(
            p.catch_ups, catch_ups,
            "steady walking needed catch-up steps"
        );
        assert_eq!(p.shuffles, shuffles, "a foot was pulled in while walking");
        // 1.35 m/s at ~1.8 steps/s: about 7 steps in 4 s
        assert!(lifts >= 6 && lifts <= 9, "{lifts} steps in 4 s");
        for side in 0..2 {
            assert!(
                knee_max[side] > 40.0 && knee_max[side] < 80.0,
                "knee flexes up to {}",
                knee_max[side]
            );
            assert!(
                knee_min[side] >= 0.0 && knee_min[side] < 15.0,
                "knee at least {}",
                knee_min[side]
            );
        }
        assert!(lowest > -0.015, "a sole went {lowest} m through the floor");
        assert!(miss < 0.03, "a planted foot was missed by {miss} m");
    }

    #[test]
    fn stopping_and_turning_bring_the_feet_back() {
        let r = rig();
        let mut p = Pose::new(11);
        let mut pos = DVec3::ZERO;
        walk(&r, &mut p, 1.2, 3.0, 1.0 / 60.0, &mut pos, |_, _, _| {});
        // stop and turn round on the spot
        let mut heading = 0.0;
        for k in 0..240 {
            if k < 90 {
                heading += 2.0;
            }
            let input = PoseInput {
                origin: pos,
                heading,
                ..Default::default()
            };
            p.advance(&r, &input, 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        assert!(p.feet.iter().all(|f| f.planted));
        for side in 0..2 {
            let (rest, yaw) = p.rest_target(&r, side, pos, heading, false);
            assert!(
                (p.feet[side].pos - rest).truncate().length() < 0.14,
                "foot {side} {:?} vs {:?}",
                p.feet[side].pos,
                rest
            );
            assert!(angle_diff(p.feet[side].yaw, yaw).abs() < 25.0);
            assert!(posed.sole[side].abs() < 0.02);
        }
    }

    #[test]
    fn sitting_down_puts_the_hips_on_the_seat() {
        let r = rig();
        let mut p = Pose::new(5);
        let seat = Vec3::new(0.0, -r.seat_front(), 0.45);
        let mut settling_seen = false;
        p.advance(&r, &PoseInput::default(), 1.0 / 60.0);
        for k in 0..120 {
            let input = PoseInput {
                activity: Activity::Sit,
                seat: Some(seat),
                ..Default::default()
            };
            p.advance(&r, &input, 1.0 / 60.0);
            let posed = p.bones(&r);
            settling_seen |= p.settling();
            assert!(
                posed.ok && posed.bones.iter().all(|b| b.is_finite()),
                "frame {k}"
            );
        }
        assert!(settling_seen);
        let input = PoseInput {
            activity: Activity::Sit,
            seat: Some(seat),
            ..Default::default()
        };
        for _ in 0..30 {
            p.advance(&r, &input, 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        let hips = (posed.hip[0] + posed.hip[1]) * 0.5;
        assert!(
            (hips.z - (seat.z + r.seat_lift)).abs() < 0.05,
            "hips at {:?}",
            hips
        );
        assert!((hips.y - seat.y).abs() < 0.08, "hips at {:?}", hips);
        for side in 0..2 {
            assert!(
                posed.knee_flex[side] > 60.0,
                "knee {}",
                posed.knee_flex[side]
            );
            assert!(
                posed.sole[side].abs() < 0.03,
                "feet on the floor: {}",
                posed.sole[side]
            );
            // hands in the lap, above the thighs
            assert!(
                posed.wrist[side].z > seat.z && posed.wrist[side].y > seat.y,
                "wrist {:?}",
                posed.wrist[side]
            );
        }
        // and up again
        let input = PoseInput::default();
        for _ in 0..90 {
            p.advance(&r, &input, 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        assert!(p.sit_amount() == 0.0);
        let hips = (posed.hip[0] + posed.hip[1]) * 0.5;
        assert!(hips.z > r.pelvis.z - 0.05, "stood up: {:?}", hips);
    }

    #[test]
    fn paying_reaches_the_desk_and_looks() {
        let r = rig();
        let mut p = Pose::new(9);
        let desk = Vec3::new(0.02, 0.5, 1.08);
        let input = PoseInput {
            activity: Activity::Pay,
            reach: Some(desk),
            look: Some(Vec3::new(-0.8, 0.9, 1.3)),
            ..Default::default()
        };
        for _ in 0..90 {
            p.advance(&r, &input, 1.0 / 60.0);
        }
        let posed = p.bones(&r);
        assert!(
            (posed.wrist[1] - desk).length() < 0.08,
            "wrist {:?}",
            posed.wrist[1]
        );
        assert!(p.head.x < -20.0, "head yaw {}", p.head.x);
    }

    #[test]
    fn jerky_crowd_motion_does_not_crouch() {
        // the crowd starts, stops, turns and shoves people about
        let r = rig();
        let mut p = Pose::new(4);
        let mut pos = DVec3::ZERO;
        let dt = 1.0 / 60.0;
        let mut worst_drop = 0f32;
        let mut worst_miss = 0f32;
        let mut worst_knee = 0f32;
        let mut deep = 0;
        for k in 0..1200 {
            let t = k as f64 * dt as f64;
            let v = match (t * 1.3) as i64 % 5 {
                0 => DVec2::new(0.0, 1.6),
                1 => DVec2::new(0.9, 0.3),
                2 => DVec2::ZERO,
                3 => DVec2::new(-1.4, -0.6),
                _ => DVec2::new(0.0, 0.25 * (t * 9.0).sin()),
            };
            pos += (v * dt as f64).extend(0.0);
            let heading = if v.length() > 0.2 {
                v.x.atan2(v.y).to_degrees()
            } else {
                0.0
            };
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                heading,
                velocity: v,
                ..Default::default()
            };
            p.advance(&r, &input, dt);
            let posed = p.bones(&r);
            worst_drop = worst_drop.max(p.drop);
            if p.drop > 0.11 {
                deep += 1;
            }
            for side in 0..2 {
                if p.feet[side].planted {
                    worst_miss = worst_miss.max(posed.leg_miss[side]);
                    worst_knee = worst_knee.max(posed.knee_flex[side]);
                }
            }
        }
        assert!(worst_drop <= 0.12, "the pelvis dropped {worst_drop} m");
        assert!(deep < 150, "the hips were low for {deep} of 1200 frames");
        assert!(
            worst_knee < 70.0,
            "a knee carrying weight bent {worst_knee} degrees"
        );
        assert!(
            worst_miss < 0.05,
            "a planted foot was missed by {worst_miss} m"
        );
    }

    #[test]
    fn bad_input_and_empty_rigs_stay_finite() {
        // a .hum without [links] and without a mesh
        let empty = Rig::measure(&Human::default(), &Joints::from_links(&[]), &[]);
        for r in [rig(), empty] {
            let mut p = Pose::new(1);
            let inputs = [
                PoseInput {
                    velocity: DVec2::new(f64::NAN, 1.0),
                    ..Default::default()
                },
                PoseInput {
                    origin: DVec3::new(1e7, -1e7, 30.0),
                    heading: 1e9,
                    velocity: DVec2::new(40.0, 0.0),
                    ..Default::default()
                },
                PoseInput {
                    activity: Activity::Sit,
                    seat: Some(Vec3::new(0.0, 5.0, -3.0)),
                    ..Default::default()
                },
                PoseInput {
                    activity: Activity::Pay,
                    reach: Some(Vec3::ZERO),
                    look: Some(Vec3::new(0.0, -0.0, 1.55)),
                    sway: Vec2::splat(1e6),
                    hold: 7.0,
                    ..Default::default()
                },
                PoseInput {
                    origin: DVec3::new(f64::NAN, 0.0, 0.0),
                    ..Default::default()
                },
            ];
            for (k, input) in inputs.iter().cycle().take(400).enumerate() {
                p.advance(&r, input, if k % 7 == 0 { 5.0 } else { 1.0 / 60.0 });
                let posed = p.bones(&r);
                assert!(
                    posed.ok && posed.bones.iter().all(|b| b.is_finite()),
                    "input {}",
                    k % inputs.len()
                );
            }
        }
    }

    #[test]
    fn a_step_up_is_climbed() {
        let r = rig();
        let mut p = Pose::new(2);
        let floor = |at: DVec2| -> Option<f64> { Some(if at.y > 1.0 { 0.3 } else { 0.0 }) };
        let mut pos = DVec3::ZERO;
        let dt = 1.0 / 60.0;
        for _ in 0..240 {
            pos.y += 0.9 * dt as f64;
            pos.z = if pos.y > 1.0 {
                (pos.z + 1.2 * dt as f64).min(0.3)
            } else {
                0.0
            };
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                velocity: DVec2::new(0.0, 0.9),
                floor: Some(&floor),
                ..Default::default()
            };
            p.advance(&r, &input, dt);
            let posed = p.bones(&r);
            assert!(posed.ok && posed.bones.iter().all(|b| b.is_finite()));
        }
        for f in &p.feet {
            if f.planted {
                assert!(
                    (f.pos.z - if f.pos.y > 1.0 { 0.3 } else { 0.0 }).abs() < 1e-6,
                    "{:?}",
                    f.pos
                );
            }
        }
    }

    /// Down a flight whose floor the feet only know in steps (a landing 0.7 m up, then the
    /// aisle) while the body slides down it evenly, as on the SD202's stairs: a foot left on
    /// the landing lifted the body's floor, the other one landing on the aisle below could
    /// not be reached and stepped again and again, and the first was dragged along the
    /// aisle a flight up. Once the body is down, both feet are down.
    #[test]
    fn a_foot_left_on_a_landing_comes_down() {
        let r = rig();
        let mut p = Pose::new(3);
        let floor = |at: DVec2| -> Option<f64> { Some(if at.x < -0.5 { 0.7 } else { 0.0 }) };
        let dt = 1.0 / 60.0;
        let mut pos = DVec3::new(-1.2, 0.0, 0.7);
        let mut worst: f64 = 0.0;
        for k in 0..400 {
            let v = if k < 30 { 0.0 } else { 0.8 };
            pos.x += v * dt as f64;
            pos.z = (0.7 * (-pos.x / 0.85)).clamp(0.0, 0.7);
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                heading: 90.0,
                velocity: DVec2::new(v, 0.0),
                floor: Some(&floor),
                ..Default::default()
            };
            p.advance(&r, &input, dt);
            assert!(p.bones(&r).ok);
            if pos.x > 1.2 {
                for f in p.feet.iter().filter(|f| f.planted) {
                    worst = worst.max(f.pos.z);
                }
            }
        }
        assert!(
            worst < 1e-6,
            "a foot stayed {worst:.2} m up ({})",
            p.describe()
        );
    }

    /// Stepping off a bus: the feet on its floor (0.45 m up) are carried into the ground's
    /// frame where they are, and within a second of walking on both are on the pavement.
    #[test]
    fn feet_come_off_the_bus_floor() {
        let r = rig();
        let mut p = Pose::new(4);
        let bus_floor = |_: DVec2| -> Option<f64> { Some(0.45) };
        let ground = |_: DVec2| -> Option<f64> { Some(0.0) };
        let dt = 1.0 / 60.0;
        let mut pos = DVec3::new(0.0, 0.0, 0.45);
        for _ in 0..90 {
            pos.y += 0.6 * dt as f64;
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                frame: 1,
                velocity: DVec2::new(0.0, 0.6),
                floor: Some(&bus_floor),
                ..Default::default()
            };
            p.advance(&r, &input, dt);
        }
        pos.z = 0.0;
        let mut worst: f64 = 0.0;
        for k in 0..120 {
            pos.y += 0.8 * dt as f64;
            let input = PoseInput {
                activity: Activity::Walk,
                origin: pos,
                frame: 0,
                velocity: DVec2::new(0.0, 0.8),
                floor: Some(&ground),
                ..Default::default()
            };
            p.advance(&r, &input, dt);
            assert!(p.bones(&r).ok);
            if k > 60 {
                for f in p.feet.iter().filter(|f| f.planted) {
                    worst = worst.max(f.pos.z);
                }
            }
        }
        assert!(
            worst < 1e-6,
            "a foot stayed {worst:.2} m up a second after stepping off ({})",
            p.describe()
        );
    }
}

/// The hands of a human type closed round a bar of `radius` (m; a steering wheel's rim):
/// every vertex of a hand beyond the knuckles is bent round an axis across the palm, as far
/// round as its distance from the knuckles reaches, so that the fingers wrap the bar from
/// the back of the hand to the palm. The rest pose stays: the hands lie palm down along the
/// arms, the thumbs (which sit before the knuckles) are left as they are. Returns positions
/// and normals per mesh, for [`skin_from`].
pub fn curl_hands(ty: &HumanType, radius: f32) -> Vec<(Vec<Vec3>, Vec<Vec3>)> {
    let rig = &ty.rig;
    let hand_len = (ty.joints.finger - ty.joints.hand).length().clamp(0.12, 0.3);
    let mut out: Vec<(Vec<Vec3>, Vec<Vec3>)> = ty.meshes.iter().map(|m| (m.data.positions.clone(), m.data.normals.clone())).collect();
    for side in 0..2 {
        let slot = HAND[side] as u8;
        let w = rig.wrist[side];
        let u = (rig.wrist[side] - rig.elbow[side]).normalize_or(Vec3::X * SIDE[side]);
        let p = -Vec3::Z;
        let kd = hand_len * 0.58;
        let owned = |inf: &Influence| (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.5);
        // the fingers' middle plane: their average height
        let (mut zs, mut n) = (0.0f32, 0.0f32);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let v = m.data.positions[i];
                if owned(inf) && (v - w).dot(u) > kd {
                    zs += v.z;
                    n += 1.0;
                }
            }
        }
        if n < 3.0 {
            continue;
        }
        let mut knuckle = w + u * kd;
        knuckle.z = zs / n;
        let centre = knuckle + p * radius;
        let axis = u.cross(p).normalize_or(Vec3::Y);
        // the fingers' spread (the rest hand's fingers are apart) and where the thumb begins:
        // across the hand, the thumb's side positive
        let side_sign = SIDE[side];
        let (mut c_sum, mut c_n, mut c_hi) = (0.0f32, 0.0f32, f32::MIN);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let rel = m.data.positions[i] - knuckle;
                if owned(inf) && rel.dot(u) > 0.01 {
                    let c = rel.dot(axis) * side_sign;
                    c_sum += c;
                    c_n += 1.0;
                    c_hi = c_hi.max(c);
                }
            }
        }
        let c_mid = if c_n > 0.0 { c_sum / c_n } else { 0.0 };
        let scale = hand_len / 0.19;
        let thumb_root = c_hi - 0.015 * scale;
        const MAX_TURN: f32 = 3.2;
        // a fist's fingers lie together; the thumb swings in under the bar
        const SQUEEZE: f32 = 0.55;
        const THUMB_TURN: f32 = 55.0;
        for (k, m) in ty.meshes.iter().enumerate() {
            for (i, inf) in m.skin.iter().enumerate() {
                if !owned(inf) {
                    continue;
                }
                let rel = m.data.positions[i] - knuckle;
                let s = rel.dot(u);
                let h = rel.dot(p);
                let c = rel.dot(axis) * side_sign;
                let thumb = smoothstep(thumb_root, thumb_root + 0.02 * scale, c) * (1.0 - smoothstep(-0.005 * scale, 0.012 * scale, s));
                if thumb > 0.0 {
                    let t = (THUMB_TURN * thumb).to_radians();
                    let (dc, dh) = (c - thumb_root, h);
                    let (c2, h2) = (thumb_root + dc * t.cos() - dh * t.sin(), dc * t.sin() + dh * t.cos());
                    out[k].0[i] = knuckle + u * s + p * h2 + axis * (c2 * side_sign);
                    // (axis = u x p: a turn about u takes axis towards -p)
                    out[k].1[i] = Quat::from_axis_angle(u, -t * side_sign) * m.data.normals[i];
                    continue;
                }
                if s <= 0.0 {
                    continue;
                }
                let squeeze = 1.0 - (1.0 - SQUEEZE) * smoothstep(0.0, 0.03 * scale, s);
                let c = c_mid + (c - c_mid) * squeeze;
                let across = axis * (c * side_sign);
                let phi = (s / radius).min(MAX_TURN);
                let extra = (s - MAX_TURN * radius).max(0.0);
                let r = (radius - h).max(0.003);
                let tangent = u * phi.cos() + p * phi.sin();
                out[k].0[i] = centre - p * (r * phi.cos()) + u * (r * phi.sin()) + tangent * extra + across;
                out[k].1[i] = Quat::from_axis_angle(axis, phi) * m.data.normals[i];
            }
        }
    }
    out
}

/// Where the fingers that [`curl_hands`] closed round a bar of `radius` hold it: the bar's
/// centre in the rest frame (left, right), for a caller that wants the bar exactly there.
pub fn grip_centres(ty: &HumanType, radius: f32) -> [Option<Vec3>; 2] {
    let rig = &ty.rig;
    let hand_len = (ty.joints.finger - ty.joints.hand).length().clamp(0.12, 0.3);
    [0, 1].map(|side| {
        let slot = HAND[side] as u8;
        let w = rig.wrist[side];
        let u = (rig.wrist[side] - rig.elbow[side]).normalize_or(Vec3::X * SIDE[side]);
        let kd = hand_len * 0.58;
        let owned = |inf: &Influence| (0..inf.n as usize).any(|k| inf.slot[k] == slot && inf.weight[k] > 0.5);
        let (mut zs, mut n) = (0.0f32, 0.0f32);
        for m in &ty.meshes {
            for (i, inf) in m.skin.iter().enumerate() {
                let v = m.data.positions[i];
                if owned(inf) && (v - w).dot(u) > kd {
                    zs += v.z;
                    n += 1.0;
                }
            }
        }
        if n < 3.0 {
            return None;
        }
        let mut knuckle = w + u * kd;
        knuckle.z = zs / n;
        Some(knuckle - Vec3::Z * radius)
    })
}

/// The bone slot of a hand (0 left, 1 right) in [`Posed::bones`].
pub fn hand_slot(side: usize) -> usize {
    HAND[side.min(1)]
}

/// [`skin`] of `mesh` with its positions and normals replaced (see [`curl_hands`]).
pub fn skin_from(
    mesh: &HumanMesh,
    rest: &(Vec<Vec3>, Vec<Vec3>),
    bones: &[Affine3A; SLOTS],
    out_pos: &mut Vec<Vec3>,
    out_nrm: &mut Vec<Vec3>,
) {
    let n = mesh.data.positions.len().min(rest.0.len());
    out_pos.clear();
    out_nrm.clear();
    for (i, inf) in mesh.skin.iter().enumerate().take(n) {
        let p = Vec3A::from(rest.0[i]);
        let nr = Vec3A::from(rest.1[i]);
        let mut mat = Mat3A::ZERO;
        let mut tr = Vec3A::ZERO;
        for k in 0..inf.n.max(1) as usize {
            let m = &bones[inf.slot[k] as usize];
            let w = if inf.n <= 1 { 1.0 } else { inf.weight[k] };
            mat += m.matrix3 * w;
            tr += m.translation * w;
        }
        out_pos.push((mat * p + tr).into());
        out_nrm.push((mat * nr).normalize_or_zero().into());
    }
}
