// Terrain shading: Bevy's standard lighting, then a haze that is thick near
// the ground and thins with height (like a real atmosphere). Distant
// lowlands fade into the sky while far, tall summits stay visible.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
    mesh_view_bindings::view,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

// One uniform block (WebGL2 allows only 14 per shader).
struct HazeU {
    // rgb: haze colour (linear)
    color: vec4<f32>,
    // x: haze density at sea level (per metre), y: scale height (metres),
    // z: how much a giant shades the player (the light is already dimmed by that)
    params: vec4<f32>,
    // xyz: direction toward the sun (or moon), w: how dark the giants' shadows are
    sun_dir: vec4<f32>,
    // Everest: x, z relative to the camera, spire height, spire radius
    everest: vec4<f32>,
    // Everest's body: height, radius, shape exponent
    everest_base: vec4<f32>,
    // St. Helens: x, z relative to the camera, cone height, cone radius
    helens: vec4<f32>,
}
@group(2) @binding(100) var<uniform> hz: HazeU;

fn everest_height(d: f32) -> f32 {
    return hz.everest.z * exp(-d / hz.everest.w) + hz.everest_base.x * exp(-pow(d / hz.everest_base.y, hz.everest_base.z));
}

fn helens_height(d: f32) -> f32 {
    return hz.helens.z * exp(-d / hz.helens.w);
}

// How much the two giant peaks shade this spot from the sun (0..1).
fn giant_shadow(p: vec3<f32>) -> f32 {
    let l = normalize(hz.sun_dir.xyz);
    let lh = l.xz;
    let lh2 = max(dot(lh, lh), 1e-6);
    var shade = 0.0;
    for (var k = 0; k < 2; k = k + 1) {
        var c = hz.everest.xy;
        var r = hz.everest.w;
        if (k == 1) {
            c = hz.helens.xy;
            r = hz.helens.w;
        }
        // where the ray toward the sun passes closest to the peak's axis
        let t_mid = dot(c - p.xz, lh) / lh2;
        if (t_mid <= 0.0) {
            continue;
        }
        let t0 = max(3000.0, t_mid - 3.0 * r);
        let t1 = t_mid + 2.0 * r;
        var worst = -1e9;
        for (var i = 0; i < 20; i = i + 1) {
            let t = mix(t0, t1, (f32(i) + 0.5) / 20.0);
            let q = p + l * t;
            let d = distance(q.xz, c);
            var top = helens_height(d);
            if (k == 0) {
                top = everest_height(d);
            }
            worst = max(worst, top - q.y);
        }
        let soft = 400.0 + 0.004 * t_mid;
        shade = max(shade, smoothstep(-soft, soft, worst));
    }
    return shade * hz.sun_dir.w;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    // the giants' shadows across the land. The light is already dimmed by
    // the shade where the player stands, so only the difference is applied.
    if (hz.sun_dir.w > 0.0) {
        let sh = giant_shadow(in.world_position.xyz);
        let ratio = clamp((1.0 - 0.6 * sh) / (1.0 - 0.6 * hz.params.z), 0.4, 2.5);
        out.color = vec4<f32>(out.color.rgb * ratio, out.color.a);
    }

    let cam = view.world_position;
    let p = in.world_position.xyz;
    let dist = distance(cam, p);
    let scale_h = hz.params.y;
    let h0 = max(cam.y, 0.0);
    let h1 = max(p.y, 0.0);
    let dh = h1 - h0;
    var k = exp(-h0 / scale_h);
    if (abs(dh) > 1.0) {
        k = (exp(-h0 / scale_h) - exp(-h1 / scale_h)) * scale_h / dh;
    }
    let f = 1.0 - exp(-hz.params.x * dist * k);
    out.color = vec4<f32>(mix(out.color.rgb, hz.color.rgb, f), out.color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
