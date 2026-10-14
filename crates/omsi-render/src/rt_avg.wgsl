// Enhanced+ (rt.rs): each texture's mean colour (sampled 8 x 8 from a small level), for the hits
@group(0) @binding(0) var avg_tex: texture_2d<f32>;
@group(0) @binding(1) var avg_samp: sampler;
@group(0) @binding(2) var<storage, read_write> avg_out: vec4<f32>;
var<workgroup> avg_sum: array<vec4<f32>, 64>;

@compute @workgroup_size(8, 8)
fn cs_tex_avg(@builtin(local_invocation_id) lid: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    let levels = f32(textureNumLevels(avg_tex));
    let lod = max(levels - 4.0, 0.0);
    let uv = (vec2<f32>(lid.xy) + vec2<f32>(0.5)) / 8.0;
    avg_sum[li] = textureSampleLevel(avg_tex, avg_samp, uv, lod);
    workgroupBarrier();
    if (li == 0u) {
        var s = vec4<f32>(0.0);
        for (var i = 0u; i < 64u; i++) {
            s += avg_sum[i];
        }
        avg_out = s / 64.0;
    }
}
