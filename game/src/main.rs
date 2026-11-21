//! WorldShard — the playable game (Bevy).
//!
//! The island is the size of Australia, so the world is streamed: terrain
//! chunks get finer near the camera, trees and grass appear around the
//! player, and everything is drawn relative to the camera so 32-bit floats
//! stay precise thousands of kilometres from the centre. The world itself,
//! its meshes, the day/night maths and the save format all come from the
//! engine-independent `worldshard` library.

use std::collections::{HashMap, HashSet};

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::primitives::Aabb;
use bevy::render::view::{ColorGrading, ColorGradingGlobal, ColorGradingSection};
use bevy::window::WindowResolution;

use worldshard::lod::{self, NodeKey};
use worldshard::meshgen::{self, MeshData, PropKinds, GRASS_CELL, PROP_CELL};
use worldshard::save::{SaveData, STORAGE_KEY};
use worldshard::sky::{clock_text, sun_state};
use worldshard::worldgen::terrain::WALL_R;
use worldshard::worldgen::Terrain;

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
    grass_radius: f64,
    msaa: bool,
    name: &'static str,
}

impl Quality {
    fn detect() -> Quality {
        let full = Quality { scale: None, split_k: lod::SPLIT_K, prop_radius: 520.0, grass_radius: 56.0, msaa: true, name: "full" };
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
                return Quality { scale: Some(dpr.min(0.75)), split_k: 0.6, prop_radius: 200.0, grass_radius: 0.0, msaa: false, name: "lite" };
            }
            if phone {
                return Quality { scale: Some(dpr.min(1.0)), split_k: 0.8, prop_radius: 320.0, grass_radius: 40.0, msaa: true, name: "phone" };
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

#[derive(Resource)]
struct Mats {
    ground: Handle<StandardMaterial>,
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
    props: HashMap<(i64, i64), Option<Entity>>,
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
        .insert_resource(ClearColor(Color::srgb(0.55, 0.72, 0.92)))
        .insert_resource(Banner::default())
        .insert_resource(TouchState::default())
        .insert_resource(Streamer::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
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
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (move_player, stream_world, place_relative, sky_and_light, update_hud, autosave, hide_loader).chain(),
        )
        .add_systems(Update, log_memory)
        .run();
}

fn to_mesh(m: MeshData) -> (Mesh, Aabb) {
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
        commands.spawn((Mesh3d(assets.add(mesh)), MeshMaterial3d(mat), aabb, ChildOf(parent)));
    }
    parent
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    quality: Res<Quality>,
) {
    info!("WorldShard quality: {:?}", *quality);
    let world = Terrain::new();

    let mats = Mats {
        ground: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.92,
            reflectance: 0.12,
            double_sided: true,
            cull_mode: None,
            ..default()
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

    // ---- the Cloudwall, at the edge of the Rim Sea ----
    let cloud_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        double_sided: true,
        cull_mode: None,
        fog_enabled: false,
        ..default()
    });
    spawn_anchored(&mut commands, &mut meshes, Anchor { x: 0.0, y: 0.0, z: 0.0 }, vec![(meshgen::cloudwall_mesh(), cloud_mat)]);

    // ---- sky dome: a soft gradient, brighter at the horizon ----
    let sky_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.72, 0.92),
        unlit: true,
        fog_enabled: false,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let (sky_mesh, sky_aabb) = to_mesh(meshgen::sky_dome(15_000_000.0));
    commands.spawn((Mesh3d(meshes.add(sky_mesh)), MeshMaterial3d(sky_mat), sky_aabb, Transform::default(), SkyDome));

    // ---- sunlight (WebGL2 allows one directional light: sun by day, moon by night) ----
    commands.spawn((DirectionalLight { illuminance: 7000.0, shadows_enabled: false, ..default() }, Transform::default(), Sun));
    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 1200.0, ..default() });

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
    if let Some(save) = storage_get().and_then(|s| SaveData::from_json(&s)) {
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
    let ground = meshgen::drawn_height(&world, player.x, player.z);
    player.y = match fly_y {
        Some(y) => y.max(ground + 2.0),
        None => ground.max(0.0) + EYE,
    };

    // ---- camera: hazy distance, a little extra colour ----
    commands.spawn((
        Camera3d::default(),
        if quality.msaa { Msaa::Sample4 } else { Msaa::Off },
        Projection::Perspective(PerspectiveProjection { fov: 65f32.to_radians(), near: 0.3, far: 30_000_000.0, ..default() }),
        Transform::from_xyz(0.0, player.y as f32, 0.0),
        DistanceFog {
            color: Color::srgb(0.72, 0.82, 0.93),
            directional_light_color: Color::srgba(1.0, 0.92, 0.75, 0.45),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::from_visibility_colors(90_000.0, Color::srgb(0.35, 0.5, 0.66), Color::srgb(0.8, 0.86, 1.0)),
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
        .spawn(Node { position_type: PositionType::Absolute, top: Val::Px(10.0), width: Val::Percent(100.0), justify_content: JustifyContent::Center, ..default() })
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
    let ground = meshgen::drawn_height(t, pl.x, pl.z);

    if pl.fly {
        let speed = FLY_SPEEDS[pl.fly_speed] * if run { 3.0 } else { 1.0 };
        let (sp, cp) = (pl.pitch.sin(), pl.pitch.cos());
        pl.x += (sy * cp * fz + cy * fx) * speed * dt;
        pl.z += (-cy * cp * fz + sy * fx) * speed * dt;
        pl.y += (sp * fz + up_down) * speed * dt;
        let g = meshgen::drawn_height(t, pl.x, pl.z).max(0.0);
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
        pl.x += vx * dt;
        pl.z += vz * dt;
        clamp_to_wall(&mut pl, &mut banner);
        let g = meshgen::drawn_height(t, pl.x, pl.z);
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

    // clock: a game day is a real day
    pl.play_sec += dt;
    pl.hours += dt / 3600.0;
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
        pl.y = meshgen::drawn_height(t, pl.x, pl.z).max(0.0) + EYE;
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
    kinds: Res<Kinds>,
    mats: Res<Mats>,
    pl: Res<Player>,
    q: Res<Quality>,
    mut st: ResMut<Streamer>,
) {
    let t = &w.0;
    let first = !st.ready;
    let ground = meshgen::drawn_height(t, pl.x, pl.z).max(0.0);
    let above = pl.y - ground;

    // terrain: build missing chunks nearest first, a few per frame
    let want = lod::select_k([pl.x, pl.y, pl.z], above, q.split_k);
    let budget = if first { usize::MAX } else { 3 };
    let mut built = 0;
    for k in &want {
        if built >= budget {
            break;
        }
        if st.chunks.contains_key(k) {
            continue;
        }
        let c = meshgen::build_chunk(t, *k);
        let mut parts = Vec::new();
        if let Some(m) = c.terrain {
            parts.push((m, mats.ground.clone()));
        }
        if let Some(m) = c.water {
            parts.push((m, mats.water.clone()));
        }
        if let Some(m) = c.lava {
            parts.push((m, mats.lava.clone()));
        }
        let e = spawn_anchored(&mut commands, &mut assets, Anchor { x: c.origin[0], y: 0.0, z: c.origin[1] }, parts);
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

    // trees, rocks and grass near the player (not when flying high)
    let near_cells = |cell: f64, radius: f64| -> Vec<(i64, i64)> {
        let (ci, cj) = ((pl.x / cell).floor() as i64, (pl.z / cell).floor() as i64);
        let n = (radius / cell).ceil() as i64 + 1;
        let mut v = Vec::new();
        for j in cj - n..=cj + n {
            for i in ci - n..=ci + n {
                let (cx, cz) = ((i as f64 + 0.5) * cell, (j as f64 + 0.5) * cell);
                let d = (cx - pl.x).hypot(cz - pl.z);
                if d < radius {
                    v.push((i, j));
                }
            }
        }
        v.sort_by_key(|&(i, j)| (i - ci).abs() + (j - cj).abs());
        v
    };
    let props_wanted = if above < 400.0 { near_cells(PROP_CELL, q.prop_radius) } else { Vec::new() };
    let grass_wanted = if above < 60.0 && q.grass_radius > 0.0 { near_cells(GRASS_CELL, q.grass_radius) } else { Vec::new() };
    let mut pb = if first { 40 } else { 2 };
    for &(i, j) in &props_wanted {
        if pb == 0 {
            break;
        }
        if st.props.contains_key(&(i, j)) {
            continue;
        }
        let e = meshgen::prop_cell(t, &kinds.0, i, j).map(|m| {
            let (x, z) = ((i as f64 + 0.5) * PROP_CELL, (j as f64 + 0.5) * PROP_CELL);
            spawn_anchored(&mut commands, &mut assets, Anchor { x, y: 0.0, z }, vec![(m, mats.plants.clone())])
        });
        st.props.insert((i, j), e);
        pb -= 1;
    }
    let mut gb = if first { usize::MAX } else { 4 };
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
    let keep_p: HashSet<(i64, i64)> = props_wanted.iter().copied().collect();
    let keep_g: HashSet<(i64, i64)> = grass_wanted.iter().copied().collect();
    let drop_p: Vec<(i64, i64)> = st.props.keys().copied().filter(|k| !keep_p.contains(k)).collect();
    for k in drop_p {
        if let Some(Some(e)) = st.props.remove(&k) {
            commands.entity(e).despawn();
        }
    }
    let drop_g: Vec<(i64, i64)> = st.grass.keys().copied().filter(|k| !keep_g.contains(k)).collect();
    for k in drop_g {
        if let Some(Some(e)) = st.grass.remove(&k) {
            commands.entity(e).despawn();
        }
    }
    st.ready = true;
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
    let above = (pl.y - meshgen::drawn_height(&w.0, pl.x, pl.z).max(0.0)).max(0.0);
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
    dome: Query<&MeshMaterial3d<StandardMaterial>, With<SkyDome>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let s = sun_state(pl.hours);
    clear.0 = rgb(s.sky_col);
    ambient.brightness = 550.0 + 1050.0 * s.day;
    let amax = s.sky_amb[2].max(0.01);
    ambient.color = rgb([s.sky_amb[0] / amax, s.sky_amb[1] / amax, 1.0]);
    if let Ok((mut light, mut tf)) = sky_light.single_mut() {
        let sun = 6000.0 * s.sun_col[0].max(s.sun_col[1]).max(s.sun_col[2]);
        let moon = 9000.0 * s.moon_col[2];
        let (col, dir, lux) = if sun >= moon { (s.sun_col, s.sun_dir, sun) } else { ([0.6, 0.7, 1.0], s.moon_dir, moon) };
        let m = col[0].max(col[1]).max(col[2]).max(0.001);
        light.illuminance = lux;
        light.color = rgb([col[0] / m, col[1] / m, col[2] / m]);
        *tf = Transform::IDENTITY.looking_to(-Vec3::from_array(dir), Vec3::Y);
    }
    // haze: thick near the ground, thinner high up so the island can be seen whole
    let ground = meshgen::drawn_height(&w.0, pl.x, pl.z).max(0.0);
    let above = (pl.y - ground).max(0.0) as f32;
    let vis = 90_000.0 + above * 5.0;
    let day = s.day;
    for mut fog in &mut fogs {
        fog.color = rgb(s.fog_col);
        fog.falloff = FogFalloff::from_visibility_colors(
            vis,
            rgb([0.35 * day + 0.03, 0.5 * day + 0.04, 0.66 * day + 0.08]),
            rgb([s.fog_col[0] * 1.05, s.fog_col[1] * 1.05, s.fog_col[2] * 1.08]),
        );
        fog.directional_light_color = Color::srgba(1.0, 0.9, 0.7, 0.45 * day);
    }
    if let Ok(h) = dome.single() {
        if let Some(mat) = materials.get_mut(&h.0) {
            mat.base_color = rgb(s.sky_col);
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
    let ground = meshgen::drawn_height(&w.0, pl.x, pl.z).max(0.0);
    let alt = if pl.fly { pl.y } else { pl.y - EYE }.max(0.0);
    let alt_text = if alt >= 10_000.0 { format!("{:.0} km", alt / 1000.0) } else { format!("{:.0} m", alt) };
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
    *next = time.elapsed_secs() + 20.0;
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
