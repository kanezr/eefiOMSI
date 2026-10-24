// Screen-space ambient occlusion from the single-sampled depth prepass, and a blur.
struct SsaoParams {
    inv_proj: mat4x4<f32>,
    // x radius (m), y strength, z width, w height
    params: vec4<f32>,
    // xy: the projection's off-centre shift (m20, m21), zero for a symmetric frustum
    shift: vec4<f32>,
};
@group(0) @binding(0) var<uniform> p: SsaoParams;
@group(0) @binding(1) var t_depth: texture_depth_2d;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // one triangle over the whole screen
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// view-space position of a pixel (reversed Z: the depth is 0 at the far plane)
fn view_pos(px: vec2<i32>) -> vec3<f32> {
    let size = vec2<i32>(i32(p.params.z), i32(p.params.w));
    let c = clamp(px, vec2<i32>(0), size - vec2<i32>(1));
    // the AO runs at half the picture's size; the depth prepass is full size
    let d = textureLoad(t_depth, c * 2, 0);
    let ndc = vec2<f32>((f32(c.x) + 0.5) / p.params.z * 2.0 - 1.0, 1.0 - (f32(c.y) + 0.5) / p.params.w * 2.0);
    let h = p.inv_proj * vec4<f32>(ndc, d, 1.0);
    return h.xyz / h.w;
}

// view-space depth (negative, metres) of a raw depth value
fn lin(d: f32) -> f32 {
    let h = p.inv_proj * vec4<f32>(0.0, 0.0, d, 1.0);
    return h.z / h.w;
}

// A 4x4 ordered pattern rather than white noise: every 4x4 block of pixels holds the same
// sixteen sample rotations, so the 5x5 blur that follows averages them out completely.
// White noise per pixel left a grain that stood still on the screen while the world moved
// under it - a faint film of static in front of the eyes.
fn hash(v: vec2<f32>) -> f32 {
    let p = vec2<i32>(i32(v.x) & 3, i32(v.y) & 3);
    let bayer = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (bayer[p.y * 4 + p.x] + 0.5) / 16.0;
}

@fragment
fn fs_ssao(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let d0 = textureLoad(t_depth, px * 2, 0);
    if (d0 <= 0.0) {
        return vec4<f32>(1.0, 60000.0, 0.0, 0.0); // sky
    }
    let pos = view_pos(px);
    // the normal from the depth neighbourhood (the smaller step of each pair, so an edge
    // between two objects does not bend it)
    let dx1 = view_pos(px + vec2<i32>(1, 0)) - pos;
    let dx2 = pos - view_pos(px - vec2<i32>(1, 0));
    let dy1 = view_pos(px + vec2<i32>(0, 1)) - pos;
    let dy2 = pos - view_pos(px - vec2<i32>(0, 1));
    let dx = select(dx2, dx1, length(dx1) < length(dx2));
    let dy = select(dy2, dy1, length(dy1) < length(dy2));
    var n = normalize(cross(dx, dy));
    if (dot(n, -pos) < 0.0) {
        n = -n;
    }
    // a hemisphere of samples around the normal, turned by a per-pixel angle
    let a = hash(in.clip.xy) * 6.2831853;
    let rnd = vec3<f32>(cos(a), sin(a), 0.0);
    let t = normalize(rnd - n * dot(rnd, n));
    let b = cross(n, t);
    // A metre and a half of radius is right for a house corner across the street, but in
    // a bus the hand straps darkened the ceiling a metre away from them, in blotches that
    // moved as the head turned: close to the camera the radius shrinks with the distance.
    let radius = min(p.params.x, 0.3 + 0.1 * -pos.z);
    var occ = 0.0;
    let count = 12;
    for (var i = 0; i < count; i = i + 1) {
        let fi = f32(i);
        // a spiral over the hemisphere, denser near the middle
        let ang = fi * 2.399963 + a;
        let r = (fi + 0.5) / f32(count);
        let scale = mix(0.15, 1.0, r * r);
        let s = vec3<f32>(cos(ang) * r, sin(ang) * r, sqrt(max(0.0, 1.0 - r * r)));
        let sp = pos + (t * s.x + b * s.y + n * s.z) * radius * scale;
        // back to the screen
        // project: we only have inv_proj, so search the depth at the sample's screen position
        // using the perspective relation x_ndc = x / (-z * tan) - reconstruct from two points
        let q = project(sp);
        if (q.x < 0.0 || q.y < 0.0 || q.x >= p.params.z || q.y >= p.params.w) {
            continue;
        }
        let qz = lin(textureLoad(t_depth, vec2<i32>(q) * 2, 0));
        // occluded when the scene surface is in front of the sample, within range
        let dz = qz - sp.z;
        let range = smoothstep(0.0, 1.0, radius / max(abs(pos.z - qz), 0.0001));
        if (dz > 0.03) {
            occ = occ + range;
        }
    }
    var ao = 1.0 - p.params.y * occ / f32(count);
    // far away a screen-space radius covers whole buildings: fade the effect out with
    // distance, where it only added shimmer
    let dist = -pos.z;
    ao = mix(ao, 1.0, clamp((dist - 60.0) / 140.0, 0.0, 1.0));
    // (clamp of a NaN is not safe on every GPU: a black blotch of shade on Windows)
    let ao_ok = select(1.0, clamp(ao, 0.0, 1.0), (bitcast<u32>(ao) & 0x7f800000u) != 0x7f800000u);
    return vec4<f32>(ao_ok, dist, 0.0, 0.0);
}

// pixel position of a view-space point, through the same projection the depth was made with
fn project(v: vec3<f32>) -> vec2<f32> {
    // inv_proj is the inverse of a perspective matrix: recover the projection's scale terms
    // from it (m00 = 1/(f/aspect), m11 = 1/f); an off-centre frustum (a headset eye, a
    // triple screen's side panel) adds its shift: x_ndc = (sx x + m20 z) / -z
    let sx = 1.0 / p.inv_proj[0][0];
    let sy = 1.0 / p.inv_proj[1][1];
    let ndc = vec2<f32>(v.x * sx, v.y * sy) / (-v.z) - p.shift.xy;
    return vec2<f32>((ndc.x + 1.0) * 0.5 * p.params.z, (1.0 - ndc.y) * 0.5 * p.params.w);
}

// --- blur: 5x5 box over the raw AO, keeps edges by depth
@group(0) @binding(2) var t_ao: texture_2d<f32>;

@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    let px = vec2<i32>(in.clip.xy);
    let size = vec2<i32>(i32(p.params.z), i32(p.params.w));
    // Raw AO already stores positive linear depth in .g (60000 for sky), so reuse it
    // for edge weights instead of reloading depth and reconstructing it for every tap.
    let center = textureLoad(t_ao, px, 0);
    let z0 = center.g;
    var sum = 0.0;
    var wsum = 0.0;
    // 6x6 window: covers every rotation of the 4x4 pattern at least once in each row and
    // column, and the depth weight keeps the occlusion from bleeding across an edge (a
    // relative tolerance: far surfaces are allowed a larger step)
    for (var y = -3; y <= 2; y = y + 1) {
        for (var x = -3; x <= 2; x = x + 1) {
            let c = clamp(px + vec2<i32>(x, y), vec2<i32>(0), size - vec2<i32>(1));
            let sample = textureLoad(t_ao, c, 0);
            let z = sample.g;
            let tol = 0.3 + 0.02 * abs(z0);
            let w = select(0.0, 1.0, abs(z - z0) < tol);
            sum = sum + sample.r * w;
            wsum = wsum + w;
        }
    }
    let blurred = select(1.0, sum / wsum, wsum > 0.0);
    return vec4<f32>(select(1.0, blurred, (bitcast<u32>(blurred) & 0x7f800000u) != 0x7f800000u), select(z0, 60000.0, center.g >= 60000.0), 0.0, 0.0);
}
