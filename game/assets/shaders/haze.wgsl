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
    // ground detail and surf: x, z the camera's position wrapped every
    // 4,096 m, y seconds, w detail strength
    detail: vec4<f32>,
}
@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> hz: HazeU;

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

// A hash that repeats every `n` cells, so patterns tied to the ground stay
// put while the camera's position (passed wrapped every 4,096 m) wraps.
fn hash2(i: vec2<i32>, n: i32) -> f32 {
    // (i is never negative: see vnoise)
    let w = i % vec2<i32>(n, n);
    var h = u32(w.x) * 374761393u + u32(w.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    h = h ^ (h >> 16u);
    return f32(h & 0xffffu) / 65535.0;
}

// Smooth value noise with cells `cell` metres across (4,096 / cell must be whole).
fn vnoise(q: vec2<f32>, cell: f32) -> f32 {
    let n = i32(4096.0 / cell);
    // shifted by a whole number of repeats so the cells counted are never
    // negative (integer remainders of negative numbers differ on WebGL)
    let g = (q + vec2<f32>(65536.0, 65536.0)) / cell;
    let i = vec2<i32>(floor(g));
    let f = fract(g);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i, n);
    let b = hash2(i + vec2<i32>(1, 0), n);
    let c = hash2(i + vec2<i32>(0, 1), n);
    let d = hash2(i + vec2<i32>(1, 1), n);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Cobbles about half a metre across, with dark joints between them.
fn cobbles(q: vec2<f32>) -> f32 {
    let g = (q + vec2<f32>(65536.0, 65536.0)) / 0.5;
    let i = vec2<i32>(floor(g));
    let f = fract(g);
    var d1 = 9.0;
    var d2 = 9.0;
    var tone = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let c = i + vec2<i32>(x, y);
            let o = vec2<f32>(hash2(c, 8192), hash2(c + vec2<i32>(3, 7), 8192)) * 0.7 + 0.15;
            let d = length(vec2<f32>(f32(x), f32(y)) + o - f);
            if (d < d1) {
                d2 = d1;
                d1 = d;
                tone = hash2(c + vec2<i32>(11, 5), 8192);
            } else if (d < d2) {
                d2 = d;
            }
        }
    }
    let joint = smoothstep(0.03, 0.12, d2 - d1);
    return (0.55 + 0.45 * joint) * (0.88 + 0.24 * tone);
}

// Dressed stone in courses: blocks 1.28 m long and 0.64 m high, every other
// course offset by half a block. `along` runs along the wall.
fn ashlar(along: f32, y: f32) -> f32 {
    let ry = (y + 65536.0) / 0.64;
    let row = floor(ry);
    let ax = (along + 65536.0) / 1.28 + 0.5 * (row - 2.0 * floor(row * 0.5));
    let col = floor(ax);
    let jy = min(fract(ry), 1.0 - fract(ry));
    let jx = min(fract(ax), 1.0 - fract(ax));
    let joint = smoothstep(0.0, 0.06, jy) * smoothstep(0.0, 0.04, jx);
    let tone = hash2(vec2<i32>(i32(col), i32(row)), 3200);
    return (0.6 + 0.4 * joint) * (0.9 + 0.2 * tone);
}

// The Great Canyon's rock: level beds in five colours about 1.1 km thick
// with thin layers inside, dust on the benches, dark old rock in the inner
// gorge, and less light from the sky the deeper it goes. Linear colour.
// From far off the beds blend together rather than flicker.
fn strata(y: f32, up: f32, dist: f32) -> vec3<f32> {
    let t = y / 1150.0 + 0.35 * sin(y / 2900.0) + 0.2 * sin(y / 1370.0 + 1.3);
    let b = t - 5.0 * floor(t / 5.0);
    var c = vec3<f32>(0.60, 0.46, 0.38);
    if (b < 1.0) {
        c = vec3<f32>(0.76, 0.38, 0.22);
    } else if (b < 2.0) {
        c = vec3<f32>(0.86, 0.74, 0.55);
    } else if (b < 3.0) {
        c = vec3<f32>(0.68, 0.33, 0.20);
    } else if (b < 4.0) {
        c = vec3<f32>(0.80, 0.54, 0.34);
    }
    c = mix(c, vec3<f32>(0.74, 0.49, 0.34), smoothstep(150000.0, 600000.0, dist));
    c = c * (1.0 + (1.0 - smoothstep(5000.0, 40000.0, dist)) * (0.06 * sin(y * 0.0739) + 0.04 * sin(y * 0.273) - 0.08));
    c = mix(c, vec3<f32>(0.80, 0.66, 0.50), 0.35 * smoothstep(0.75, 0.95, up));
    c = mix(c, vec3<f32>(0.36, 0.32, 0.36), 1.0 - smoothstep(-33500.0, -31000.0, y));
    c = c * (1.0 - 0.38 * clamp(-y / 45000.0, 0.0, 1.0));
    return pow(c, vec3<f32>(2.2, 2.2, 2.2));
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
#ifdef VERTEX_UVS_A
    // uv: x how sandy (below 0: the Great Canyon; 2: the bridge's dressed
    // stone), y how rocky
    let wp = in.world_position.xyz;
    let dcam = distance(view.world_position, wp);
    let nrm = normalize(in.world_normal);
    var col = pbr_input.material.base_color.rgb;
    // the canyon's beds of rock stay level at any distance
    let cw = clamp(-in.uv.x, 0.0, 1.0);
    if (cw > 0.0) {
        col = mix(col, strata(wp.y, nrm.y, dcam), cw);
    }
    // close up, the ground gets texture: clumps in the grass, ripples in
    // the sand, cracks in the rock; cobbles and stone courses on the bridge
    let fade = (1.0 - smoothstep(80.0, 240.0, dcam)) * hz.detail.w;
    if (fade > 0.0) {
        let q = wp.xz + hz.detail.xz;
        // steep faces take their pattern from the wall, not the ground plan
        var qs = q;
        if (abs(nrm.y) < 0.6) {
            if (abs(nrm.x) > abs(nrm.z)) {
                qs = vec2<f32>(q.y, wp.y);
            } else {
                qs = vec2<f32>(q.x, wp.y);
            }
        }
        if (in.uv.x > 1.5) {
            var k = 1.0;
            if (abs(nrm.y) > 0.7) {
                k = cobbles(q);
            } else {
                k = ashlar(qs.x, wp.y);
            }
            col = col * mix(1.0, k, fade);
        } else {
            let sand = clamp(in.uv.x, 0.0, 1.0);
            let rock = clamp(in.uv.y, 0.0, 1.0);
            let grass = (1.0 - sand) * (1.0 - rock);
            let n1 = vnoise(qs, 2.0);
            let n2 = vnoise(qs, 0.5);
            var k = 1.0 + grass * (0.24 * (n1 - 0.5) + 0.18 * (n2 - 0.5));
            if (sand > 0.01) {
                let ripple = sin((q.x * 0.82 + q.y * 0.57) * 7.0 + n1 * 5.0) * (1.0 - smoothstep(40.0, 140.0, dcam));
                k = k + sand * (0.06 * ripple + 0.08 * (n2 - 0.5));
            }
            if (rock > 0.01) {
                let crack = 1.0 - abs(2.0 * vnoise(qs, 1.0) - 1.0);
                k = k - rock * (0.38 * smoothstep(0.88, 0.98, crack) - 0.2 * (n1 - 0.5) - 0.1 * (n2 - 0.5));
            }
            col = col * mix(1.0, k, fade);
            col = mix(col, col * vec3<f32>(1.08, 1.04, 0.86), grass * fade * smoothstep(0.6, 0.85, n1) * 0.6);
        }
    }
    // surf washing up the beaches and breaking at the foot of the cliffs
    if (wp.y > -1.0 && wp.y < 2.0 && dcam < 2500.0 && in.uv.x < 1.5 && (in.uv.x > 0.2 || in.uv.y > 0.5)) {
        let q2 = wp.xz + hz.detail.xz;
        let wash = 0.55 + 0.45 * sin(hz.detail.y * 0.9 + vnoise(q2, 32.0) * 6.283);
        let edge = wash * 0.9 - 0.15 + 0.25 * (vnoise(q2, 2.0) - 0.5);
        let foam = smoothstep(edge - 0.3, edge, wp.y) * (1.0 - smoothstep(edge, edge + 0.08, wp.y));
        let a = foam * 0.8 * (1.0 - smoothstep(800.0, 2500.0, dcam));
        col = mix(col, vec3<f32>(0.9, 0.94, 0.96), a);
    }
    pbr_input.material.base_color = vec4<f32>(col, pbr_input.material.base_color.a);
#endif
    out.color = apply_pbr_lighting(pbr_input);
    // where this point's light is worked out: the skirts that hide the
    // seams between chunks hang below their edge (the vertex colour's alpha
    // says how far), and are lit as if they were at the edge
    var p = in.world_position.xyz;
#ifdef VERTEX_COLORS
    p.y = p.y + (1.0 - in.color.a) * 100000.0;
#endif
    // the big peaks' shadows across the land. The light is already dimmed
    // by the shade where the player stands, so only the difference is applied.
    if (hz.sun_dir.w > 0.0) {
        let sh = giant_shadow(p);
        let ratio = clamp((1.0 - 0.6 * sh) / (1.0 - 0.6 * hz.params.z), 0.4, 2.5);
        out.color = vec4<f32>(out.color.rgb * ratio, out.color.a);
    }

    let cam = view.world_position;
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
