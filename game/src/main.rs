//! WorldShard — the playable game (Bevy).
//!
//! This crate is a thin layer: the world, its meshes, the day/night maths
//! and the save format all come from the engine-independent `worldshard`
//! library. Here we put them on screen, read the controls and keep the save.

use bevy::asset::RenderAssetUsages;
use bevy::input::mouse::MouseMotion;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::window::PrimaryWindow;

use worldshard::meshgen::{self, MeshData};
use worldshard::save::{SaveData, STORAGE_KEY};
use worldshard::sky::{clock_text, sun_state};
use worldshard::worldgen::grid::WALL_R;
use worldshard::worldgen::World;

// Pulled in only to switch on browser randomness for Bevy's dependencies.
#[cfg(target_arch = "wasm32")]
use {getrandom as _, getrandom02 as _};

// ---------------------------------------------------------------------
// Resources and components
// ---------------------------------------------------------------------

#[derive(Resource)]
struct GameWorld(World);

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
}

#[derive(Resource)]
struct SaveState {
    dirty: bool,
    since_change: f32,
    since_save: f32,
    last_x: f64,
    last_z: f64,
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

#[derive(Component)]
struct Sun;
#[derive(Component)]
struct PropChunk {
    centre: Vec2,
}
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

const EYE: f64 = 1.7;
const PROP_DRAW_DISTANCE: f32 = 1150.0;

// ---------------------------------------------------------------------
// Save storage: browser localStorage on the web, a file on desktop
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
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

/// Removes the page's "Building the world…" cover once frames are drawing.
#[cfg(target_arch = "wasm32")]
fn remove_loading_cover() {
    if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("loading")) {
        el.remove();
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn remove_loading_cover() {}

#[cfg(not(target_arch = "wasm32"))]
fn storage_get() -> Option<String> {
    std::fs::read_to_string(format!("{STORAGE_KEY}.json")).ok()
}
#[cfg(not(target_arch = "wasm32"))]
fn storage_set(data: &str) -> bool {
    std::fs::write(format!("{STORAGE_KEY}.json"), data).is_ok()
}

// ---------------------------------------------------------------------
// App
// ---------------------------------------------------------------------

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.45, 0.64, 0.88)))
        .insert_resource(Banner::default())
        .insert_resource(TouchState::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "WorldShard".into(),
                canvas: Some("#bevy".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (move_player, sky_and_light, cull_props, update_hud, autosave, hide_loader).chain())
        .run();
}

fn to_mesh(m: MeshData) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, m.colors)
        .with_inserted_indices(Indices::U32(m.indices))
}

fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let world = World::generate(|_| {});

    // ---- terrain, lava, sea ----
    let ground = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.95,
        reflectance: 0.08,
        ..default()
    });
    commands.spawn((Mesh3d(meshes.add(to_mesh(meshgen::terrain_mesh(&world)))), MeshMaterial3d(ground), Transform::default()));

    let lava = meshgen::lava_mesh(&world);
    if !lava.is_empty() {
        let lava_mat = materials.add(StandardMaterial { base_color: Color::WHITE, unlit: true, ..default() });
        commands.spawn((Mesh3d(meshes.add(to_mesh(lava))), MeshMaterial3d(lava_mat), Transform::default()));
    }

    let sea = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.12,
        reflectance: 0.35,
        ..default()
    });
    commands.spawn((Mesh3d(meshes.add(to_mesh(meshgen::water_mesh(&world)))), MeshMaterial3d(sea), Transform::default()));

    // ---- trees, rocks, mines (merged per chunk) ----
    let props_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.05,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let kinds = meshgen::PropKinds::build();
    let props = meshgen::scatter(&world);
    for chunk in meshgen::prop_chunks(&props, &kinds) {
        commands.spawn((
            Mesh3d(meshes.add(to_mesh(chunk.mesh))),
            MeshMaterial3d(props_mat.clone()),
            Transform::default(),
            Visibility::Inherited,
            PropChunk { centre: Vec2::new(chunk.centre[0], chunk.centre[1]) },
        ));
    }

    // ---- the Cloudwall ----
    let cloud_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    for m in meshgen::cloud_meshes(12) {
        commands.spawn((Mesh3d(meshes.add(to_mesh(m))), MeshMaterial3d(cloud_mat.clone()), Transform::default()));
    }

    // ---- sun, moon, sky light ----
    // WebGL2 allows one directional light: it is the sun by day, the moon by night.
    commands.spawn((DirectionalLight { illuminance: 9000.0, shadows_enabled: false, ..default() }, Transform::default(), Sun));
    commands.insert_resource(AmbientLight { color: Color::WHITE, brightness: 400.0, ..default() });

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
    };
    let mut status = "New game".to_string();
    if let Some(save) = storage_get().and_then(|s| SaveData::from_json(&s)) {
        player.x = save.x;
        player.z = save.z;
        player.yaw = save.yaw;
        player.pitch = save.pitch;
        player.hours = save.hours;
        player.day = save.day;
        player.visited = save.visited;
        player.play_sec = save.play_sec;
        status = "Save loaded".to_string();
    }
    player.y = ground_y(&world, player.x, player.z) + EYE;

    // ---- camera ----
    commands.spawn((
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection { fov: 65f32.to_radians(), near: 0.3, far: 9000.0, ..default() }),
        Transform::from_xyz(player.x as f32, player.y as f32, player.z as f32),
        DistanceFog {
            color: Color::srgb(0.7, 0.8, 0.9),
            falloff: FogFalloff::ExponentialSquared { density: 0.0003 },
            ..default()
        },
    ));

    // ---- HUD ----
    let ink = Color::srgb(0.95, 0.92, 0.84);
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
        TextColor(ink.with_alpha(0.8)),
        Node { position_type: PositionType::Absolute, top: Val::Px(34.0), left: Val::Px(12.0), ..default() },
        StatusText,
    ));
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Px(10.0),
            width: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((Text::new("N"), TextFont { font_size: 18.0, ..default() }, TextColor(ink), CompassText));
        });
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: Val::Percent(20.0),
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|p| {
            p.spawn((Text::new(""), TextFont { font_size: 40.0, ..default() }, TextColor(ink.with_alpha(0.0)), BannerTitle));
            p.spawn((Text::new(""), TextFont { font_size: 18.0, ..default() }, TextColor(ink.with_alpha(0.0)), BannerSub));
        });
    commands.spawn((
        Text::new("WASD / arrows move | hold mouse button and drag to look | Shift run | Space jump\nPhone: drag on the left to walk (push far to run), drag on the right to look"),
        TextFont { font_size: 12.0, ..default() },
        TextColor(ink.with_alpha(0.75)),
        Node { position_type: PositionType::Absolute, bottom: Val::Px(10.0), left: Val::Px(12.0), ..default() },
    ));

    commands.insert_resource(SaveState { dirty: false, since_change: 0.0, since_save: 0.0, last_x: player.x, last_z: player.z, last_visited: player.visited.len(), status });
    commands.insert_resource(player);
    commands.insert_resource(GameWorld(world));
}

fn ground_y(w: &World, x: f64, z: f64) -> f64 {
    let h = w.height_at(x, z);
    if h < 0.0 {
        h.max(-0.9)
    } else {
        h
    }
}

// ---------------------------------------------------------------------
// Movement: keyboard + mouse on desktop, two-finger zones on phones
// ---------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut motion: EventReader<MouseMotion>,
    touches: Res<Touches>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut ts: ResMut<TouchState>,
    gw: Res<GameWorld>,
    mut pl: ResMut<Player>,
    mut banner: ResMut<Banner>,
    mut cams: Query<&mut Transform, With<Camera3d>>,
) {
    let dt = time.delta_secs().min(0.05) as f64;
    let w = &gw.0;

    // look
    let mut look = Vec2::ZERO;
    for ev in motion.read() {
        if mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Right) {
            look += ev.delta * 0.0042;
        }
    }
    let width = windows.single().map(|w| w.width()).unwrap_or(800.0);
    for t in touches.iter_just_pressed() {
        if t.position().x < width * 0.45 && ts.move_id.is_none() {
            ts.move_id = Some(t.id());
        } else if ts.look_id.is_none() {
            ts.look_id = Some(t.id());
        }
    }
    for t in touches.iter_just_released().chain(touches.iter_just_canceled()) {
        if ts.move_id == Some(t.id()) {
            ts.move_id = None;
        }
        if ts.look_id == Some(t.id()) {
            ts.look_id = None;
        }
    }
    let mut joy = Vec2::ZERO;
    if let Some(t) = ts.move_id.and_then(|id| touches.get_pressed(id)) {
        joy = ((t.position() - t.start_position()) / 60.0).clamp_length_max(1.0);
    }
    if let Some(t) = ts.look_id.and_then(|id| touches.get_pressed(id)) {
        look += t.delta() * 0.0065;
    }
    pl.yaw += look.x as f64;
    pl.pitch = (pl.pitch - look.y as f64).clamp(-1.3, 1.3);
    if keys.pressed(KeyCode::ArrowLeft) {
        pl.yaw -= 1.8 * dt;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        pl.yaw += 1.8 * dt;
    }

    // walk
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
    let ground = w.height_at(pl.x, pl.z);
    let swimming = ground < -1.0;
    let run = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) || joy.length() > 0.92;
    let speed = match (swimming, run) {
        (true, true) => 7.0,
        (true, false) => 3.5,
        (false, true) => 13.0,
        (false, false) => 5.5,
    };
    let (sy, cy) = (pl.yaw.sin(), pl.yaw.cos());
    let mut vx = (sy * fz + cy * fx) * speed;
    let mut vz = (-cy * fz + sy * fx) * speed;
    if !swimming && (vx != 0.0 || vz != 0.0) {
        let ahead = w.height_at(pl.x + vx * 0.25, pl.z + vz * 0.25) - ground;
        let grade = ahead / (vx.hypot(vz) * 0.25);
        if grade > 0.35 {
            let k = (1.0 - (grade - 0.35) * 1.4).max(0.15);
            vx *= k;
            vz *= k;
        }
    }
    let (mut nx, mut nz) = (pl.x + vx * dt, pl.z + vz * dt);
    let r = nx.hypot(nz);
    if r > WALL_R {
        nx *= WALL_R / r;
        nz *= WALL_R / r;
        if banner.show_t <= 0.0 {
            banner.title = "The Cloudwall".into();
            banner.subtitle = "It can only be crossed late in the game".into();
            banner.show_t = 3.5;
        }
    }
    pl.x = nx;
    pl.z = nz;

    // fall, jump, swim
    let g = ground_y(w, pl.x, pl.z);
    if swimming {
        pl.y += (0.55 - pl.y) * (dt * 4.0).min(1.0);
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

    // clock
    pl.play_sec += dt;
    pl.hours += dt / 60.0; // one game hour per real minute
    if pl.hours >= 24.0 {
        pl.hours -= 24.0;
        pl.day += 1;
    }

    // region banner
    let (name, desc) = w.place_name(pl.x, pl.z);
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

    for mut tf in &mut cams {
        tf.translation = Vec3::new(pl.x as f32, pl.y as f32, pl.z as f32);
        tf.rotation = Quat::from_rotation_y(-pl.yaw as f32) * Quat::from_rotation_x(pl.pitch as f32);
    }
}

// ---------------------------------------------------------------------
// Day and night
// ---------------------------------------------------------------------

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0))
}

fn sky_and_light(
    pl: Res<Player>,
    mut clear: ResMut<ClearColor>,
    mut ambient: ResMut<AmbientLight>,
    mut sky_light: Query<(&mut DirectionalLight, &mut Transform), With<Sun>>,
    mut fogs: Query<&mut DistanceFog>,
) {
    let s = sun_state(pl.hours);
    clear.0 = rgb(s.sky_col);
    ambient.brightness = 190.0 + 450.0 * s.day;
    let amax = s.sky_amb[2].max(0.01);
    ambient.color = rgb([s.sky_amb[0] / amax, s.sky_amb[1] / amax, 1.0]);
    if let Ok((mut light, mut tf)) = sky_light.single_mut() {
        let sun = 11000.0 * s.sun_col[0].max(s.sun_col[1]).max(s.sun_col[2]);
        let moon = 4000.0 * s.moon_col[2];
        let (col, dir, lux) = if sun >= moon { (s.sun_col, s.sun_dir, sun) } else { ([0.6, 0.7, 1.0], s.moon_dir, moon) };
        let m = col[0].max(col[1]).max(col[2]).max(0.001);
        light.illuminance = lux;
        light.color = rgb([col[0] / m, col[1] / m, col[2] / m]);
        *tf = Transform::IDENTITY.looking_to(-Vec3::from_array(dir), Vec3::Y);
    }
    for mut fog in &mut fogs {
        fog.color = rgb(s.fog_col);
    }
}

fn cull_props(pl: Res<Player>, mut chunks: Query<(&PropChunk, &mut Visibility)>) {
    let p = Vec2::new(pl.x as f32, pl.z as f32);
    for (c, mut vis) in &mut chunks {
        let want = if c.centre.distance(p) < PROP_DRAW_DISTANCE { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

// ---------------------------------------------------------------------
// HUD and saving
// ---------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn update_hud(
    pl: Res<Player>,
    banner: Res<Banner>,
    save: Res<SaveState>,
    mut texts: ParamSet<(
        Query<&mut Text, With<ClockText>>,
        Query<&mut Text, With<StatusText>>,
        Query<&mut Text, With<CompassText>>,
        Query<(&mut Text, &mut TextColor), With<BannerTitle>>,
        Query<(&mut Text, &mut TextColor), With<BannerSub>>,
    )>,
) {
    if let Ok(mut t) = texts.p0().single_mut() {
        let s = format!("Day {}  {}", pl.day, clock_text(pl.hours));
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
    let ink = Color::srgba(0.95, 0.92, 0.84, alpha);
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
}

/// Saves to the browser a few seconds after the player moves, straight away
/// on reaching a new place, and every 30 s while playing. The page's
/// "Save file" button turns this into a file the player can keep in Drive.
fn autosave(time: Res<Time>, pl: Res<Player>, mut save: ResMut<SaveState>) {
    let dt = time.delta_secs();
    save.since_save += dt;
    if (pl.x - save.last_x).hypot(pl.z - save.last_z) > 2.0 {
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
    };
    save.status = if storage_set(&data.to_json()) {
        format!("Saved - Day {} {}", pl.day, clock_text(pl.hours))
    } else {
        "Saving is not available here".into()
    };
    save.dirty = false;
    save.since_change = 0.0;
    save.since_save = 0.0;
    save.last_x = pl.x;
    save.last_z = pl.z;
    save.last_visited = pl.visited.len();
}

fn hide_loader(mut frames: Local<u32>) {
    if *frames < 3 {
        *frames += 1;
        if *frames == 3 {
            remove_loading_cover();
        }
    }
}
