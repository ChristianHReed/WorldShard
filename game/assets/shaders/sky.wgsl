// The sky and the Cloudwall.
//
// Sky (mode 0): a gradient from the horizon to the zenith, the sun and its
// glow, the moon and stars at night, and two layers of moving clouds: puffy
// clouds a couple of kilometres up, and thin high streaks above them.
//
// Cloudwall (mode 1): the towering ring of cloud at the edge of the Rim Sea,
// with billows lit by the sun, darker bases and storm sections, fading into
// the haze near the sea.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
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
    pbr_functions::main_pass_post_lighting_processing,
}
#endif

// One uniform block (WebGL2 allows only 14 per shader).
struct SkyU {
    // xyz: toward the sun; w: daylight 0..1
    sun: vec4<f32>,
    // rgb: sunlight colour (linear); w: time in seconds (for drifting clouds)
    sun_col: vec4<f32>,
    // rgb: sky colour overhead (linear)
    zenith: vec4<f32>,
    // rgb: sky colour at the horizon (linear); w: 0 = sky, 1 = Cloudwall
    horizon: vec4<f32>,
    // x, z: player position in km; y: camera height above its ground (m);
    // w: camera height above the sea (m)
    cam: vec4<f32>,
    // rgb: haze colour (linear); w: haze density at sea level (per metre)
    haze: vec4<f32>,
    // x: haze scale height (m); y: cloud cover 0..1
    misc: vec4<f32>,
}
@group(2) @binding(100) var<uniform> sky: SkyU;

// ---- noise (integer hashing, so it stays clean far from the centre) ----
fn hash_u(v: u32) -> u32 {
    let s = v * 747796405u + 2891336453u;
    let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
    return (w >> 22u) ^ w;
}
fn hash2(i: vec2<f32>) -> f32 {
    let a = bitcast<u32>(i32(i.x));
    let b = bitcast<u32>(i32(i.y));
    return f32(hash_u(a ^ hash_u(b))) / 4294967295.0;
}
fn hash3(i: vec3<f32>) -> f32 {
    let a = bitcast<u32>(i32(i.x));
    let b = bitcast<u32>(i32(i.y));
    let c = bitcast<u32>(i32(i.z));
    return f32(hash_u(a ^ hash_u(b ^ hash_u(c)))) / 4294967295.0;
}
fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
fn fbm(p0: vec2<f32>, octaves: i32) -> f32 {
    var p = p0;
    var amp = 0.5;
    var sum = 0.0;
    for (var i = 0; i < octaves; i = i + 1) {
        sum = sum + amp * vnoise(p);
        p = mat2x2<f32>(1.6, 1.2, -1.2, 1.6) * p;
        amp = amp * 0.5;
    }
    return sum;
}

// ---- the sky ----
fn sky_gradient(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    var c = mix(sky.horizon.rgb, sky.zenith.rgb, pow(up, 0.55));
    if (dir.y < 0.0) {
        c = mix(sky.horizon.rgb, sky.haze.rgb, clamp(-dir.y * 8.0, 0.0, 1.0));
    }
    let l = normalize(sky.sun.xyz);
    let sd = max(dot(dir, l), 0.0);
    let day = sky.sun.w;
    // glow around the sun, and its disc
    c = c + sky.sun_col.rgb * (0.22 * pow(sd, 8.0) + 0.35 * pow(sd, 90.0)) * day;
    c = c + sky.sun_col.rgb * smoothstep(0.99955, 0.99975, sd) * 6.0 * day;
    // at night: the moon (opposite the sun) and stars
    let night = 1.0 - day;
    if (night > 0.02 && dir.y > -0.02) {
        let md = dot(dir, -l);
        c = c + vec3<f32>(0.85, 0.88, 0.95) * smoothstep(0.99962, 0.9998, md) * 1.6 * night;
        c = c + vec3<f32>(0.25, 0.3, 0.42) * pow(max(md, 0.0), 60.0) * 0.25 * night;
        let g = dir * 420.0;
        let cell = floor(g);
        let h = hash3(cell);
        if (h > 0.9965) {
            let off = fract(g) - vec3<f32>(0.5);
            let twinkle = 0.65 + 0.35 * sin(sky.sun_col.w * (2.0 + 3.0 * fract(h * 917.0)) + h * 400.0);
            let star = smoothstep(0.32, 0.0, length(off)) * twinkle * smoothstep(-0.02, 0.15, dir.y);
            c = c + vec3<f32>(0.9, 0.92, 1.0) * star * night * (0.6 + 1.4 * fract(h * 131.0));
        }
    }
    return c;
}

// One cloud layer `layer` metres above the camera's ground: colour and cover.
fn cloud_layer(dir: vec3<f32>, layer: f32, scale: f32, puffy: bool) -> vec4<f32> {
    let above = sky.cam.y;
    if (dir.y <= 0.01 || above > layer - 150.0) {
        return vec4<f32>(0.0);
    }
    let dist = (layer - above) / dir.y;
    let t = sky.sun_col.w;
    let wind = vec2<f32>(t * 0.00045, t * 0.00018);
    var uv = (sky.cam.xz * 1000.0 + dir.xz * dist) / scale + wind;
    let cover = sky.misc.y;
    var d = 0.0;
    var lit = 1.0;
    if (puffy) {
        // fbm sits around 0.48 +- 0.1; stretch it to use the whole range
        let n = clamp((fbm(uv, 5) - 0.48) * 1.9 + 0.5, 0.0, 1.0);
        let thresh = mix(0.68, 0.3, cover);
        d = smoothstep(thresh, thresh + 0.22, n);
        // brighter on the side toward the sun
        let ls = normalize(sky.sun.xz + vec2<f32>(1e-4, 0.0));
        let n2 = clamp((fbm(uv + ls * 0.07, 5) - 0.48) * 1.9 + 0.5, 0.0, 1.0);
        lit = clamp(0.62 + (n - n2) * 4.0, 0.2, 1.25);
    } else {
        // long thin streaks
        let r = vec2<f32>(uv.x * 0.8 + uv.y * 0.6, -uv.x * 0.6 + uv.y * 0.8);
        let n = clamp((fbm(vec2<f32>(r.x * 0.35, r.y * 3.0), 4) - 0.48) * 1.9 + 0.5, 0.0, 1.0);
        d = smoothstep(0.5, 0.85, n) * 0.4 * (0.4 + cover);
    }
    let fade = exp(-dist / 110000.0) * smoothstep(0.01, 0.09, dir.y) * (1.0 - smoothstep(layer - 1200.0, layer - 150.0, above));
    let day = sky.sun.w;
    let l = normalize(sky.sun.xyz);
    let lit_col = sky.sun_col.rgb * 1.05 + sky.zenith.rgb * 0.3;
    let shade_col = mix(sky.horizon.rgb, vec3<f32>(0.52, 0.55, 0.62), 0.5) * 0.85;
    var col = mix(shade_col, lit_col, clamp(lit, 0.0, 1.0)) * max(lit, 0.8);
    // silver edges when looking toward the sun
    col = col + sky.sun_col.rgb * pow(max(dot(dir, l), 0.0), 10.0) * (1.0 - d) * 0.9 * day;
    // moonlit grey at night
    col = col * mix(0.07, 1.0, day);
    return vec4<f32>(col, d * fade);
}

fn sky_colour(dir: vec3<f32>) -> vec3<f32> {
    var c = sky_gradient(dir);
    let high = cloud_layer(dir, 9000.0, 26000.0, false);
    c = mix(c, high.rgb, high.a);
    let low = cloud_layer(dir, 2200.0, 9000.0, true);
    c = mix(c, low.rgb, low.a);
    return c;
}

// ---- the Cloudwall ----
fn wall_colour(rel: vec3<f32>, wy: f32) -> vec3<f32> {
    let dist = length(rel);
    let wx = rel.x + sky.cam.x * 1000.0;
    let wz = rel.z + sky.cam.z * 1000.0;
    let ang = atan2(wz, wx);
    let u = ang * 10940.0;          // km along the wall
    let v = wy / 1000.0;            // km up
    let drift = sky.sun_col.w * 0.002;
    let p = vec2<f32>((u + drift) / 160.0, v / 120.0);
    let n = fbm(p, 5);
    let e = 0.03;
    let nx = fbm(p + vec2<f32>(e, 0.0), 5) - n;
    let ny = fbm(p + vec2<f32>(0.0, e), 5) - n;
    // facing the island, bent by the billows
    let inward = normalize(vec3<f32>(-wx, 0.0, -wz));
    let along = vec3<f32>(-inward.z, 0.0, inward.x);
    let tilt = 0.25 + 0.5 * smoothstep(400.0, 900.0, v);
    let nrm = normalize(inward + vec3<f32>(0.0, tilt, 0.0) - along * nx * 14.0 - vec3<f32>(0.0, 1.0, 0.0) * ny * 14.0);
    let l = normalize(sky.sun.xyz);
    let day = sky.sun.w;
    let wrap = clamp(dot(nrm, l) * 0.6 + 0.45, 0.0, 1.0);
    // darker bases, and great dark storm sections in places
    let base = smoothstep(0.0, 260.0, v);
    let storm = smoothstep(0.58, 0.78, fbm(vec2<f32>(u / 2600.0, 3.7), 3));
    let shade_col = mix(vec3<f32>(0.30, 0.33, 0.40), vec3<f32>(0.56, 0.59, 0.67), base) * mix(1.0, 0.5, storm);
    let lit_col = (sky.sun_col.rgb * 1.15 + sky.zenith.rgb * 0.25) * mix(1.0, 0.62, storm);
    var c = mix(shade_col * (0.25 + 0.75 * day), lit_col, wrap * day);
    c = c * (0.82 + 0.36 * n);
    // night: faintly moonlit
    c = c * mix(0.12, 1.0, day);
    // haze between the camera and the wall: thick near the sea, thin high up
    let H = sky.misc.x;
    let h0 = max(sky.cam.w, 0.0);
    let h1 = max(wy, 0.0);
    let dh = h1 - h0;
    var k = exp(-h0 / H);
    if (abs(dh) > 1.0) {
        k = (exp(-h0 / H) - exp(-h1 / H)) * H / dh;
    }
    let f = 1.0 - exp(-sky.haze.w * dist * k);
    return mix(c, sky.haze.rgb, f);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    let rel = in.world_position.xyz - view.world_position.xyz;
    var c = vec3<f32>(0.0);
    if (sky.horizon.w > 0.5) {
        c = wall_colour(rel, in.world_position.y);
    } else {
        c = sky_colour(normalize(rel));
    }
    out.color = vec4<f32>(c, 1.0);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
