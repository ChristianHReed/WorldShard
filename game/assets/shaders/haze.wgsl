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

// rgb: haze colour (linear)
@group(2) @binding(100) var<uniform> haze_color: vec4<f32>;
// x: haze density at sea level (per metre), y: scale height (metres)
@group(2) @binding(101) var<uniform> haze_params: vec4<f32>;

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

    let cam = view.world_position;
    let p = in.world_position.xyz;
    let dist = distance(cam, p);
    let scale_h = haze_params.y;
    let h0 = max(cam.y, 0.0);
    let h1 = max(p.y, 0.0);
    let dh = h1 - h0;
    var k = exp(-h0 / scale_h);
    if (abs(dh) > 1.0) {
        k = (exp(-h0 / scale_h) - exp(-h1 / scale_h)) * scale_h / dh;
    }
    let f = 1.0 - exp(-haze_params.x * dist * k);
    out.color = vec4<f32>(mix(out.color.rgb, haze_color.rgb, f), out.color.a);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
