//! WorldShard — the playable game (Bevy).
//!
//! The island is the size of Australia, so the world is streamed: terrain
//! chunks get finer near the camera, trees and grass appear around the
//! player, and everything is drawn relative to the camera so 32-bit floats
//! stay precise thousands of kilometres from the centre. The world itself,
//! its meshes, the day/night maths and the save format all come from the
//! engine-independent `worldshard` library.

// a Windows release build opens as a game window only, without a console
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet};

use bevy::asset::{AssetId, AssetMetaCheck, RenderAssetUsages};
use bevy::image::{ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::input::mouse::MouseMotion;
use bevy::pbr::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, DistanceFog, ExtendedMaterial, FogFalloff, MaterialExtension, NotShadowCaster};
use bevy::render::render_resource::{AsBindGroup, ShaderRef, ShaderType};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::primitives::Aabb;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use bevy::window::WindowResolution;

use worldshard::lod::{self, NodeKey};
use worldshard::bridge;
use worldshard::landmarks;
use worldshard::meshgen::{self, model, MeshData, Placement, PropKinds, Tint, DETAIL_CELL, GRASS_CELL, MODELS, PROP_CELL};
use worldshard::places::{places, MapImage};
use worldshard::save::{SaveData, STORAGE_KEY};
use worldshard::sky::{clock_rate, clock_text, sun_state};
use worldshard::worldgen::terrain::WALL_R;
use worldshard::worldgen::Terrain;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

// Pulled in only to switch on browser randomness for Bevy's dependencies.
#[cfg(target_arch = "wasm32")]
use {getrandom as _, getrandom02 as _};

const EYE: f64 = 1.7;
/// Trees and rocks are drawn this far around the player; grass closer.
/// How much detail to draw. Phones get less, so they don't run out of memory.
#[derive(Resource, Clone, Copy, Debug)]
struct Quality {
    /// Render resolution as a multiple of CSS pixels (None: the device's own).
    scale: Option<f32>,
    split_k: f64,
    prop_radius: f64,
    /// Real tree and rock models within this distance (stand-ins beyond).
    near_radius: f64,
    /// Flowers, ferns, clover, mushrooms and grass clumps.
    detail_radius: f64,
    grass_radius: f64,
    msaa: bool,
    /// Sun shadows reach this far from the camera (0: off), in this many steps.
    shadow_distance: f32,
    cascades: usize,
    shadow_map: usize,
    /// Land is drawn this far (metres).
    view: f64,
    name: &'static str,
}

/// The quality this device starts with; the settings sliders adjust from it.
#[derive(Resource, Clone, Copy)]
struct DeviceQuality(Quality);

impl Quality {
    fn detect() -> Quality {
        let full = Quality { scale: None, split_k: lod::SPLIT_K, prop_radius: 520.0, near_radius: 100.0, detail_radius: 44.0, grass_radius: 56.0, msaa: true, shadow_distance: 400.0, cascades: 1, shadow_map: 2048, view: lod::DEFAULT_VIEW, name: "full" };
        #[cfg(target_arch = "wasm32")]
        {
            let Some(w) = web_sys::window() else { return full };
            let dpr = w.device_pixel_ratio() as f32;
            let iw = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1280.0);
            let ih = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
            let lite = w
                .local_storage()
                .ok()
                .flatten()
                .and_then(|s| s.get_item("worldshard-quality").ok().flatten())
                .map_or(false, |v| v == "lite");
            let phone = iw.min(ih) < 700.0;
            if lite {
                return Quality { scale: Some(dpr.min(0.75)), split_k: 0.6, prop_radius: 200.0, near_radius: 50.0, detail_radius: 0.0, grass_radius: 0.0, msaa: false, shadow_distance: 0.0, cascades: 1, shadow_map: 512, view: 2_600_000.0, name: "lite" };
            }
            if phone {
                return Quality { scale: Some(dpr.min(1.0)), split_k: 0.8, prop_radius: 320.0, near_radius: 70.0, detail_radius: 30.0, grass_radius: 40.0, msaa: true, shadow_distance: 160.0, cascades: 1, shadow_map: 1024, view: lod::DEFAULT_VIEW, name: "phone" };
            }
            return Quality { scale: Some(dpr.min(1.5)), ..full };
        }
        #[allow(unreachable_code)]
        full
    }
}
/// Fly-through speeds (metres per second).
const FLY_SPEEDS: [f64; 4] = [30.0, 300.0, 3000.0, 30000.0];

// ---------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------

#[derive(Resource)]
struct GameWorld(Terrain);

#[derive(Resource)]
struct Kinds(PropKinds);

/// Haze that is thick near the ground and thins with height: distant
/// lowlands fade into the sky while far summits stay visible.
const HAZE_DENSITY: f32 = 2.8e-6;
const HAZE_SCALE_HEIGHT: f32 = 8000.0;

/// Terrain material: standard lighting plus the height haze (haze.wgsl).
/// All in one uniform block: WebGL2 allows only 14 per shader and Bevy's
/// standard lighting already uses most of them.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct Haze {
    #[uniform(100)]
    u: HazeUniform,
}

#[derive(ShaderType, Reflect, Debug, Clone)]
struct HazeUniform {
    color: Vec4,
    params: Vec4,
    /// toward the sun (xyz), shadow strength (w)
    sun_dir: Vec4,
    /// sunlight colour (rgb), how much the haze glows toward the sun (w)
    sun_col: Vec4,
    /// the region's ground mist: density, thickness, ground height
    mist: Vec4,
    mist_col: Vec4,
    /// the big peaks, two entries each (see haze.wgsl)
    peaks: [Vec4; 24],
    /// ground detail and surf: x, z the camera's position (wrapped every
    /// 4,096 m), y seconds (wrapped), w detail strength
    detail: Vec4,
}

/// The sky dome and the Cloudwall: their own shader (sky.wgsl).
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
struct SkyExt {
    #[uniform(100)]
    u: SkyUniform,
}

#[derive(ShaderType, Reflect, Debug, Clone, Default)]
struct SkyUniform {
    sun: Vec4,
    sun_col: Vec4,
    zenith: Vec4,
    /// w: 0 = sky, 1 = Cloudwall
    horizon: Vec4,
    cam: Vec4,
    haze: Vec4,
    misc: Vec4,
}

impl MaterialExtension for SkyExt {
    fn fragment_shader() -> ShaderRef {
        "shaders/sky.wgsl".into()
    }
}

type SkyMaterial = ExtendedMaterial<StandardMaterial, SkyExt>;

#[derive(Resource)]
struct SkyMats {
    dome: Handle<SkyMaterial>,
    wall: Handle<SkyMaterial>,
}

/// Each region's ground mist (first pass): density per metre, thickness in
/// metres, colour. Indexed like `REGIONS`: North, Northeast, Southeast,
/// South, Southwest, Northwest.
const MIST: [(f32, f32, [f32; 3]); 6] = [
    (0.0, 300.0, [0.80, 0.82, 0.85]),
    (3.0e-5, 700.0, [0.86, 0.74, 0.58]),
    (1.2e-5, 400.0, [0.78, 0.84, 0.86]),
    (8.0e-5, 400.0, [0.56, 0.66, 0.60]),
    (0.0, 300.0, [0.85, 0.85, 0.80]),
    (1.5e-4, 260.0, [0.72, 0.78, 0.76]),
];

impl MaterialExtension for Haze {
    fn fragment_shader() -> ShaderRef {
        "shaders/haze.wgsl".into()
    }
    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/haze.wgsl".into()
    }
}

type GroundMaterial = ExtendedMaterial<StandardMaterial, Haze>;

#[derive(Resource)]
struct Mats {
    ground: Handle<GroundMaterial>,
    water: Handle<StandardMaterial>,
    lava: Handle<StandardMaterial>,
    plants: Handle<StandardMaterial>,
}

#[derive(Resource)]
struct Player {
    x: f64,
    y: f64,
    z: f64,
    yaw: f64,
    pitch: f64,
    vy: f64,
    on_ground: bool,
    hours: f64,
    day: u32,
    visited: Vec<String>,
    play_sec: f64,
    fly: bool,
    fly_speed: usize,
}

#[derive(Resource, Default)]
struct Streamer {
    chunks: HashMap<NodeKey, Entity>,
    grass: HashMap<(i64, i64), Option<Entity>>,
    ready: bool,
}

#[derive(Resource)]
struct SaveState {
    dirty: bool,
    since_change: f32,
    since_save: f32,
    last: [f64; 3],
    last_visited: usize,
    status: String,
}

#[derive(Resource, Default)]
struct Banner {
    current: String,
    candidate: String,
    candidate_t: f32,
    show_t: f32,
    title: String,
    subtitle: String,
}

#[derive(Resource, Default)]
struct TouchState {
    move_id: Option<u64>,
    look_id: Option<u64>,
}

/// Where a piece of the world sits, in full-precision world metres.
#[derive(Component)]
struct Anchor {
    x: f64,
    y: f64,
    z: f64,
}

/// A stretch of the bridge city drawn as simple blocks, shown only beyond
/// the distance where the real houses (or their stand-ins) are drawn.
#[derive(Component)]
struct CityPiece {
    u0: f64,
    u1: f64,
}

#[derive(Component)]
struct Sun;
#[derive(Component)]
struct SkyDome;
#[derive(Component)]
struct ClockText;
#[derive(Component)]
struct StatusText;
#[derive(Component)]
struct BannerTitle;
#[derive(Component)]
struct BannerSub;
#[derive(Component)]
struct CompassText;
#[derive(Component)]
struct SpeedLabel;

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum FlyButton {
    Toggle,
    Up,
    Down,
    Speed,
}

// ---------------------------------------------------------------------
// Browser storage (desktop: a file)
// ---------------------------------------------------------------------

#[cfg(target_arch = "wasm32")]
fn storage_get() -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(STORAGE_KEY).ok()?
}
#[cfg(target_arch = "wasm32")]
fn storage_set(data: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .map(|s| s.set_item(STORAGE_KEY, data).is_ok())
        .unwrap_or(false)
}
#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}
#[cfg(target_arch = "wasm32")]
fn remove_loading_cover() {
    if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("loading")) {
        el.remove();
    }
}
/// Read a value the web page stored (settings, travel requests).
#[cfg(target_arch = "wasm32")]
fn page_get(key: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}
/// Hand the web page a value (as `window[name]`): text, or bytes.
#[cfg(target_arch = "wasm32")]
fn page_set_text(name: &str, value: &str) {
    if let Some(w) = web_sys::window() {
        let _ = js_sys::Reflect::set(&w, &JsValue::from_str(name), &JsValue::from_str(value));
    }
}
#[cfg(target_arch = "wasm32")]
fn page_set_bytes(name: &str, bytes: &[u8]) {
    if let Some(w) = web_sys::window() {
        let arr = js_sys::Uint8ClampedArray::from(bytes);
        let _ = js_sys::Reflect::set(&w, &JsValue::from_str(name), &arr);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn page_get(_key: &str) -> Option<String> {
    None
}
#[cfg(not(target_arch = "wasm32"))]
fn page_set_text(_name: &str, _value: &str) {}
#[cfg(not(target_arch = "wasm32"))]
fn page_set_bytes(_name: &str, _bytes: &[u8]) {}

#[cfg(not(target_arch = "wasm32"))]
fn storage_get() -> Option<String> {
    std::fs::read_to_string(format!("{STORAGE_KEY}.json")).ok()
}
#[cfg(not(target_arch = "wasm32"))]
fn storage_set(data: &str) -> bool {
    std::fs::write(format!("{STORAGE_KEY}.json"), data).is_ok()
}
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}
#[cfg(not(target_arch = "wasm32"))]
fn remove_loading_cover() {}

// ---------------------------------------------------------------------
// App
// ---------------------------------------------------------------------

fn main() {
    let quality = Quality::detect();
    let mut resolution = WindowResolution::default();
    if let Some(scale) = quality.scale {
        resolution = resolution.with_scale_factor_override(scale);
    }
    App::new()
        .insert_resource(quality)
        .insert_resource(DeviceQuality(quality))
        .insert_resource(ClearColor(Color::srgb(0.55, 0.72, 0.92)))
        .insert_resource(Banner::default())
        .insert_resource(TouchState::default())
        .insert_resource(Streamer::default())
        .insert_resource(Plants::default())
        .insert_resource(MipDone::default())
        .add_plugins(DefaultPlugins.set(AssetPlugin { meta_check: AssetMetaCheck::Never, ..default() }).set(WindowPlugin {
            primary_window: Some(Window {
                resolution,
                title: "WorldShard".into(),
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MaterialPlugin::<GroundMaterial>::default())
        .add_plugins(MaterialPlugin::<SkyMaterial>::default())
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (move_player, stream_world, stream_plants, place_relative, sky_and_light, update_hud, autosave, hide_loader).chain(),
        )
        .add_systems(Update, (apply_settings, web_bridge).before(move_player))
        .add_systems(Update, show_city_pieces.after(stream_plants))
        .add_systems(Update, load_regions.before(stream_plants))
        .add_systems(Update, add_mipmaps)
        .add_systems(Update, log_memory)
        .run();
}

fn to_mesh(m: MeshData) -> (Mesh, Aabb) {
    let n = m.positions.len();
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for p in &m.positions {
        let v = Vec3::from_array(*p);
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices));
    let mesh = if !m.uvs.is_empty() && m.uvs.len() == n { mesh.with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs) } else { mesh };
    (mesh, Aabb::from_min_max(lo, hi))
}

/// Spawns `meshes` as children of one anchored entity.
fn spawn_anchored(
    commands: &mut Commands,
    assets: &mut Assets<Mesh>,
    anchor: Anchor,
    parts: Vec<(MeshData, Handle<StandardMaterial>)>,
) -> Entity {
    let parent = commands.spawn((anchor, Transform::default(), Visibility::default())).id();
    for (data, mat) in parts {
        if data.is_empty() {
            continue;
        }
        let (mesh, aabb) = to_mesh(data);
        commands.spawn((Mesh3d(assets.add(mesh)), MeshMaterial3d(mat), aabb, NotShadowCaster, ChildOf(parent)));
    }
    parent
}

#[allow(clippy::too_many_arguments)]
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ground_materials: ResMut<Assets<GroundMaterial>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    asset_server: Res<AssetServer>,
    quality: Res<Quality>,
) {
    info!("WorldShard quality: {:?}", *quality);
    let world = Terrain::new();

    let mats = Mats {
        ground: ground_materials.add(GroundMaterial {
            base: StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.92,
                reflectance: 0.12,
                double_sided: true,
                cull_mode: None,
                // the haze is done in the terrain shader
                fog_enabled: false,
                ..default()
            },
            extension: Haze { u: HazeUniform {
                color: Vec4::new(0.5, 0.62, 0.8, 1.0),
                params: Vec4::new(HAZE_DENSITY, HAZE_SCALE_HEIGHT, 0.0, 0.0),
                sun_dir: Vec4::new(0.5, 0.7, 0.3, 0.0),
                sun_col: Vec4::new(1.0, 0.9, 0.75, 0.0),
                mist: Vec4::ZERO,
                mist_col: Vec4::ONE,
                peaks: [Vec4::ZERO; 24],
                detail: Vec4::new(0.0, 0.0, 0.0, 1.0),
            } },
        }),
        water: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.08,
            reflectance: 0.45,
            ..default()
        }),
        lava: materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() }),
        plants: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.85,
            reflectance: 0.08,
            double_sided: true,
            cull_mode: None,
            ..default()
        }),
    };

    // ---- the sky and the Cloudwall: one shader (sky.wgsl), two modes ----
    let sky_base = StandardMaterial { base_color: Color::WHITE, unlit: true, fog_enabled: false, double_sided: true, cull_mode: None, ..default() };
    let sky_mats = SkyMats {
        dome: sky_materials.add(SkyMaterial { base: sky_base.clone(), extension: SkyExt { u: SkyUniform::default() } }),
        wall: sky_materials.add(SkyMaterial {
            base: sky_base,
            extension: SkyExt { u: SkyUniform { horizon: Vec4::new(0.0, 0.0, 0.0, 1.0), ..default() } },
        }),
    };
    // the Cloudwall, at the edge of the Rim Sea
    let (wall_mesh, wall_aabb) = to_mesh(meshgen::cloudwall_mesh());
    let wall = commands.spawn((Anchor { x: 0.0, y: 0.0, z: 0.0 }, Transform::default(), Visibility::default())).id();
    commands.spawn((Mesh3d(meshes.add(wall_mesh)), MeshMaterial3d(sky_mats.wall.clone()), wall_aabb, NotShadowCaster, ChildOf(wall)));
    // the sky dome; it surrounds everything, so it must not cast a shadow
    let (sky_mesh, sky_aabb) = to_mesh(meshgen::sky_dome(15_000_000.0));
    commands.spawn((Mesh3d(meshes.add(sky_mesh)), MeshMaterial3d(sky_mats.dome.clone()), sky_aabb, Transform::default(), SkyDome, NotShadowCaster));
    commands.insert_resource(sky_mats);

    // ---- sunlight (WebGL2 allows one directional light: sun by day, moon by night) ----
    let shadows = quality.shadow_distance > 0.0;
    let mut sun = commands.spawn((DirectionalLight { illuminance: 6000.0, shadows_enabled: shadows, ..default() }, Transform::default(), Sun));
    if shadows {
        sun.insert(
            CascadeShadowConfigBuilder {
                num_cascades: quality.cascades,
                minimum_distance: 0.3,
                first_cascade_far_bound: quality.shadow_distance * 0.12,
                maximum_distance: quality.shadow_distance,
                overlap_proportion: 0.2,
            }
            .build(),
        );
    }
    commands.insert_resource(DirectionalLightShadowMap { size: quality.shadow_map });
    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 1200.0, ..default() });

    // ---- the bridge city across the canyon ----
    let bm = bridge::bridge_meshes(&world);
    let anchor = Anchor { x: bm.centre.0, y: 0.0, z: bm.centre.1 };
    let bridge = commands.spawn((anchor, Transform::default(), Visibility::default())).id();
    let (mesh, aabb) = to_mesh(bm.structure);
    commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.ground.clone()), aabb, ChildOf(bridge)));
    for (u0, u1, m) in bm.city {
        let (mesh, aabb) = to_mesh(m);
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.ground.clone()), aabb, NotShadowCaster, CityPiece { u0, u1 }, ChildOf(bridge)));
    }

    // ---- landmarks: the ruined outpost and the Terrace Lakes' waterfalls ----
    let o = world.outpost;
    let (mesh, aabb) = to_mesh(landmarks::outpost_mesh(&o));
    let ruin = commands.spawn((Anchor { x: o.x, y: 0.0, z: o.z }, Transform::default(), Visibility::default())).id();
    commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(mats.ground.clone()), aabb, ChildOf(ruin)));
    if let Some(tr) = &world.terraces {
        let (mesh, aabb) = to_mesh(landmarks::falls_mesh(&tr.falls, tr.start));
        let water = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.25,
            reflectance: 0.3,
            emissive: LinearRgba::rgb(0.04, 0.05, 0.06),
            ..default()
        });
        let falls = commands.spawn((Anchor { x: tr.start.0, y: 0.0, z: tr.start.1 }, Transform::default(), Visibility::default())).id();
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(water), aabb, NotShadowCaster, ChildOf(falls)));
    }

    // ---- the player: resume the save, or start in the Southwest grasslands ----
    let (sx, sz) = meshgen::spawn_point();
    let mut player = Player {
        x: sx,
        y: 0.0,
        z: sz,
        yaw: (-sx).atan2(sz),
        pitch: -0.05,
        vy: 0.0,
        on_ground: true,
        hours: 9.0,
        day: 1,
        visited: Vec::new(),
        play_sec: 0.0,
        fly: false,
        fly_speed: 1,
    };
    let mut status = "New game".to_string();
    let mut fly_y = None;
    let mut saved_y = f64::INFINITY;
    if let Some(save) = storage_get().and_then(|s| SaveData::from_json(&s)) {
        saved_y = save.y;
        player.x = save.x;
        player.z = save.z;
        player.yaw = save.yaw;
        player.pitch = save.pitch;
        player.hours = save.hours;
        player.day = save.day;
        player.visited = save.visited;
        player.play_sec = save.play_sec;
        player.fly = save.fly;
        if save.fly {
            fly_y = Some(save.y);
        }
        status = "Save loaded".to_string();
    }
    let ground = meshgen::ground_under(&world, player.x, player.z, fly_y.unwrap_or(saved_y));
    player.y = match fly_y {
        Some(y) => y.max(ground + 2.0),
        None => ground + EYE,
    };

    // ---- camera: hazy distance, a little extra colour ----
    commands.spawn((
        Camera3d::default(),
        if quality.msaa { Msaa::Sample4 } else { Msaa::Off },
        Projection::Perspective(PerspectiveProjection { fov: 55f32.to_radians(), near: 0.3, far: 30_000_000.0, ..default() }),
        Transform::from_xyz(0.0, player.y as f32, 0.0),
        DistanceFog {
            color: Color::srgb(0.72, 0.82, 0.93),
            directional_light_color: Color::srgba(1.0, 0.92, 0.75, 0.45),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::Exponential { density: HAZE_DENSITY },
        },
        ColorGrading::with_identical_sections(
            ColorGradingGlobal { post_saturation: 1.12, ..default() },
            ColorGradingSection { saturation: 1.08, contrast: 1.04, ..default() },
        ),
    ));

    // ---- HUD ----
    let ink = Color::srgb(0.97, 0.94, 0.86);
    commands.spawn((
        Text::new("Day 1  09:00"),
        TextFont { font_size: 18.0, ..default() },
        TextColor(ink),
        Node { position_type: PositionType::Absolute, top: Val::Px(10.0), left: Val::Px(12.0), ..default() },
        ClockText,
    ));
    commands.spawn((
        Text::new(status.clone()),
        TextFont { font_size: 13.0, ..default() },
        TextColor(ink.with_alpha(0.85)),
        Node { position_type: PositionType::Absolute, top: Val::Px(56.0), left: Val::Px(12.0), ..default() },
        StatusText,
    ));
    let compass_row = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            // on narrow phone screens the compass sits under the clock
            top: Val::Px(if quality.name == "full" { 10.0 } else { 34.0 }),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .id();
    commands.spawn((Text::new("N"), TextFont { font_size: 18.0, ..default() }, TextColor(ink), CompassText, ChildOf(compass_row)));
    let banner = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(20.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    commands.spawn((Text::new(""), TextFont { font_size: 40.0, ..default() }, TextColor(ink.with_alpha(0.0)), BannerTitle, ChildOf(banner)));
    commands.spawn((Text::new(""), TextFont { font_size: 18.0, ..default() }, TextColor(ink.with_alpha(0.0)), BannerSub, ChildOf(banner)));
    let help = if quality.name == "full" {
        "WASD / arrows move | hold mouse button and drag to look | Shift run | Space jump\n\
         Fly (testing): F or the Fly button | E / Q up and down | R changes speed\n\
         Phone: drag on the left to move (push far to run), drag on the right to look"
    } else {
        "Drag on the left to move (push far to run)\nDrag on the right to look"
    };
    commands.spawn((
        Text::new(help),
        TextFont { font_size: 12.0, ..default() },
        TextColor(ink.with_alpha(0.75)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(10.0), left: Val::Px(12.0), ..default() },
    ));

    // ---- fly-through buttons (testing the map) ----
    let panel = commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            right: Val::Px(10.0),
            top: Val::Percent(28.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(8.0),
            ..default()
        })
        .id();
    for (kind, label) in [(FlyButton::Toggle, "Fly"), (FlyButton::Up, "Up"), (FlyButton::Down, "Down"), (FlyButton::Speed, "x300")] {
        let b = commands
            .spawn((
                Button,
                kind,
                Node { width: Val::Px(66.0), height: Val::Px(44.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() },
                BackgroundColor(Color::srgba(0.05, 0.07, 0.12, 0.55)),
                BorderRadius::all(Val::Px(8.0)),
                ChildOf(panel),
            ))
            .id();
        let text = commands.spawn((Text::new(label), TextFont { font_size: 15.0, ..default() }, TextColor(ink), ChildOf(b))).id();
        if kind == FlyButton::Speed {
            commands.entity(text).insert(SpeedLabel);
        }
    }

    commands.insert_resource(SaveState {
        dirty: false,
        since_change: 0.0,
        since_save: 0.0,
        last: [player.x, player.y, player.z],
        last_visited: player.visited.len(),
        status,
    });
    commands.insert_resource(player);
    commands.insert_resource(Kinds(PropKinds::build()));
    commands.insert_resource(empty_model_assets());
    commands.insert_resource(RegionLoad::default());
    commands.insert_resource(mats);
    commands.insert_resource(GameWorld(world));
}

// ---------------------------------------------------------------------
// Movement: walking, swimming and the fly-through mode
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    touches: Res<Touches>,
    windows: Query<&Window>,
    buttons: Query<(&Interaction, &FlyButton)>,
    pressed_now: Query<(&Interaction, &FlyButton), Changed<Interaction>>,
    mut ts: ResMut<TouchState>,
    w: Res<GameWorld>,
    mut pl: ResMut<Player>,
    mut banner: ResMut<Banner>,
) {
    let dt = time.delta_secs().min(0.05) as f64;
    let t = &w.0;

    // fly-through controls
    let mut up_down = 0.0;
    for (i, b) in &pressed_now {
        if *i == Interaction::Pressed {
            match b {
                FlyButton::Toggle => toggle_fly(&mut pl, t),
                FlyButton::Speed => pl.fly_speed = (pl.fly_speed + 1) % FLY_SPEEDS.len(),
                _ => {}
            }
        }
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            match b {
                FlyButton::Up => up_down += 1.0,
                FlyButton::Down => up_down -= 1.0,
                _ => {}
            }
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        toggle_fly(&mut pl, t);
    }
    if keys.just_pressed(KeyCode::KeyR) {
        pl.fly_speed = (pl.fly_speed + 1) % FLY_SPEEDS.len();
    }
    if keys.pressed(KeyCode::KeyE) || (pl.fly && keys.pressed(KeyCode::Space)) {
        up_down += 1.0;
    }
    if keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::KeyC) {
        up_down -= 1.0;
    }
    let ui_busy = buttons.iter().any(|(i, _)| *i != Interaction::None);

    // look
    let mut look = Vec2::ZERO;
    for ev in motion.read() {
        if mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right) {
            look += ev.delta * 0.0042;
        }
    }
    let width = windows.iter().next().map(|w| w.width()).unwrap_or(800.0);
    for touch in touches.iter_just_pressed() {
        if ui_busy {
            continue;
        }
        if touch.position().x < width * 0.45 && ts.move_id.is_none() {
            ts.move_id = Some(touch.id());
        } else if ts.look_id.is_none() {
            ts.look_id = Some(touch.id());
        }
    }
    for touch in touches.iter_just_released().chain(touches.iter_just_canceled()) {
        if ts.move_id == Some(touch.id()) {
            ts.move_id = None;
        }
        if ts.look_id == Some(touch.id()) {
            ts.look_id = None;
        }
    }
    let mut joy = Vec2::ZERO;
    if let Some(touch) = ts.move_id.and_then(|id| touches.get_pressed(id)) {
        joy = ((touch.position() - touch.start_position()) / 60.0).clamp_length_max(1.0);
    }
    if let Some(touch) = ts.look_id.and_then(|id| touches.get_pressed(id)) {
        look += touch.delta() * 0.0065;
    }
    pl.yaw += look.x as f64;
    pl.pitch = (pl.pitch - look.y as f64).clamp(-1.45, 1.45);
    if keys.pressed(KeyCode::ArrowLeft) {
        pl.yaw -= 1.8 * dt;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        pl.yaw += 1.8 * dt;
    }

    // move
    let (mut fx, mut fz) = (0.0f64, 0.0f64);
    if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
        fz += 1.0;
    }
    if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
        fz -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        fx += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        fx -= 1.0;
    }
    fx += joy.x as f64;
    fz -= joy.y as f64;
    let mag = fx.hypot(fz);
    if mag > 1.0 {
        fx /= mag;
        fz /= mag;
    }
    let run = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) || joy.length() > 0.92;
    let (sy, cy) = (pl.yaw.sin(), pl.yaw.cos());
    let ground = meshgen::stand_height(t, pl.x, pl.z, pl.y);

    if pl.fly {
        let speed = FLY_SPEEDS[pl.fly_speed] * if run { 3.0 } else { 1.0 };
        let (sp, cp) = (pl.pitch.sin(), pl.pitch.cos());
        pl.x += (sy * cp * fz + cy * fx) * speed * dt;
        pl.z += (-cy * cp * fz + sy * fx) * speed * dt;
        pl.y += (sp * fz + up_down) * speed * dt;
        let g = meshgen::ground_under(t, pl.x, pl.z, pl.y);
        pl.y = pl.y.clamp(g + 2.0, 3_000_000.0);
        clamp_to_wall(&mut pl, &mut banner);
    } else {
        let s = t.sample(pl.x, pl.z);
        let water = s.water.unwrap_or(f64::MIN);
        let swimming = ground < water - 1.0;
        let speed = match (swimming, run) {
            (true, true) => 7.0,
            (true, false) => 3.5,
            (false, true) => 13.0,
            (false, false) => 5.5,
        };
        let mut vx = (sy * fz + cy * fx) * speed;
        let mut vz = (-cy * fz + sy * fx) * speed;
        if !swimming && (vx != 0.0 || vz != 0.0) {
            let ahead = t.height(pl.x + vx * 0.25, pl.z + vz * 0.25) - ground;
            let grade = ahead / (vx.hypot(vz) * 0.25);
            if grade > 0.35 {
                let k = (1.0 - (grade - 0.35) * 1.4).max(0.15);
                vx *= k;
                vz *= k;
            }
        }
        let (nx, nz) = meshgen::keep_on_deck(t, pl.x, pl.z, pl.x + vx * dt, pl.z + vz * dt, pl.y - EYE);
        pl.x = nx;
        pl.z = nz;
        clamp_to_wall(&mut pl, &mut banner);
        let g = meshgen::stand_height(t, pl.x, pl.z, pl.y);
        if swimming {
            pl.y += (water + 0.55 - pl.y) * (dt * 4.0).min(1.0);
            pl.vy = 0.0;
            pl.on_ground = true;
        } else {
            if keys.just_pressed(KeyCode::Space) && pl.on_ground {
                pl.vy = 6.0;
                pl.on_ground = false;
            }
            pl.vy -= 16.0 * dt;
            pl.y += pl.vy * dt;
            let eye = g + EYE;
            if pl.y <= eye {
                pl.y = eye;
                pl.vy = 0.0;
                pl.on_ground = true;
            } else if pl.y > eye + 0.25 {
                pl.on_ground = false;
            }
        }
    }

    // clock: a game day lasts a real hour (40 minutes of daylight, 20 of night)
    pl.play_sec += dt;
    pl.hours += dt * clock_rate(pl.hours);
    if pl.hours >= 24.0 {
        pl.hours -= 24.0;
        pl.day += 1;
    }

    // region banner
    let (name, desc) = t.place_name(pl.x, pl.z);
    if name != banner.current {
        if name == banner.candidate {
            banner.candidate_t += dt as f32;
            if banner.candidate_t > 0.8 {
                banner.current = name.to_string();
                banner.title = name.to_string();
                banner.subtitle = desc.to_string();
                banner.show_t = 3.5;
                if !pl.visited.iter().any(|v| v == name) {
                    pl.visited.push(name.to_string());
                }
            }
        } else {
            banner.candidate = name.to_string();
            banner.candidate_t = 0.0;
        }
    }
    banner.show_t -= dt as f32;
}

fn toggle_fly(pl: &mut Player, t: &Terrain) {
    pl.fly = !pl.fly;
    if !pl.fly {
        // land straight away rather than falling for minutes
        pl.y = meshgen::ground_under(t, pl.x, pl.z, pl.y) + EYE;
        pl.vy = 0.0;
    } else {
        pl.y += 3.0;
    }
}

fn clamp_to_wall(pl: &mut Player, banner: &mut Banner) {
    let r = pl.x.hypot(pl.z);
    if r > WALL_R {
        pl.x *= WALL_R / r;
        pl.z *= WALL_R / r;
        if banner.show_t <= 0.0 {
            banner.title = "The Cloudwall".into();
            banner.subtitle = "It can only be crossed late in the game".into();
            banner.show_t = 3.5;
        }
    }
}

// ---------------------------------------------------------------------
// Streaming: terrain chunks, trees and rocks, grass
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn stream_world(
    mut commands: Commands,
    mut assets: ResMut<Assets<Mesh>>,
    w: Res<GameWorld>,
    mats: Res<Mats>,
    pl: Res<Player>,
    q: Res<Quality>,
    mut st: ResMut<Streamer>,
) {
    let t = &w.0;
    let first = !st.ready;
    let ground = meshgen::ground_under(t, pl.x, pl.z, pl.y);
    let above = pl.y - ground;

    // terrain: build missing chunks nearest first, a few per frame. The
    // first view is built in batches behind the loading screen: built in a
    // single frame, its burst of memory and uploads to the graphics chip was
    // more than an iPhone 11 allows a page, and Safari closed it.
    let want = lod::select_kv([pl.x, pl.y, pl.z], above, q.split_k, q.view);
    let budget = if first { FIRST_BATCH } else { 3 };
    let mut built = 0;
    let mut missing = false;
    for k in &want {
        if st.chunks.contains_key(k) {
            continue;
        }
        if built >= budget {
            missing = true;
            break;
        }
        let c = meshgen::build_chunk(t, *k);
        let mut parts = Vec::new();
        if let Some(m) = c.water {
            parts.push((m, mats.water.clone()));
        }
        if let Some(m) = c.lava {
            parts.push((m, mats.lava.clone()));
        }
        let e = spawn_anchored(&mut commands, &mut assets, Anchor { x: c.origin[0], y: 0.0, z: c.origin[1] }, parts);
        if let Some(m) = c.terrain {
            let (mesh, aabb) = to_mesh(m);
            commands.spawn((Mesh3d(assets.add(mesh)), MeshMaterial3d(mats.ground.clone()), aabb, ChildOf(e)));
        }
        st.chunks.insert(*k, e);
        built += 1;
    }
    // drop chunks that are no longer wanted, once whatever replaces them is ready
    let wanted: HashSet<NodeKey> = want.iter().copied().collect();
    let stale: Vec<NodeKey> = st
        .chunks
        .keys()
        .copied()
        .filter(|k| !wanted.contains(k))
        .filter(|k| want.iter().filter(|w| w.contains(k) || k.contains(*w)).all(|w| st.chunks.contains_key(w)))
        .collect();
    for k in stale {
        if let Some(e) = st.chunks.remove(&k) {
            commands.entity(e).despawn();
        }
    }

    // grass tufts close to the player (not when flying)
    let grass_wanted = if above < 60.0 && q.grass_radius > 0.0 { cells_near(pl.x, pl.z, GRASS_CELL, q.grass_radius) } else { Vec::new() };
    let mut gb = if first { FIRST_BATCH } else { 4 };
    for &(i, j) in &grass_wanted {
        if gb == 0 {
            break;
        }
        if st.grass.contains_key(&(i, j)) {
            continue;
        }
        let e = meshgen::grass_cell(t, i, j).map(|m| {
            let (x, z) = ((i as f64 + 0.5) * GRASS_CELL, (j as f64 + 0.5) * GRASS_CELL);
            spawn_anchored(&mut commands, &mut assets, Anchor { x, y: 0.0, z }, vec![(m, mats.plants.clone())])
        });
        st.grass.insert((i, j), e);
        gb -= 1;
    }
    let keep_g: HashSet<(i64, i64)> = grass_wanted.iter().copied().collect();
    let drop_g: Vec<(i64, i64)> = st.grass.keys().copied().filter(|k| !keep_g.contains(k)).collect();
    for k in drop_g {
        if let Some(Some(e)) = st.grass.remove(&k) {
            commands.entity(e).despawn();
        }
    }
    // the first view is ready once every chunk it wants is built
    if !missing {
        st.ready = true;
    }
}

/// Terrain chunks (and grass cells) built per frame while the first view is
/// put together behind the loading screen.
const FIRST_BATCH: usize = 24;

/// Grid cells of side `cell` whose centres are within `radius` of (x, z), nearest first.
fn cells_near(x: f64, z: f64, cell: f64, radius: f64) -> Vec<(i64, i64)> {
    let (ci, cj) = ((x / cell).floor() as i64, (z / cell).floor() as i64);
    let n = (radius / cell).ceil() as i64 + 1;
    let mut v = Vec::new();
    for j in cj - n..=cj + n {
        for i in ci - n..=ci + n {
            let (cx, cz) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
            if (cx - x).hypot(cz - z) < radius {
                v.push((i, j));
            }
        }
    }
    v.sort_by_key(|&(i, j)| (i - ci).abs() + (j - cj).abs());
    v
}

// ---------------------------------------------------------------------
// Plants and rocks: real models near the player, stand-ins further out
// ---------------------------------------------------------------------

/// The MegaKit models (one mesh per part) and their textures.
#[derive(Resource)]
struct ModelAssets {
    /// Each model's parts, while it is loaded (see `load_regions`).
    parts: Vec<Option<Vec<Handle<Mesh>>>>,
    textures: HashMap<&'static str, Handle<Image>>,
    /// Textures with cut-out leaves (their mipmaps keep the leaves full).
    masked: HashSet<AssetId<Image>>,
    /// Models asked for on the spot, outside the loaded sets (kept until
    /// the sets change).
    extra: HashSet<u16>,
}

const TEXTURES: &[(&str, bool)] = &[
    ("Bark_NormalTree.png", true),
    ("Bark_DeadTree.png", false),
    ("Bark_TwistedTree.png", false),
    ("Leaves_NormalTree_C.png", true),
    ("Leaf_Pine_C.png", true),
    ("Leaves_TwistedTree_Green.png", true),
    ("Leaves.png", true),
    ("Flowers.png", true),
    ("Grass.png", false),
    ("Mushrooms.png", false),
    ("PathRocks_Diffuse.png", false),
    ("Rocks_Diffuse.png", false),
    ("Rocks_Desert_Diffuse.png", false),
    ("T_WoodTrim_BaseColor.png", false),
    ("T_Plaster_BaseColor.png", false),
    ("T_UnevenBrick_BaseColor.png", false),
    ("T_RoundTiles_BaseColor.png", false),
    ("T_RockTrim_BaseColor.png", false),
    ("T_Brick_BaseColor.png", false),
    ("T_MetalOrnaments_BaseColor.png", false),
];

fn empty_model_assets() -> ModelAssets {
    ModelAssets { parts: vec![None; MODELS.len()], textures: HashMap::new(), masked: HashSet::new(), extra: HashSet::new() }
}

/// Start loading a model and its textures, if they aren't already.
fn ensure_model(server: &AssetServer, ma: &mut ModelAssets, m: u16) {
    let info = &MODELS[m as usize];
    if ma.parts[m as usize].is_none() {
        ma.parts[m as usize] = Some((0..info.parts.len()).map(|p| server.load(format!("models/{}.gltf#Mesh0/Primitive{}", info.name, p))).collect());
    }
    for file in meshgen::model_textures(m) {
        if !ma.textures.contains_key(file) {
            // kept on the CPU side only until its mipmaps are made (see add_mipmaps);
            // then it moves to the graphics chip once, without a second copy
            let h: Handle<Image> = server.load_with_settings(format!("textures/{file}"), |s: &mut ImageLoaderSettings| {
                s.asset_usage = RenderAssetUsages::MAIN_WORLD;
            });
            if TEXTURES.iter().any(|&(f, mask)| f == file && mask) {
                ma.masked.insert(h.id());
            }
            ma.textures.insert(file, h);
        }
    }
}

/// A model whose parts and textures have all arrived.
fn model_ready(ma: &ModelAssets, meshes: &Assets<Mesh>, mips: &MipDone, m: u16) -> bool {
    let Some(parts) = &ma.parts[m as usize] else { return false };
    parts.iter().all(|h| meshes.contains(h.id())) && meshgen::model_textures(m).iter().all(|f| ma.textures.get(f).is_some_and(|h| mips.done.contains(&h.id())))
}

/// Which model sets are loaded (each region's plants and rocks, and the
/// Bridge City's buildings), and when to look again.
#[derive(Resource, Default)]
struct RegionLoad {
    loaded: [bool; 7],
    wait: f32,
}

/// Each region's models load as the player nears it (within a few km of
/// its transition zone) and are let go well after they leave, so only the
/// regions around the player take up memory.
fn load_regions(
    time: Res<Time>,
    server: Res<AssetServer>,
    w: Res<GameWorld>,
    pl: Res<Player>,
    mut ma: ResMut<ModelAssets>,
    mut rl: ResMut<RegionLoad>,
    mut pt: ResMut<Plants>,
) {
    rl.wait -= time.delta_secs();
    if rl.wait > 0.0 {
        return;
    }
    rl.wait = 0.5;
    let t = &w.0;
    let want = meshgen::wanted_sets(t, pl.x, pl.z, 6_000.0);
    let keep = meshgen::wanted_sets(t, pl.x, pl.z, 14_000.0);
    let mut changed = false;
    for k in 0..7 {
        if want[k] && !rl.loaded[k] {
            rl.loaded[k] = true;
            changed = true;
            for m in meshgen::set_models(k) {
                ensure_model(&server, &mut ma, m);
            }
        } else if rl.loaded[k] && !keep[k] {
            rl.loaded[k] = false;
            changed = true;
        }
    }
    if !changed {
        return;
    }
    // let go of what no loaded set uses (cells still showing a model keep
    // it until they go)
    ma.extra.clear();
    let needed: HashSet<u16> = (0..7).filter(|&k| rl.loaded[k]).flat_map(meshgen::set_models).collect();
    for m in 0..MODELS.len() {
        if !needed.contains(&(m as u16)) {
            ma.parts[m] = None;
        }
    }
    let tex: HashSet<&str> = needed.iter().flat_map(|&m| meshgen::model_textures(m)).collect();
    ma.textures.retain(|f, _| tex.contains(f));
    pt.materials.retain(|(part, tint), _| meshgen::part_texture(part, *tint).is_none_or(|f| tex.contains(f)));
    let names: Vec<&str> = (0..7).filter(|&k| rl.loaded[k]).map(|k| meshgen::LOAD_SETS[k]).collect();
    info!("WorldShard models for: {}", names.join(", "));
    page_set_text("wsLoaded", &names.join(","));
}

/// One material per (part, tint), made the first time it is needed.
fn plant_material(
    cache: &mut HashMap<(&'static str, Tint), Handle<StandardMaterial>>,
    materials: &mut Assets<StandardMaterial>,
    ma: &ModelAssets,
    part: &'static str,
    tint: Tint,
) -> Handle<StandardMaterial> {
    if let Some(h) = cache.get(&(part, tint)) {
        return h.clone();
    }
    if let Some(h) = village_material(materials, ma, part, tint) {
        cache.insert((part, tint), h.clone());
        return h;
    }
    let file = meshgen::part_texture(part, tint).unwrap_or("Rocks_Diffuse.png");
    let mask = TEXTURES.iter().any(|&(f, m)| f == file && m);
    let rough = if part.starts_with("Bark") {
        0.95
    } else if part.starts_with("Leaves") {
        0.8
    } else if part == "Flowers" || part == "Mushrooms" {
        0.7
    } else if part == "Grass" {
        0.85
    } else {
        0.9
    };
    let foliage = part.starts_with("Leaves") || part == "Grass";
    let c = match (foliage, tint) {
        (true, Tint::Dark) => [0.55, 0.68, 0.58],
        (true, Tint::Swamp) => [0.82, 0.86, 0.62],
        (true, Tint::Dry) => [1.0, 0.95, 0.66],
        _ => [1.0, 1.0, 1.0],
    };
    let h = materials.add(StandardMaterial {
        base_color: Color::srgb(c[0], c[1], c[2]),
        base_color_texture: ma.textures.get(file).cloned(),
        alpha_mode: if mask { AlphaMode::Mask(0.5) } else { AlphaMode::Opaque },
        double_sided: true,
        cull_mode: None,
        perceptual_roughness: rough,
        reflectance: 0.2,
        ..default()
    });
    cache.insert((part, tint), h.clone());
    h
}

/// The bridge city's walls, roofs and trim (the Medieval Village MegaKit).
/// The tint varies the plaster and roof colours from house to house.
fn village_material(materials: &mut Assets<StandardMaterial>, ma: &ModelAssets, part: &str, tint: Tint) -> Option<Handle<StandardMaterial>> {
    let file = match part {
        "MI_WoodTrim" => "T_WoodTrim_BaseColor.png",
        "MI_Plaster" => "T_Plaster_BaseColor.png",
        "MI_UnevenBrick" => "T_UnevenBrick_BaseColor.png",
        "MI_RoundTiles" => "T_RoundTiles_BaseColor.png",
        "MI_RockTrim" => "T_RockTrim_BaseColor.png",
        "MI_Brick" => "T_Brick_BaseColor.png",
        "MI_MetalOrnaments" => "T_MetalOrnaments_BaseColor.png",
        "MI_WindowGlass" => {
            return Some(materials.add(StandardMaterial {
                base_color: Color::srgb(0.16, 0.2, 0.26),
                perceptual_roughness: 0.15,
                reflectance: 0.5,
                ..default()
            }));
        }
        _ => return None,
    };
    let c = match (part, tint) {
        ("MI_Plaster", Tint::Dry) => [1.0, 0.92, 0.78],
        ("MI_Plaster", Tint::Desert) => [1.0, 0.86, 0.72],
        ("MI_Plaster", Tint::Dark) => [0.9, 0.92, 0.95],
        ("MI_RoundTiles", Tint::Dark) => [0.62, 0.68, 0.8],
        ("MI_RoundTiles", Tint::Desert) => [1.0, 0.8, 0.66],
        ("MI_RoundTiles", Tint::Dry) => [0.9, 0.78, 0.62],
        _ => [1.0, 1.0, 1.0],
    };
    Some(materials.add(StandardMaterial {
        base_color: Color::srgb(c[0], c[1], c[2]),
        base_color_texture: ma.textures.get(file).cloned(),
        perceptual_roughness: if part == "MI_MetalOrnaments" { 0.5 } else { 0.9 },
        metallic: if part == "MI_MetalOrnaments" { 0.6 } else { 0.0 },
        reflectance: 0.2,
        double_sided: true,
        cull_mode: None,
        ..default()
    }))
}

#[derive(Resource, Default)]
struct Plants {
    placements: HashMap<(i64, i64), Vec<Placement>>,
    near: HashMap<(i64, i64), Entity>,
    far: HashMap<(i64, i64), Option<Entity>>,
    detail: HashMap<(i64, i64), Option<Entity>>,
    materials: HashMap<(&'static str, Tint), Handle<StandardMaterial>>,
    ready: bool,
}

/// Spawns real models for `placements` under one anchored entity.
#[allow(clippy::too_many_arguments)]
fn spawn_models(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    cache: &mut HashMap<(&'static str, Tint), Handle<StandardMaterial>>,
    ma: &ModelAssets,
    plain: &Handle<StandardMaterial>,
    kinds: &PropKinds,
    anchor: Anchor,
    placements: &[Placement],
    cast_shadows: bool,
) -> Entity {
    let parent = commands.spawn((anchor, Transform::default(), Visibility::default())).id();
    for p in placements {
        if p.model == model::MINE {
            continue;
        }
        let info = &MODELS[p.model as usize];
        let Some(handles) = &ma.parts[p.model as usize] else { continue };
        let tf = Transform { translation: Vec3::from_array(p.pos), rotation: Quat::from_rotation_y(p.rot), scale: Vec3::splat(p.scale) };
        for (k, &part) in info.parts.iter().enumerate() {
            let mat = plant_material(cache, materials, ma, part, p.tint);
            let mut e = commands.spawn((Mesh3d(handles[k].clone()), MeshMaterial3d(mat), tf, ChildOf(parent)));
            if !cast_shadows {
                e.insert(NotShadowCaster);
            }
        }
    }
    // mine entrances have no model yet: use the stand-in shapes
    if let Some(m) = meshgen::placeholder_mesh(kinds, placements, false) {
        let (mesh, aabb) = to_mesh(m);
        commands.spawn((Mesh3d(meshes.add(mesh)), MeshMaterial3d(plain.clone()), aabb, ChildOf(parent)));
    }
    parent
}

#[allow(clippy::too_many_arguments)]
fn stream_plants(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    w: Res<GameWorld>,
    kinds: Res<Kinds>,
    mats: Res<Mats>,
    mut ma: ResMut<ModelAssets>,
    pl: Res<Player>,
    q: Res<Quality>,
    mut pt: ResMut<Plants>,
    server: Res<AssetServer>,
    mips: Res<MipDone>,
) {
    let t = &w.0;
    let first = !pt.ready;
    pt.ready = true;
    let ground = meshgen::ground_under(t, pl.x, pl.z, pl.y);
    let above = pl.y - ground;
    let (near_want, mut far_want, detail_want) = if above < 400.0 {
        let all = cells_near(pl.x, pl.z, PROP_CELL, q.prop_radius);
        let near: Vec<(i64, i64)> = cells_near(pl.x, pl.z, PROP_CELL, q.near_radius);
        let near_set: HashSet<(i64, i64)> = near.iter().copied().collect();
        let far: Vec<(i64, i64)> = all.into_iter().filter(|c| !near_set.contains(c)).collect();
        let detail = if above < 60.0 && q.detail_radius > 0.0 { cells_near(pl.x, pl.z, DETAIL_CELL, q.detail_radius) } else { Vec::new() };
        (near, far, detail)
    } else {
        (Vec::new(), Vec::new(), Vec::new())
    };
    let pt = &mut *pt;

    // near cells: real models, once they have loaded (till then, the
    // stand-in shapes)
    let mut budget = if first { usize::MAX } else { 2 };
    for &c in &near_want {
        if budget == 0 {
            break;
        }
        if pt.near.contains_key(&c) {
            continue;
        }
        let pl_list = pt.placements.entry(c).or_insert_with(|| meshgen::prop_placements(t, c.0, c.1));
        let mut seen = HashSet::new();
        let mut waiting = false;
        for p in pl_list.iter() {
            if p.model != model::MINE && seen.insert(p.model) && !model_ready(&ma, &meshes, &mips, p.model) {
                waiting = true;
                if ma.parts[p.model as usize].is_none() {
                    ensure_model(&server, &mut ma, p.model);
                    ma.extra.insert(p.model);
                }
            }
        }
        if waiting {
            far_want.push(c);
            continue;
        }
        let (x, z) = ((c.0 as f64 + 0.5) * PROP_CELL, (c.1 as f64 + 0.5) * PROP_CELL);
        let e = spawn_models(&mut commands, &mut meshes, &mut materials, &mut pt.materials, &ma, &mats.plants, &kinds.0, Anchor { x, y: 0.0, z }, pl_list, true);
        pt.near.insert(c, e);
        budget -= 1;
    }
    // far cells: one merged mesh of stand-in shapes each
    let mut budget = if first { 16 } else { 4 };
    for &c in &far_want {
        if budget == 0 {
            break;
        }
        if pt.far.contains_key(&c) {
            continue;
        }
        let pl_list = pt.placements.entry(c).or_insert_with(|| meshgen::prop_placements(t, c.0, c.1));
        let e = meshgen::placeholder_mesh(&kinds.0, pl_list, true).map(|m| {
            let (x, z) = ((c.0 as f64 + 0.5) * PROP_CELL, (c.1 as f64 + 0.5) * PROP_CELL);
            spawn_anchored(&mut commands, &mut meshes, Anchor { x, y: 0.0, z }, vec![(m, mats.plants.clone())])
        });
        pt.far.insert(c, e);
        budget -= 1;
    }
    // swap levels of detail only once the replacement is there
    let near_set: HashSet<(i64, i64)> = near_want.iter().copied().collect();
    let far_set: HashSet<(i64, i64)> = far_want.iter().copied().collect();
    let drop_near: Vec<(i64, i64)> = pt.near.keys().copied().filter(|c| !near_set.contains(c) && (!far_set.contains(c) || pt.far.contains_key(c))).collect();
    for c in drop_near {
        if let Some(e) = pt.near.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    let drop_far: Vec<(i64, i64)> = pt.far.keys().copied().filter(|c| !far_set.contains(c) && (!near_set.contains(c) || pt.near.contains_key(c))).collect();
    for c in drop_far {
        if let Some(Some(e)) = pt.far.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    let keep: Vec<(i64, i64)> = pt.placements.keys().copied().filter(|c| !near_set.contains(c) && !far_set.contains(c)).collect();
    for c in keep {
        pt.placements.remove(&c);
    }

    // small plants right around the player
    let mut budget = if first { usize::MAX } else { 3 };
    for &c in &detail_want {
        if budget == 0 {
            break;
        }
        if pt.detail.contains_key(&c) {
            continue;
        }
        let list = meshgen::detail_placements(t, c.0, c.1);
        let mut seen = HashSet::new();
        let mut waiting = false;
        for p in &list {
            if seen.insert(p.model) && !model_ready(&ma, &meshes, &mips, p.model) {
                waiting = true;
                if ma.parts[p.model as usize].is_none() {
                    ensure_model(&server, &mut ma, p.model);
                    ma.extra.insert(p.model);
                }
            }
        }
        if waiting {
            continue;
        }
        let e = (!list.is_empty()).then(|| {
            let (x, z) = ((c.0 as f64 + 0.5) * DETAIL_CELL, (c.1 as f64 + 0.5) * DETAIL_CELL);
            spawn_models(&mut commands, &mut meshes, &mut materials, &mut pt.materials, &ma, &mats.plants, &kinds.0, Anchor { x, y: 0.0, z }, &list, false)
        });
        pt.detail.insert(c, e);
        budget -= 1;
    }
    let detail_set: HashSet<(i64, i64)> = detail_want.iter().copied().collect();
    let drop_detail: Vec<(i64, i64)> = pt.detail.keys().copied().filter(|c| !detail_set.contains(c)).collect();
    for c in drop_detail {
        if let Some(Some(e)) = pt.detail.remove(&c) {
            commands.entity(e).despawn();
        }
    }
}

/// The city's distant blocks give way to the real houses near the player.
fn show_city_pieces(pl: Res<Player>, w: Res<GameWorld>, q: Res<Quality>, mut pieces: Query<(&CityPiece, &mut Visibility)>) {
    let t = &w.0;
    let b = &t.bridge;
    let above = pl.y - meshgen::ground_under(t, pl.x, pl.z, pl.y);
    // props (and the houses with them) are only drawn near the ground
    let props_on = above < 400.0;
    let (u, v) = b.to_deck(pl.x, pl.z);
    let hide_r = (q.prop_radius - 100.0).max(q.near_radius + 60.0);
    for (c, mut vis) in &mut pieces {
        let du = (c.u0 - u).max(u - c.u1).max(0.0);
        let d = du.hypot((v.abs() - b.half_w).max(0.0));
        let want = if props_on && d < hide_r { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
    }
}

/// Builds mipmaps for the plant textures as they arrive, so leaves and bark
/// stay smooth in the distance instead of shimmering. Cut-out leaf textures
/// keep the same amount of leaf in every mipmap.
///
/// Model textures load into the main world only. A couple a frame get their
/// mipmaps here and are then handed to the render world, which takes them
/// over: one upload each, and no second copy left behind. (All at once, a
/// region's textures and their copies were more memory than an iPhone 11
/// allows a page while the game starts.)
#[derive(Resource, Default)]
struct MipDone {
    done: HashSet<AssetId<Image>>,
    queue: std::collections::VecDeque<AssetId<Image>>,
}

const MIPS_PER_FRAME: usize = 2;

fn add_mipmaps(mut events: EventReader<AssetEvent<Image>>, mut images: ResMut<Assets<Image>>, mut mips: ResMut<MipDone>, ma: Option<Res<ModelAssets>>) {
    let Some(ma) = ma else { return };
    for ev in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = ev else { continue };
        if mips.done.contains(id) || mips.queue.contains(id) || !ma.textures.values().any(|h| h.id() == *id) {
            continue;
        }
        mips.queue.push_back(*id);
    }
    for _ in 0..MIPS_PER_FRAME {
        let Some(id) = mips.queue.pop_front() else { break };
        if let Some(img) = images.get_mut(id) {
            make_mips(img, ma.masked.contains(&id));
            img.asset_usage = RenderAssetUsages::RENDER_WORLD;
        }
        mips.done.insert(id);
    }
}

fn make_mips(img: &mut Image, masked: bool) {
    let fmt = img.texture_descriptor.format;
    if (fmt != TextureFormat::Rgba8UnormSrgb && fmt != TextureFormat::Rgba8Unorm) || img.texture_descriptor.mip_level_count > 1 {
        return;
    }
    let (w, h) = (img.texture_descriptor.size.width as usize, img.texture_descriptor.size.height as usize);
    let Some(base) = img.data.clone() else { return };
    if base.len() != w * h * 4 {
        return;
    }
    let cover = |d: &[u8], k: f32| d.chunks(4).filter(|p| p[3] as f32 * k >= 127.5).count() as f32 / (d.len() / 4) as f32;
    let target = if masked { cover(&base, 1.0) } else { 0.0 };
    let mut levels = vec![base];
    let (mut cw, mut ch) = (w, h);
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let prev = levels.last().unwrap();
        let mut next = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(cw - 1);
                        let sy = (y * 2 + dy).min(ch - 1);
                        sum += prev[(sy * cw + sx) * 4 + c] as u32;
                    }
                    next[(y * nw + x) * 4 + c] = (sum / 4) as u8;
                }
            }
        }
        if masked && target > 0.0 {
            let (mut lo, mut hi) = (0.5f32, 4.0f32);
            for _ in 0..10 {
                let mid = (lo + hi) / 2.0;
                if cover(&next, mid) < target {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let k = (lo + hi) / 2.0;
            for px in next.chunks_mut(4) {
                px[3] = (px[3] as f32 * k).min(255.0) as u8;
            }
        }
        levels.push(next);
        cw = nw;
        ch = nh;
    }
    img.texture_descriptor.mip_level_count = levels.len() as u32;
    img.data = Some(levels.concat());
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    });
}

/// Draws everything relative to the camera, which always sits at x = z = 0.
#[allow(clippy::type_complexity)]
fn place_relative(
    pl: Res<Player>,
    mut anchored: Query<(&Anchor, &mut Transform), (Without<Camera3d>, Without<SkyDome>)>,
    mut cams: Query<&mut Transform, (With<Camera3d>, Without<Anchor>, Without<SkyDome>)>,
    mut sky: Query<&mut Transform, (With<SkyDome>, Without<Anchor>, Without<Camera3d>)>,
    mut projections: Query<&mut Projection, With<Camera3d>>,
    w: Res<GameWorld>,
) {
    // the near clip plane moves out when flying high, which keeps the
    // depth buffer precise for land and sea thousands of km away
    let above = (pl.y - meshgen::ground_under(&w.0, pl.x, pl.z, pl.y)).max(0.0);
    let near = (above * 0.02).clamp(0.3, 50_000.0) as f32;
    for mut proj in &mut projections {
        if let Projection::Perspective(p) = &mut *proj {
            if (p.near - near).abs() > p.near * 0.05 {
                p.near = near;
            }
        }
    }
    for (a, mut tf) in &mut anchored {
        tf.translation = Vec3::new((a.x - pl.x) as f32, a.y as f32, (a.z - pl.z) as f32);
    }
    for mut tf in &mut cams {
        tf.translation = Vec3::new(0.0, pl.y as f32, 0.0);
        tf.rotation = Quat::from_rotation_y(-pl.yaw as f32) * Quat::from_rotation_x(pl.pitch as f32);
    }
    for mut tf in &mut sky {
        tf.translation = Vec3::new(0.0, pl.y as f32, 0.0);
    }
}

// ---------------------------------------------------------------------
// Day and night, sky and haze
// ---------------------------------------------------------------------

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0))
}

#[allow(clippy::too_many_arguments)]
fn sky_and_light(
    pl: Res<Player>,
    w: Res<GameWorld>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<AmbientLight>,
    mut sky_light: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut fogs: Query<&mut DistanceFog>,
    mut ground_materials: ResMut<Assets<GroundMaterial>>,
    mut sky_materials: ResMut<Assets<SkyMaterial>>,
    mats: Res<Mats>,
    sky_mats: Res<SkyMats>,
    q: Res<Quality>,
) {
    let s = sun_state(pl.hours);
    clear.0 = rgb(s.sky_col);
    ambient.brightness = 420.0 + 820.0 * s.day;
    let amax = s.sky_amb[2].max(0.01);
    ambient.color = rgb([s.sky_amb[0] / amax, s.sky_amb[1] / amax, 1.0]);
    // the light comes from the sun by day and the moon by night; the giants'
    // shadows follow it
    let sun = 6000.0 * s.sun_col[0].max(s.sun_col[1]).max(s.sun_col[2]);
    let moon = 9000.0 * s.moon_col[2];
    let (col, dir, lux, strength) = if sun >= moon { (s.sun_col, s.sun_dir, sun, s.day) } else { ([0.6, 0.7, 1.0], s.moon_dir, moon, 0.5 * (1.0 - s.day)) };
    let light_dir = [dir[0], dir[1].max(0.02), dir[2]];
    // standing in a giant's shadow dims the light on everything around you
    let shade_here = (w.0.giant_shadow([pl.x, pl.y + 2.0, pl.z], [light_dir[0] as f64, light_dir[1] as f64, light_dir[2] as f64]) as f32) * strength;
    if let Ok((mut light, mut tf)) = sky_light.single_mut() {
        let m = col[0].max(col[1]).max(col[2]).max(0.001);
        light.illuminance = lux * (1.0 - 0.85 * shade_here);
        light.color = rgb([col[0] / m, col[1] / m, col[2] / m]);
        *tf = Transform::IDENTITY.looking_to(-Vec3::from_array(dir), Vec3::Y);
    }
    // haze: the terrain shader works it out along each line of sight; water,
    // plants and clouds use the haze density at the camera's height
    let day = s.day;
    let haze = rgb(s.fog_col);
    let lin = |c: Color| {
        let l = c.to_linear();
        Vec4::new(l.red, l.green, l.blue, 1.0)
    };
    let haze_lin = lin(haze);
    let sun_lin = lin(rgb(s.sun_col));
    let t = &w.0;
    let ground = meshgen::ground_under(t, pl.x, pl.z, pl.y);
    let above = (pl.y - ground).max(0.0);

    // the region's ground mist, thickest in the early morning
    let wr = t.region_mix(pl.x, pl.z);
    let (mut md, mut mh, mut mc) = (0.0f32, 0.0f32, [0.0f32; 3]);
    for (k, &wk) in wr.iter().enumerate() {
        let wk = wk as f32;
        md += wk * MIST[k].0;
        mh += wk * MIST[k].1;
        for c in 0..3 {
            mc[c] += wk * MIST[k].2[c];
        }
    }
    let morning = 1.0 + 1.5 * (-((pl.hours as f32 - 7.0) / 2.0).powi(2)).exp();
    md *= morning;
    let mist_lin = lin(rgb(mc)) * (0.15 + 0.85 * day);
    let mist_here = md * (-(above as f32) / mh.max(1.0)).exp();

    let cam_density = HAZE_DENSITY * (-(pl.y.max(0.0) as f32) / HAZE_SCALE_HEIGHT).exp();
    let mist_share = mist_here / (cam_density + mist_here).max(1e-9);
    for mut fog in &mut fogs {
        let fc = haze_lin.truncate() * (1.0 - mist_share) + mist_lin.truncate() * mist_share;
        fog.color = Color::linear_rgb(fc.x, fc.y, fc.z);
        fog.falloff = FogFalloff::Exponential { density: cam_density + mist_here };
        fog.directional_light_color = Color::srgba(1.0, 0.9, 0.7, 0.45 * day);
    }
    if let Some(g) = ground_materials.get_mut(&mats.ground) {
        let u = &mut g.extension.u;
        u.color = haze_lin;
        u.sun_dir = Vec4::new(light_dir[0], light_dir[1], light_dir[2], strength);
        u.params.z = shade_here;
        u.sun_col = Vec4::new(sun_lin.x, sun_lin.y, sun_lin.z, 0.55 * day);
        u.mist = Vec4::new(md, mh, ground as f32, 0.0);
        u.mist_col = mist_lin;
        let mut peaks = [Vec4::ZERO; 24];
        for (k, sp) in t.shadow_peaks().iter().take(12).enumerate() {
            peaks[2 * k] = Vec4::new((sp.x - pl.x) as f32, (sp.z - pl.z) as f32, sp.spire_m as f32, sp.spire_r as f32);
            peaks[2 * k + 1] = Vec4::new(sp.body_m as f32, sp.body_r as f32, sp.body_p as f32, sp.reach() as f32);
        }
        u.peaks = peaks;
        // (no close-up ground detail on lite quality: it costs too much there)
        let detail = if q.name == "lite" { 0.0 } else { 1.0 };
        u.detail = Vec4::new(pl.x.rem_euclid(4096.0) as f32, (pl.play_sec % 4096.0) as f32, pl.z.rem_euclid(4096.0) as f32, detail);
    }

    // sky and Cloudwall: gradient, sun, clouds that drift and vary by day
    // and by region (dry Northeast skies, cloudier wet Northwest)
    let sky_lin = lin(rgb(s.sky_col));
    let zenith = sky_lin * 0.72;
    let horizon = (sky_lin * 1.12) * 0.65 + haze_lin * 0.35;
    let cover_region = (wr[5] * 0.18 + wr[3] * 0.1 + wr[2] * 0.05 - wr[1] * 0.25 - wr[4] * 0.05) as f32;
    let cover = (0.45 + 0.22 * ((pl.day as f32) * 1.7 + pl.hours as f32 / 9.0).sin() + cover_region).clamp(0.05, 0.95);
    // clouds drift and stars twinkle in real seconds (the game clock runs
    // 24 to 36 times faster than real time)
    let secs = pl.play_sec.rem_euclid(86400.0) as f32;
    for (handle, mode) in [(&sky_mats.dome, 0.0f32), (&sky_mats.wall, 1.0f32)] {
        if let Some(m) = sky_materials.get_mut(handle) {
            m.extension.u = SkyUniform {
                sun: Vec4::new(s.sun_dir[0], s.sun_dir[1], s.sun_dir[2], day),
                sun_col: Vec4::new(sun_lin.x, sun_lin.y, sun_lin.z, secs),
                zenith,
                horizon: Vec4::new(horizon.x, horizon.y, horizon.z, mode),
                cam: Vec4::new((pl.x / 1000.0) as f32, above as f32, (pl.z / 1000.0) as f32, pl.y as f32),
                haze: Vec4::new(haze_lin.x, haze_lin.y, haze_lin.z, HAZE_DENSITY),
                misc: Vec4::new(HAZE_SCALE_HEIGHT, cover, 0.0, 0.0),
            };
        }
    }
}

// ---------------------------------------------------------------------
// HUD and saving
// ---------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn update_hud(
    pl: Res<Player>,
    w: Res<GameWorld>,
    banner: Res<Banner>,
    save: Res<SaveState>,
    mut buttons: Query<(&FlyButton, &mut BackgroundColor)>,
    mut texts: ParamSet<(
        Query<&mut Text, With<ClockText>>,
        Query<&mut Text, With<StatusText>>,
        Query<&mut Text, With<CompassText>>,
        Query<(&mut Text, &mut TextColor), With<BannerTitle>>,
        Query<(&mut Text, &mut TextColor), With<BannerSub>>,
        Query<&mut Text, With<SpeedLabel>>,
    )>,
) {
    let ground = meshgen::ground_under(&w.0, pl.x, pl.z, pl.y);
    let alt = if pl.fly { pl.y } else { pl.y - EYE };
    let alt_text = if alt.abs() >= 10_000.0 { format!("{:.0} km", alt / 1000.0) } else { format!("{:.0} m", alt) };
    let above = pl.y - ground;
    if let Ok(mut t) = texts.p0().single_mut() {
        let mut s = format!("Day {}  {}  |  {}", pl.day, clock_text(pl.hours), alt_text);
        if pl.fly {
            let a = above.max(0.0);
            let a = if a >= 10_000.0 { format!("{:.0} km", a / 1000.0) } else { format!("{:.0} m", a) };
            s.push_str(&format!("\nFLY: {} above ground", a));
        }
        if t.0 != s {
            t.0 = s;
        }
    }
    if let Ok(mut t) = texts.p1().single_mut() {
        if t.0 != save.status {
            t.0 = save.status.clone();
        }
    }
    if let Ok(mut t) = texts.p2().single_mut() {
        let deg = (pl.yaw.to_degrees()).rem_euclid(360.0);
        let names = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
        let dir = names[(((deg + 22.5) / 45.0) as usize) % 8];
        let s = format!("{dir}  {:03.0}", deg);
        if t.0 != s {
            t.0 = s;
        }
    }
    let alpha = banner.show_t.clamp(0.0, 1.0);
    let ink = Color::srgba(0.97, 0.94, 0.86, alpha);
    if let Ok((mut t, mut c)) = texts.p3().single_mut() {
        let up = banner.title.to_uppercase();
        if t.0 != up {
            t.0 = up;
        }
        c.0 = ink;
    }
    if let Ok((mut t, mut c)) = texts.p4().single_mut() {
        if t.0 != banner.subtitle {
            t.0 = banner.subtitle.clone();
        }
        c.0 = ink;
    }
    if let Ok(mut t) = texts.p5().single_mut() {
        let s = ["x30", "x300", "x3k", "x30k"][pl.fly_speed.min(3)].to_string();
        if t.0 != s {
            t.0 = s;
        }
    }
    for (b, mut bg) in &mut buttons {
        let on = pl.fly && *b == FlyButton::Toggle;
        let want = if on { Color::srgba(0.85, 0.62, 0.2, 0.75) } else { Color::srgba(0.05, 0.07, 0.12, 0.55) };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

/// Saves to the browser a few seconds after the player moves, straight away
/// on reaching a new place, and every 30 s while playing. The page's
/// "Save file" button turns this into a file the player can keep in Drive.
fn autosave(time: Res<Time>, pl: Res<Player>, mut save: ResMut<SaveState>) {
    let dt = time.delta_secs();
    save.since_save += dt;
    let moved = ((pl.x - save.last[0]).powi(2) + (pl.y - save.last[1]).powi(2) + (pl.z - save.last[2]).powi(2)).sqrt();
    if moved > 2.0 {
        save.dirty = true;
    }
    if save.dirty {
        save.since_change += dt;
    }
    let new_place = pl.visited.len() != save.last_visited;
    if !(new_place || (save.dirty && save.since_change >= 4.0) || save.since_save >= 30.0) {
        return;
    }
    let data = SaveData {
        x: pl.x,
        z: pl.z,
        yaw: pl.yaw,
        pitch: pl.pitch,
        hours: pl.hours,
        day: pl.day,
        visited: pl.visited.clone(),
        play_sec: pl.play_sec,
        saved_at: now_ms(),
        fly: pl.fly,
        y: pl.y,
    };
    save.status = if storage_set(&data.to_json()) {
        format!("Saved - Day {} {}", pl.day, clock_text(pl.hours))
    } else {
        "Saving is not available here".into()
    };
    save.dirty = false;
    save.since_change = 0.0;
    save.since_save = 0.0;
    save.last = [pl.x, pl.y, pl.z];
    save.last_visited = pl.visited.len();
}

/// Writes the game's memory use to the browser console now and then
/// (handy when a phone runs out of memory).
fn log_memory(time: Res<Time>, mut next: Local<f32>) {
    if time.elapsed_secs() < *next {
        return;
    }
    // every 2 seconds while the world is first built (when a phone is most
    // likely to run short), then every 20
    *next = time.elapsed_secs() + if time.elapsed_secs() < 90.0 { 2.0 } else { 20.0 };
    #[cfg(target_arch = "wasm32")]
    {
        let mb = core::arch::wasm32::memory_size::<0>() as f64 * 65536.0 / 1_048_576.0;
        info!("WorldShard memory: {:.0} MB", mb);
    }
}

fn hide_loader(st: Res<Streamer>, mut frames: Local<u32>) {
    if st.ready && *frames < 3 {
        *frames += 1;
        if *frames == 3 {
            remove_loading_cover();
        }
    }
}

// ---------------------------------------------------------------------
// Settings sliders, the map and fast travel (with the web page)
// ---------------------------------------------------------------------

/// The settings sliders on the page store "view=3600;detail=1;trees=520;
/// plants=56;shadows=400" (km, multiple, metres, metres, metres).
const SETTINGS_KEY: &str = "worldshard-settings";
/// A fast-travel request from the map: "id;x;z" (world metres).
const TRAVEL_KEY: &str = "worldshard-travel";

#[derive(Default)]
struct SettingsPoll {
    wait: f32,
    started: bool,
    last: String,
}

fn quality_json(q: &Quality, dev: &Quality) -> String {
    format!(
        "{{\"view\":{:.0},\"detail\":{:.2},\"trees\":{:.0},\"plants\":{:.0},\"shadows\":{:.0},\"name\":\"{}\"}}",
        q.view / 1000.0,
        q.split_k / dev.split_k,
        q.prop_radius,
        q.grass_radius,
        q.shadow_distance,
        q.name
    )
}

/// Apply the page's settings sliders on top of this device's quality.
fn apply_settings(
    time: Res<Time>,
    mut poll: Local<SettingsPoll>,
    dev: Res<DeviceQuality>,
    mut q: ResMut<Quality>,
    mut sun: Query<(Entity, &mut DirectionalLight), With<Sun>>,
    mut commands: Commands,
) {
    poll.wait -= time.delta_secs();
    if poll.wait > 0.0 {
        return;
    }
    poll.wait = 0.3;
    let raw = page_get(SETTINGS_KEY).unwrap_or_default();
    if poll.started && raw == poll.last {
        return;
    }
    poll.started = true;
    poll.last = raw.clone();
    let d = dev.0;
    let mut nq = d;
    for part in raw.split(';') {
        let Some((k, v)) = part.split_once('=') else { continue };
        let Ok(v) = v.trim().parse::<f64>() else { continue };
        match k.trim() {
            "view" => nq.view = (v * 1000.0).clamp(300_000.0, 11_000_000.0),
            "detail" => nq.split_k = (d.split_k * v).clamp(0.3, 2.0),
            "trees" => {
                nq.prop_radius = v.clamp(60.0, 2_000.0);
                nq.near_radius = (v * 0.2).clamp(30.0, 200.0);
            }
            "plants" => {
                nq.grass_radius = v.clamp(0.0, 150.0);
                nq.detail_radius = (v * 0.8).clamp(0.0, 120.0);
            }
            "shadows" => nq.shadow_distance = v.clamp(0.0, 1_000.0) as f32,
            _ => {}
        }
    }
    *q = nq;
    if let Ok((e, mut light)) = sun.single_mut() {
        light.shadows_enabled = nq.shadow_distance > 0.0;
        if nq.shadow_distance > 0.0 {
            commands.entity(e).insert(
                CascadeShadowConfigBuilder {
                    num_cascades: 1,
                    minimum_distance: 0.3,
                    first_cascade_far_bound: nq.shadow_distance * 0.12,
                    maximum_distance: nq.shadow_distance,
                    overlap_proportion: 0.2,
                }
                .build(),
            );
        }
    }
    page_set_text("wsSettings", &quality_json(&nq, &d));
    page_set_text("wsDefaults", &quality_json(&d, &d));
}

#[derive(Default)]
struct BridgeState {
    wait: f32,
    map: Option<MapImage>,
    map_sent: bool,
    places_sent: bool,
    last_travel: String,
}

fn json_text(t: &str) -> String {
    t.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The map screen and fast travel: give the page the island map, the places
/// and where the player is; carry out travel requests from it.
#[allow(clippy::too_many_arguments)]
fn web_bridge(
    time: Res<Time>,
    mut b: Local<BridgeState>,
    w: Res<GameWorld>,
    mut pl: ResMut<Player>,
    mut streamer: ResMut<Streamer>,
    mut plants: ResMut<Plants>,
    mut save: ResMut<SaveState>,
) {
    let t = &w.0;
    if !streamer.ready {
        return;
    }
    if !b.places_sent {
        b.places_sent = true;
        let list: Vec<String> = places(t)
            .iter()
            .map(|p| format!("{{\"name\":\"{}\",\"sub\":\"{}\",\"x\":{:.0},\"z\":{:.0}}}", json_text(&p.name), json_text(&p.sub), p.x, p.z))
            .collect();
        let peaks: Vec<String> = t.shadow_peaks().iter().map(|p| format!("[{:.0},{:.0},{:.0}]", p.x, p.z, t.height(p.x, p.z))).collect();
        page_set_text("wsPlaces", &format!("{{\"places\":[{}],\"peaks\":[{}]}}", list.join(","), peaks.join(",")));
    }
    // the map, a little each frame so the game doesn't stutter (more per
    // frame when frames are slow anyway)
    if !b.map_sent {
        let budget = (time.delta_secs() as f64 * 150.0).clamp(8.0, 400.0);
        let start = now_ms();
        let m = b.map.get_or_insert_with(|| MapImage::new(320, 1_750_000.0));
        while !m.done() && now_ms() - start < budget {
            m.build_rows(t, 2);
        }
        if m.done() {
            page_set_text("wsMapInfo", &format!("{{\"size\":{},\"half\":{:.0}}}", m.size, m.half));
            page_set_bytes("wsMap", &m.rgba);
            b.map = None;
            b.map_sent = true;
        }
    }
    b.wait -= time.delta_secs();
    if b.wait > 0.0 {
        return;
    }
    b.wait = 0.2;
    page_set_text("wsPlayer", &format!("{:.0},{:.0},{:.3},{}", pl.x, pl.z, pl.yaw, pl.fly as u8));
    let Some(req) = page_get(TRAVEL_KEY) else { return };
    if req.is_empty() || req == b.last_travel {
        return;
    }
    b.last_travel = req.clone();
    let parts: Vec<f64> = req.split(';').filter_map(|v| v.trim().parse::<f64>().ok()).collect();
    if parts.len() < 3 || !parts[1].is_finite() || !parts[2].is_finite() {
        return;
    }
    let (x, z) = (parts[1], parts[2]);
    if x.hypot(z) > WALL_R {
        return;
    }
    // go there: land on the ground (or keep flying just above it) and
    // build the world around the new spot at once. (Tests may add
    // ";yaw;pitch;hours;fly;height" to set the view too.)
    pl.x = x;
    pl.z = z;
    if parts.len() >= 6 && parts[3..6].iter().all(|v| v.is_finite()) {
        pl.yaw = parts[3];
        pl.pitch = parts[4];
        pl.hours = parts[5].rem_euclid(24.0);
    }
    if parts.len() >= 7 {
        pl.fly = parts[6] > 0.5;
    }
    let ground = meshgen::ground_under(t, x, z, f64::INFINITY);
    pl.y = match (pl.fly, parts.get(7)) {
        (true, Some(&y)) if y.is_finite() && y > ground + 2.0 => y,
        (true, _) => ground + 150.0,
        _ => ground + EYE,
    };
    pl.vy = 0.0;
    streamer.ready = false;
    plants.ready = false;
    save.dirty = true;
}
