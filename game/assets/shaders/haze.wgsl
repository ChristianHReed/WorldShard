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
    // z: how much a big peak shades the player (the light is already dimmed by that)
    params: vec4<f32>,
    // xyz: direction toward the sun (or moon), w: how dark the peaks' shadows are
    sun_dir: vec4<f32>,
    // rgb: sunlight colour (linear), w: how much the haze glows toward the sun
    sun_col: vec4<f32>,
    // the region's ground mist: x density (per metre), y thickness (m),
    // z ground height under the player (m)
    mist: vec4<f32>,
    // rgb: mist colour (linear)
    mist_col: vec4<f32>,
    // the big peaks (the giants and their range), two entries each:
    // (x, z relative to the camera, spire height, spire radius),
    // (body height, body radius, body shape, how far its shadow reaches sideways)
    peaks: array<vec4<f32>, 24>,
}
@group(2) @binding(100) var<uniform> hz: HazeU;

const SHADOW_STEPS: i32 = 14;

fn peak_top(a: vec4<f32>, b: vec4<f32>, d: f32) -> f32 {
    var top = a.z * exp(-d / a.w);
    if (b.x > 0.0) {
        top = top + b.x * exp(-pow(d / b.y, b.z));
    }
    return top;
}

// How much the big peaks shade this spot from the sun (0..1).
fn giant_shadow(p: vec3<f32>) -> f32 {
    let l = normalize(hz.sun_dir.xyz);
    let lh = l.xz;
    let lh2 = max(dot(lh, lh), 1e-6);
    let lhl = sqrt(lh2);
    var shade = 0.0;
    for (var k = 0; k < 12; k = k + 1) {
        let a = hz.peaks[2 * k];
        let b = hz.peaks[2 * k + 1];
        if (a.z <= 0.0) {
            continue;
        }
        let c = a.xy;
        // where the ray toward the sun passes closest to the peak's axis
        let t_mid = dot(c - p.xz, lh) / lh2;
        if (t_mid <= 0.0) {
            continue;
        }
        if (length(p.xz + lh * t_mid - c) > b.w) {
            continue;
        }
        let r = b.w / lhl;
        let t0 = max(3000.0, t_mid - r);
        let t1 = t_mid + r;
        var worst = -1e9;
        for (var i = 0; i < SHADOW_STEPS; i = i + 1) {
            let t = mix(t0, t1, (f32(i) + 0.5) / f32(SHADOW_STEPS));
            let q = p + l * t;
            worst = max(worst, peak_top(a, b, distance(q.xz, c)) - q.y);
        }
        let soft = 400.0 + 0.004 * t_mid;
        shade = max(shade, smoothstep(-soft, soft, worst));
    }
    return shade * hz.sun_dir.w;
}

// The fraction of light lost through a layer that thins with height
// (scale height `sh`), from height h0 to h1 over `dist` metres.
fn layer_fog(density: f32, sh: f32, h0: f32, h1: f32, dist: f32) -> f32 {
    let dh = h1 - h0;
    var k = exp(-h0 / sh);
    if (abs(dh) > 1.0) {
        k = (exp(-h0 / sh) - exp(-h1 / sh)) * sh / dh;
    }
    return 1.0 - exp(-density * dist * k);
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
    // the big peaks' shadows across the land. The light is already dimmed
    // by the shade where the player stands, so only the difference is applied.
    if (hz.sun_dir.w > 0.0) {
        let sh = giant_shadow(in.world_position.xyz);
        let ratio = clamp((1.0 - 0.6 * sh) / (1.0 - 0.6 * hz.params.z), 0.4, 2.5);
        out.color = vec4<f32>(out.color.rgb * ratio, out.color.a);
    }

    let cam = view.world_position;
    let p = in.world_position.xyz;
    let dist = distance(cam, p);
    let view_dir = (p - cam) / max(dist, 1.0);
    // haze that thins with height, glowing warmer toward the sun
    let f = layer_fog(hz.params.x, hz.params.y, max(cam.y, 0.0), max(p.y, 0.0), dist);
    let toward_sun = pow(max(dot(view_dir, normalize(hz.sun_dir.xyz)), 0.0), 6.0);
    let haze = mix(hz.color.rgb, hz.sun_col.rgb, toward_sun * hz.sun_col.w);
    out.color = vec4<f32>(mix(out.color.rgb, haze, f), out.color.a);
    // the region's low ground mist (forest mist, swamp fog, desert dust)
    if (hz.mist.x > 0.0) {
        let g = hz.mist.z;
        let fm = layer_fog(hz.mist.x, hz.mist.y, max(cam.y - g, 0.0), max(p.y - g, 0.0), dist);
        out.color = vec4<f32>(mix(out.color.rgb, hz.mist_col.rgb, fm), out.color.a);
    }

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
