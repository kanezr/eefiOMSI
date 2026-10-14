fn srgb_encode(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(1.055 * pow(x, vec3<f32>(1.0 / 2.4)) - 0.055, x * 12.92, x <= vec3<f32>(0.0031308));
}
fn srgb_decode(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3<f32>(0.0));
    return select(pow((x + 0.055) / 1.055, vec3<f32>(2.4)), x / 12.92, x <= vec3<f32>(0.04045));
}
override ENCODE_OUTPUT: bool = false;

// 1 on pipelines that draw into a non-sRGB view of an sRGB texture (classic frames):
// the blend unit then works on gamma-encoded values, as D3D9 does.
fn finish_output(c: vec3<f32>) -> vec3<f32> {
    return select(c, srgb_encode(c), ENCODE_OUTPUT);
}