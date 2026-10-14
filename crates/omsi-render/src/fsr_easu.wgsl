// FSR 1.0 EASU (Edge-Adaptive Spatial Upsampling) - adapted for openOMSI
// Based on AMD FidelityFX FSR 1.0 (MIT licensed)

struct Params {
    // xy: input size, zw: output size
    input_size: vec2<f32>,
    output_size: vec2<f32>,
    // x: sharpness 0..1 (not used in EASU, for RCAS)
    sharpness: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> params: Params;
@group(0) @binding(1)
var t_src: texture_2d<f32>;
@group(0) @binding(2)
var s_src: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    var out: VsOut;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn min3(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return min(a, min(b, c));
}
fn max3(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return max(a, max(b, c));
}

fn saturate(num: f32) -> f32 {
    return clamp(num, 0.0, 1.0);
}

fn match_set(pp: vec2<f32>, bi_s: bool, bi_t: bool, bi_u: bool, bi_v: bool) -> f32 {
    if (bi_s) {
        return (1.0 - pp.x) * (1.0 - pp.y);
    } else if (bi_t) {
        return pp.x * (1.0 - pp.y);
    } else if (bi_u) {
        return (1.0 - pp.x) * pp.y;
    } else if (bi_v) {
        return pp.x * pp.y;
    } else {
        return 0.0;
    }
}

struct FsrTap {
    a_c: vec3<f32>,
    a_w: f32,
};

struct FsrSet {
    dir: vec2<f32>,
    len: f32,
};

// Filtering for a given tap for the scalar.
fn fsr_easu_tap(
    fsr: FsrTap,
    off: vec2<f32>,
    dir: vec2<f32>,
    len: vec2<f32>,
    lob: f32,
    clp: f32,
    c: vec3<f32>
) -> FsrTap {
    var fsr1: FsrTap = fsr;
    var a_c: vec3<f32> = fsr1.a_c;
    var a_w: f32 = fsr1.a_w;
    var v: vec2<f32> = vec2<f32>(0.0);
    v.x = (off.x * dir.x) + (off.y * dir.y);
    v.y = (off.x * -dir.y) + (off.y * dir.x);
    // Anisotropy.
    v = v * len;
    // Compute distance^2.
    var d2: f32 = v.x * v.x + v.y * v.y;
    // Limit to the window as at corner, 2 taps can easily be outside.
    d2 = min(d2, clp);
    // Approximation of lanczos2 without sin() or rcp(), or sqrt() to get x.
    //  (25/16 * (2/5 * x^2 - 1)^2 - (25/16 - 1)) * (1/4 * x^2 - 1)^2
    var w_b: f32 = 2.0 / 5.0 * d2 - 1.0;
    var w_a: f32 = lob * d2 - 1.0;
    w_b = w_b * w_b;
    w_a = w_a * w_a;
    w_b = 25.0 / 16.0 * w_b - (25.0 / 16.0 - 1.0);
    var w: f32 = w_b * w_a;
    // Do weighted average.
    a_c = a_c + c * w;
    a_w = a_w + w;
    fsr1.a_c = a_c;
    fsr1.a_w = a_w;
    return fsr1;
}

// Accumulate direction and length.
fn fsr_easu_set(
    fsr: FsrSet,
    pp: vec2<f32>,
    bi_s: bool, bi_t: bool, bi_u: bool, bi_v: bool,
    l_a: f32, l_b: f32, l_c: f32, l_d: f32, l_e: f32
) -> FsrSet {
    var fsr1: FsrSet = fsr;
    var dir: vec2<f32> = fsr1.dir;
    var len: f32 = fsr1.len;
    var w: f32 = 0.0;
    w = match_set(pp, bi_s, bi_t, bi_u, bi_v);
    // Direction is the '+' diff.
    //    a
    //  b c d
    //    e
    // Then takes magnitude from abs average of both sides of 'c'.
    // Length converts gradient reversal to 0, smoothly to non-reversal at 1, shaped, then adding horz and vert terms.
    var dc: f32 = l_d - l_c;
    var cb: f32 = l_c - l_b;
    var len_x: f32 = max(abs(dc), abs(cb));
    len_x = 1.0 / len_x;
    var dir_x: f32 = l_d - l_b;
    dir.x = dir.x + (dir_x * w);
    len_x = saturate(abs(dir_x) * len_x);
    len_x = len_x * len_x;
    len = len + (len_x * w);
    // Repeat for the y axis.
    var ec: f32 = l_e - l_c;
    var ca: f32 = l_c - l_a;
    var len_y: f32 = max(abs(ec), abs(ca));
    len_y = 1.0 / len_y;
    var dir_y: f32 = l_e - l_a;
    dir.y = dir.y + (dir_y * w);
    len_y = saturate(abs(dir_y) * len_y);
    len_y = len_y * len_y;
    len = len + (len_y * w);
    fsr1.dir = dir;
    fsr1.len = len;
    return fsr1;
}

fn gather_red(c: vec2<f32>) -> vec4<f32> {
    return textureGather(0, t_src, s_src, c);
}
fn gather_green(c: vec2<f32>) -> vec4<f32> {
    return textureGather(1, t_src, s_src, c);
}
fn gather_blue(c: vec2<f32>) -> vec4<f32> {
    return textureGather(2, t_src, s_src, c);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let input_size = params.input_size;
    let output_size = params.output_size;

    //------------------------------------------------------------------------------------------------------------------------------
    // Get position of 'f'.
    var pp: vec2<f32> = (floor(in.uv * output_size) + 0.5) / output_size * input_size - 0.5;
    var fp: vec2<f32> = floor(pp);
    pp = pp - fp;
    //------------------------------------------------------------------------------------------------------------------------------
    // 12-tap kernel.
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    // Gather 4 ordering.
    //  a b
    //  r g
    // For packed FP16, need either {rg} or {ab} so using the following setup for gather in all versions,
    //    a b    <- unused (z)
    //    r g
    //  a b a b
    //  r g r g
    //    a b
    //    r g    <- unused (z)
    // Allowing dead-code removal to remove the 'z's  
    var p0: vec2<f32> = fp + vec2<f32>(1.0, -1.0);
    // These are from p0 to avoid pulling two constants on pre-Navi hardware.
    var p1: vec2<f32> = p0 + vec2<f32>(-1.0, 2.0);
    var p2: vec2<f32> = p0 + vec2<f32>(1.0, 2.0);
    var p3: vec2<f32> = p0 + vec2<f32>(0.0, 4.0);

    p0 = p0 / input_size;
    p1 = p1 / input_size;
    p2 = p2 / input_size;
    p3 = p3 / input_size;

    var bczz_r: vec4<f32> = gather_red(p0);
    var bczz_g: vec4<f32> = gather_green(p0);
    var bczz_b: vec4<f32> = gather_blue(p0);
    var ijfe_r: vec4<f32> = gather_red(p1);
    var ijfe_g: vec4<f32> = gather_green(p1);
    var ijfe_b: vec4<f32> = gather_blue(p1);
    var klhg_r: vec4<f32> = gather_red(p2);
    var klhg_g: vec4<f32> = gather_green(p2);
    var klhg_b: vec4<f32> = gather_blue(p2);
    var zzon_r: vec4<f32> = gather_red(p3);
    var zzon_g: vec4<f32> = gather_green(p3);
    var zzon_b: vec4<f32> = gather_blue(p3);
    //------------------------------------------------------------------------------------------------------------------------------
    // Simplest multi-channel approximate luma possible (luma times 2, in 2 FMA/MAD).
    var bczz_l: vec4<f32> = bczz_b * 0.5 + (bczz_r * 0.5 + bczz_g);
    var ijfe_l: vec4<f32> = ijfe_b * 0.5 + (ijfe_r * 0.5 + ijfe_g);
    var klhg_l: vec4<f32> = klhg_b * 0.5 + (klhg_r * 0.5 + klhg_g);
    var zzon_l: vec4<f32> = zzon_b * 0.5 + (zzon_r * 0.5 + zzon_g);
    // Rename.
    var b_l: f32 = bczz_l.x;
    var c_l: f32 = bczz_l.y;
    var i_l: f32 = ijfe_l.x;
    var j_l: f32 = ijfe_l.y;
    var f_l: f32 = ijfe_l.z;
    var e_l: f32 = ijfe_l.w;
    var k_l: f32 = klhg_l.x;
    var l_l: f32 = klhg_l.y;
    var h_l: f32 = klhg_l.z;
    var g_l: f32 = klhg_l.w;
    var o_l: f32 = zzon_l.z;
    var n_l: f32 = zzon_l.w;
    // Accumulate for bilinear interpolation.
    var fsr: FsrSet;
    fsr.dir = vec2<f32>(0.0, 0.0);
    fsr.len = 0.0;
    fsr = fsr_easu_set(fsr, pp, true, false, false, false, b_l, e_l, f_l, g_l, j_l);
    fsr = fsr_easu_set(fsr, pp, false, true, false, false, c_l, f_l, g_l, h_l, k_l);
    fsr = fsr_easu_set(fsr, pp, false, false, true, false, f_l, i_l, j_l, k_l, n_l);
    fsr = fsr_easu_set(fsr, pp, false, false, false, true, g_l, j_l, k_l, l_l, o_l);
    //------------------------------------------------------------------------------------------------------------------------------
    // Normalize with approximation, and cleanup close to zero.
    var dir2: vec2<f32> = fsr.dir * fsr.dir;
    var dir_r: f32 = dir2.x + dir2.y;
    var zro: bool = dir_r < 1.0 / 32768.0;
    dir_r = 1.0 / sqrt(dir_r);
    if (zro) { dir_r = 1.0; }
    if (zro) { fsr.dir.x = 1.0; }
    fsr.dir = fsr.dir * dir_r;
    // Transform from {0 to 2} to {0 to 1} range, and shape with square.
    fsr.len = fsr.len * 0.5;
    fsr.len = fsr.len * fsr.len;
    // Stretch kernel {1.0 vert|horz, to sqrt(2.0) on diagonal}.
    var stretch: f32 = (fsr.dir.x * fsr.dir.x + fsr.dir.y * fsr.dir.y) * 1.0 / max(abs(fsr.dir.x), abs(fsr.dir.y));
    // Anisotropic length after rotation,
    //  x := 1.0 lerp to 'stretch' on edges
    //  y := 1.0 lerp to 2x on edges
    var len2: vec2<f32> = vec2<f32>(1.0 + (stretch - 1.0) * fsr.len, 1.0 - 0.5 * fsr.len);
    // Based on the amount of 'edge',
    // the window shifts from +/-{sqrt(2.0) to slightly beyond 2.0}.
    var lob: f32 = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * fsr.len;
    // Set distance^2 clipping point to the end of the adjustable window.
    var clp: f32 = 1.0 / lob;
    //------------------------------------------------------------------------------------------------------------------------------
    // Accumulation mixed with min/max of 4 nearest.
    //    b c
    //  e f g h
    //  i j k l
    //    n o
    var min4 = min(min3(vec3<f32>(ijfe_r.z, ijfe_g.z, ijfe_b.z), vec3<f32>(klhg_r.w, klhg_g.w, klhg_b.w), vec3<f32>(ijfe_r.y, ijfe_g.y, ijfe_b.y)),
        vec3<f32>(klhg_r.x, klhg_g.x, klhg_b.x));

    var max4: vec3<f32> = max(max3(vec3<f32>(ijfe_r.z, ijfe_g.z, ijfe_b.z), vec3<f32>(klhg_r.w, klhg_g.w, klhg_b.w), vec3<f32>(ijfe_r.y, ijfe_g.y, ijfe_b.y)),
        vec3<f32>(klhg_r.x, klhg_g.x, klhg_b.x));
    // Accumulation.
    var fsr2: FsrTap;
    fsr2.a_c = vec3<f32>(0.0, 0.0, 0.0);
    fsr2.a_w = 0.0;
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(0.0, -1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(bczz_r.x, bczz_g.x, bczz_b.x)); // b
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(1.0, -1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(bczz_r.y, bczz_g.y, bczz_b.y)); // c
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(-1.0, 1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(ijfe_r.x, ijfe_g.x, ijfe_b.x)); // i
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(0.0, 1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(ijfe_r.y, ijfe_g.y, ijfe_b.y)); // j
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(0.0, 0.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(ijfe_r.z, ijfe_g.z, ijfe_b.z)); // f
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(-1.0, 0.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(ijfe_r.w, ijfe_g.w, ijfe_b.w)); // e
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(1.0, 1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(klhg_r.x, klhg_g.x, klhg_b.x)); // k
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(2.0, 1.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(klhg_r.y, klhg_g.y, klhg_b.y)); // l
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(2.0, 0.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(klhg_r.z, klhg_g.z, klhg_b.z)); // h
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(1.0, 0.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(klhg_r.w, klhg_g.w, klhg_b.w)); // g
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(1.0, 2.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(zzon_r.z, zzon_g.z, zzon_b.z)); // o
    fsr2 = fsr_easu_tap(fsr2, vec2<f32>(0.0, 2.0) - pp, fsr.dir, len2, lob, clp, vec3<f32>(zzon_r.w, zzon_g.w, zzon_b.w)); // n
    //------------------------------------------------------------------------------------------------------------------------------
    // Normalize and dering.
    var c: vec3<f32> = min(max4, max(min4, fsr2.a_c * (1.0 / fsr2.a_w)));

    return vec4<f32>(c, 1.0);
}