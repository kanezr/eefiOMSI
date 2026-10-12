//! The body's and the meshes' transforms in the world, the cameras, and the skinned vertices.

use super::*;

impl VehicleInstance {
    /// Rotation of the vehicle body (model frame → world, without translation).
    pub fn body_rotation(&self) -> Mat4 {
        body_rotation(self.heading, self.pitch, self.bank, body_reversed(&self.ty.def, false))
    }

    pub fn body_heading(&self) -> f64 {
        body_heading(&self.ty.def, self.heading, false)
    }

    /// World transform of the vehicle body (f32; for local computations such as sound
    /// positions - use `body_rotation` + `position` for rendering).
    pub fn world_transform(&self) -> Mat4 {
        Mat4::from_translation(self.position.as_vec3()) * self.body_rotation()
    }

    /// Transform for mesh `i` relative to the vehicle position (rotation + animation). A
    /// flat shadow blob (`[isshadow]`, drawn `[matl_noZcheck]` in the original, i.e. over the
    /// road whatever the depth) is put onto the plane the wheels stand on: in the model it
    /// lies at z = 0, which the springs' sag takes 10-16 cm under the road, where no depth
    /// bias brings it through.
    pub fn mesh_local_transform(&self, i: usize) -> Mat4 {
        let xf = self.mesh_transforms.get(i).copied().unwrap_or(Mat4::IDENTITY);
        if is_shadow_mesh(&self.ty, i) {
            return self.body_rotation()
                * onto_plane(self.contact_plane())
                * xf;
        }
        self.body_rotation() * xf
    }

    /// The plane the wheels stand on, in the body frame: z = p[0] + p[1]·x + p[2]·y (m).
    /// The rigid body's tyres touch it a radius under their hubs (which the springs have
    /// pushed up into the body), the simple physics asks the ground under each wheel; an
    /// AI copy stands `ai_rest_offset` above its plane.
    pub fn contact_plane(&self) -> [f32; 3] {
        if self.ty.def.is_rail() {
            return [0.0, 0.0, 0.0];
        }
        let mut points: Vec<Vec3> = Vec::new();
        if let Some(rb) = &self.rigid {
            let standing = rb.wheels.iter().any(|w| w.on_ground);
            for w in rb.wheels.iter().filter(|w| w.on_ground || !standing) {
                points.push(
                    w.attach + Vec3::Z * (w.compression.max(-crate::rigid::DROOP) - w.radius),
                );
            }
        } else {
            let rot = self.body_rotation();
            let inv = rot.inverse();
            for w in self.physics.wheels.iter().flatten() {
                let p = self.position
                    + rot
                        .transform_vector3(Vec3::new(w.lat, w.long, 0.0))
                        .as_dvec3();
                if let Some(z) = wheel_ground(self.contact.as_deref(), self.ground.as_deref(), p) {
                    points.push(
                        inv.transform_vector3((DVec3::new(p.x, p.y, z) - self.position).as_vec3()),
                    );
                }
            }
        }
        if points.is_empty() {
            return [-self.ai_rest_offset().0, 0.0, 0.0];
        }
        fit_plane(&points)
    }

    /// The ground its `[smoke]` puffs are set off over, as a plane of the body frame (see
    /// `contact_plane`): the driven one's where its tyres touch the road; an AI copy's the
    /// plane it is placed `ai_rest_offset` over (the road its axles were set on), without
    /// asking the ground under every wheel again for the exhaust of every car on the map.
    pub(super) fn particle_ground(&self) -> [f32; 3] {
        if self.rigid.is_some() {
            self.contact_plane()
        } else {
            [-self.ai_rest_offset().0, 0.0, 0.0]
        }
    }

    /// Position/direction of a `.bus` camera in world space: (eye, yaw, pitch).
    pub fn camera_world(&self, cam: &omsi_vehicle::Camera) -> (DVec3, f32, f32) {
        let local = Vec3::new(cam.pos[0], cam.pos[1], cam.pos[2]);
        let eye = self.position + self.body_rotation().transform_point3(local).as_dvec3();
        (eye, self.body_heading() as f32 + cam.yaw, cam.pitch)
    }

    /// A camera fixed to the body as OMSI keeps one (`[add_camera_reflexion]`: Omsi.exe
    /// 0x7edfd0 puts it in the vehicle's own matrix): its eye, and yaw, pitch and roll (deg)
    /// of its view with the body's pitch and bank in them - a mirror leans with the bus. A
    /// `dist` above zero puts the eye that far behind the point along the view.
    pub fn camera_world_full(&self, cam: &omsi_vehicle::Camera) -> (DVec3, f32, f32, f32) {
        camera_in_body(self.position, self.body_rotation(), cam)
    }
}

/// A camera in a body's own matrix (`rot`, at `position`): see
/// `VehicleInstance::camera_world_full`.
pub(super) fn camera_in_body(position: DVec3, rot: Mat4, cam: &omsi_vehicle::Camera) -> (DVec3, f32, f32, f32) {
    let (sy, cy) = cam.yaw.to_radians().sin_cos();
    let (sp, cp) = cam.pitch.to_radians().sin_cos();
    let f_local = Vec3::new(sy * cp, cy * cp, sp);
    let r_local = Vec3::new(cy, -sy, 0.0);
    let f = rot.transform_vector3(f_local).normalize_or(Vec3::Y);
    let up = rot.transform_vector3(r_local.cross(f_local)).normalize_or(Vec3::Z);
    let local = Vec3::new(cam.pos[0], cam.pos[1], cam.pos[2]);
    let eye = position + rot.transform_point3(local).as_dvec3() - (f * cam.dist.max(0.0)).as_dvec3();
    let yaw = f.x.atan2(f.y).to_degrees();
    let pitch = f.z.clamp(-1.0, 1.0).asin().to_degrees();
    // (the roll the renderer's `Camera::up` turns back into this up)
    let r0 = Vec3::new(f.y, -f.x, 0.0).normalize_or(Vec3::X);
    let u0 = r0.cross(f);
    let roll = up.dot(r0).atan2(up.dot(u0)).to_degrees();
    (eye, yaw, pitch, roll)
}

pub(super) fn skin_key(ty: &VehicleType, i: usize, transforms: &[Mat4]) -> Vec<Mat4> {
    let Some(vm) = ty.meshes.get(i) else {
        return Vec::new();
    };
    let mut key = vec![transforms.get(i).copied().unwrap_or(Mat4::IDENTITY)];
    for b in &vm.skin {
        if let Some(k) = ty.meshes.iter().position(|m| Some(m.def_index) == b.def_index) {
            key.push(transforms.get(k).copied().unwrap_or(Mat4::IDENTITY));
        }
    }
    key
}

/// The transforms of every mesh with all variables at 0: the pose the model was built in
/// (what a skinned mesh's vertices are given in).
pub(super) fn rest_transforms(animators: &[MeshAnimator], n_vars: usize) -> Vec<Mat4> {
    let zeros = vec![0.0; n_vars];
    let mut probes = animators.to_vec();
    let mut out: Vec<Mat4> = probes
        .iter_mut()
        .map(|a| {
            for (_, _, st) in a.anims.iter_mut() {
                st.initialized = false;
            }
            a.update(0.0, &zeros)
        })
        .collect();
    crate::anim::apply_parents(&probes, &mut out);
    out
}

/// The vertices (positions, normals) of `[smoothskin]` mesh `i` with its bones where
/// `transforms` has them, in the mesh's own frame (the renderer puts `transforms[i]` on
/// top); `rest` are the transforms of the modelled pose. None for a mesh without bones or
/// vertices.
///
/// As in Omsi.exe, the bones move the vertices first and the mesh's own motion (its
/// animations and its `[animparent]`) comes on top: the Agora L's rear half of the bellows
/// (`gelenk_B`) hangs on the arch that turns by half the joint's angle and takes the same
/// bones as the front half (a quarter and a half of it), so that its far ring ends up at
/// the whole angle, with the rear section. Put in place of the mesh's motion, the bones
/// held that ring at half the angle and the bellows fanned out across the bend.
///
/// A vertex no bone holds stays with the mesh, and so does the share of a vertex that an
/// unbound bone holds: dropping that share and making up the rest to 1, a vertex hung half
/// on the armature's fixed root moved all the way with the other bone - the Agora's retarder
/// lever bent out of shape.
pub fn skin_vertices(
    ty: &VehicleType,
    i: usize,
    transforms: &[Mat4],
    rest: &[Mat4],
) -> Option<(Vec<Vec3>, Vec<Vec3>)> {
    let vm = ty.meshes.get(i)?;
    let n = vm.data.positions.len();
    if vm.skin.is_empty()
        || n == 0
        || transforms.len() < ty.meshes.len()
        || rest.len() < ty.meshes.len()
    {
        return None;
    }
    let mut sum = vec![Mat4::ZERO; n];
    let mut total = vec![0.0f32; n];
    for b in &vm.skin {
        let bone = match b.def_index {
            None => Mat4::IDENTITY,
            Some(d) => {
                let Some(k) = ty.meshes.iter().position(|m| m.def_index == d) else {
                    continue;
                };
                transforms[k] * rest[k].inverse()
            }
        };
        for &(v, w) in &b.weights {
            let v = v as usize;
            if v < n && w.is_finite() && w > 0.0 {
                sum[v] += bone * w;
                total[v] += w;
            }
        }
    }
    // (the renderer puts the mesh's transform on top: in the modelled pose, the vertices
    // are where the file has them)
    let own_rest_inv = rest[i].inverse();
    let mut pos = Vec::with_capacity(n);
    let mut nrm = Vec::with_capacity(n);
    for v in 0..n {
        let p = vm.data.positions[v];
        let q = vm.data.normals.get(v).copied().unwrap_or(Vec3::Z);
        if total[v] < 1e-4 {
            pos.push(p);
            nrm.push(q);
            continue;
        }
        let m = own_rest_inv * (sum[v] * (1.0 / total[v]));
        pos.push(m.transform_point3(p));
        nrm.push(m.transform_vector3(q).normalize_or_zero());
    }
    Some((pos, nrm))
}
