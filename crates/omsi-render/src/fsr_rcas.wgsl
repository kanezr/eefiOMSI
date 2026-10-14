// FSR 1.0 RCAS (Robust Contrast-Adaptive Sharpening) - adapted for openOMSI
// Based on AMD FidelityFX FSR 1.0 (MIT licensed)

struct Params {
    // xy: input size, zw: output size (for RCAS, input==output)
    input_size: vec2<f32>,
    output_size: vec2<f32>,
    // x: sharpness 0..1
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

fn min3f(a: f32, b: f32, c: f32) -> f32 {
    return min(a, min(b, c));
}
fn max3f(a: f32, b: f32, c: f32) -> f32 {
    return max(a, max(b, c));
}

fn saturate(num: f32) -> f32 {
    return clamp(num, 0.0, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let output_size = params.output_size;
    let sharpness = params.sharpness;

    // Algorithm uses minimal 3x3 pixel neighborhood.
    //    b 
    //  d e f
    //    h
    let texel = 1.0 / output_size;
    
    let b: vec3<f32> = textureSample(t_src, s_src, in.uv + vec2<f32>(0.0, -texel.y)).rgb;
    let d: vec3<f32> = textureSample(t_src, s_src, in.uv + vec2<f32>(-texel.x, 0.0)).rgb;
    var e: vec3<f32> = textureSample(t_src, s_src, in.uv).rgb;
    let f: vec3<f32> = textureSample(t_src, s_src, in.uv + vec2<f32>(texel.x, 0.0)).rgb;
    let h: vec3<f32> = textureSample(t_src, s_src, in.uv + vec2<f32>(0.0, texel.y)).rgb;

    // Rename (32-bit) or regroup (16-bit).
    var b_r: f32 = b.r;
    var b_g: f32 = b.g;
    var b_b: f32 = b.b;
    var d_r: f32 = d.r;
    var d_g: f32 = d.g;
    var d_b: f32 = d.b;
    var e_r: f32 = e.r;
    var e_g: f32 = e.g;
    var e_b: f32 = e.b;
    var f_r: f32 = f.r;
    var f_g: f32 = f.g;
    var f_b: f32 = f.b;
    var h_r: f32 = h.r;
    var h_g: f32 = h.g;
    var h_b: f32 = h.b;

    var nz: f32 = 0.0;

    // Luma times 2.
    var b_l: f32 = b_b * 0.5 + (b_r * 0.5 + b_g);
    var d_l: f32 = d_b * 0.5 + (d_r * 0.5 + d_g);
    var e_l: f32 = e_b * 0.5 + (e_r * 0.5 + e_g);
    var f_l: f32 = f_b * 0.5 + (f_r * 0.5 + f_g);
    var h_l: f32 = h_b * 0.5 + (h_r * 0.5 + h_g);

    // Noise detection.
    nz = 0.25 * b_l + 0.25 * d_l + 0.25 * f_l + 0.25 * h_l - e_l;
    nz = saturate(abs(nz) * 1.0 / (max3f(max3f(b_l, d_l, e_l), f_l, h_l) - min3f(min3f(b_l, d_l, e_l), f_l, h_l)));
    nz = -0.5 * nz + 1.0;

    // Min and max of ring.
    var mn4_r: f32 = min(min3f(b_r, d_r, f_r), h_r);
    var mn4_g: f32 = min(min3f(b_g, d_g, f_g), h_g);
    var mn4_b: f32 = min(min3f(b_b, d_b, f_b), h_b);
    var mx4_r: f32 = max(max3f(b_r, d_r, f_r), h_r);
    var mx4_g: f32 = max(max3f(b_g, d_g, f_g), h_g);
    var mx4_b: f32 = max(max3f(b_b, d_b, f_b), h_b);
    // Immediate constants for peak range.
    var peak_c: vec2<f32> = vec2<f32>(1.0, -1.0 * 4.0);
    // Limiters, these need to be high precision RCPs.
    var hit_min_r: f32 = min(mn4_r, e_r) * 1.0 / (4.0 * mx4_r);
    var hit_min_g: f32 = min(mn4_g, e_g) * 1.0 / (4.0 * mx4_g);
    var hit_min_b: f32 = min(mn4_b, e_b) * 1.0 / (4.0 * mx4_b);
    var hit_max_r: f32 = (peak_c.x - max(mx4_r, e_r)) * 1.0 / (4.0 * mn4_r + peak_c.y);
    var hit_max_g: f32 = (peak_c.x - max(mx4_g, e_g)) * 1.0 / (4.0 * mn4_g + peak_c.y);
    var hit_max_b: f32 = (peak_c.x - max(mx4_b, e_b)) * 1.0 / (4.0 * mn4_b + peak_c.y);
    var lobe_r: f32 = max(-hit_min_r, hit_max_r);
    var lobe_g: f32 = max(-hit_min_g, hit_max_g);
    var lobe_b: f32 = max(-hit_min_b, hit_max_b);
    let fsr_rcas_limit: f32 = 0.25 - (1.0 / 16.0);
    var lobe: f32 = max(-fsr_rcas_limit, min(max3f(lobe_r, lobe_g, lobe_b), 0.0)) * sharpness;

    // Apply noise removal.
    lobe = lobe * nz;

    // Resolve, which needs the medium precision rcp approximation to avoid visible tonality changes.
    var rcp_l: f32 = 1.0 / (4.0 * lobe + 1.0);
    var c: vec3<f32> = vec3<f32>(
        (lobe * b_r + lobe * d_r + lobe * h_r + lobe * f_r + e_r) * rcp_l,
        (lobe * b_g + lobe * d_g + lobe * h_g + lobe * f_g + e_g) * rcp_l,
        (lobe * b_b + lobe * d_b + lobe * h_b + lobe * f_b + e_b) * rcp_l
    );
    return vec4<f32>(c, 1.0);
}