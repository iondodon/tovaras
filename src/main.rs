use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowLevel, WindowMode, WindowPosition, WindowResolution};
use bevy::winit::WinitWindows;
use std::time::Duration;

// ===== Sprite sheet layout =====
const SHEET_COLS: usize = 27;
const SHEET_ROWS: usize = 9;

const ROW_FRAMES: [usize; 9] = [13, 5, 17, 27, 1, 9, 1, 8, 8];
const ROW_IDLE1: usize = 0;
const ROW_WALK_R: usize = 1;
const ROW_IDLE2: usize = 2; // available for variety
const ROW_IDLE3: usize = 3; // available for variety
const ROW_JUMP_R: usize = 4;
const ROW_LAND_R: usize = 5;
const ROW_ROLL: usize = 6;
const ROW_HIDE: usize = 7;
const ROW_CLIMB_R: usize = 8;

const FPS_IDLE: f32 = 10.0;
const FPS_MOVE: f32 = 14.0;
const FPS_CLIMB: f32 = 12.0;
const FPS_HIDE: f32 = 10.0;
const FPS_ROLL: f32 = 16.0;
const FPS_JUMP: f32 = 1.0;
const FPS_LAND: f32 = 20.0;

const SPEED_FLOOR: f32 = 160.0;
const SPEED_WALL: f32 = 120.0;
const SPEED_CEIL: f32 = 160.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Surface {
    Floor,
    RightWall,
    Ceiling,
    LeftWall,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Action {
    Idle,
    Move,
    Climb,
    Jumping,
    Landing,
    Rolling,
    Hiding,
}

#[derive(Resource, Default)]
struct SheetInfo {
    frame_w: f32,
    frame_h: f32,
    atlas_layout: Handle<TextureAtlasLayout>,
    texture: Handle<Image>,
    ready: bool,
}

#[derive(Component)]
struct Pet;

#[derive(Component)]
struct Anim {
    start_index: usize,
    len: usize,
    timer: Timer,
}

impl Anim {
    fn new(start_index: usize, len: usize, fps: f32) -> Self {
        let spf = 1.0 / fps.max(1.0);
        Self {
            start_index,
            len,
            timer: Timer::from_seconds(spf, TimerMode::Repeating),
        }
    }
}

#[derive(Component)]
struct PetState {
    surface: Surface,
    action: Action,
    dir: f32, // +1 clockwise/right/up, -1 opposite
    t: f32,   // scratch timer (unused in this minimal loop)
    window_pos: IVec2,
}

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // Load assets from project root so `pet.png` can sit next to Cargo.toml
                    file_path: ".".into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Pet".into(),
                        resolution: WindowResolution::new(64., 64.), // temp; corrected after image load
                        resizable: false,
                        decorations: false,
                        transparent: true,
                        window_level: WindowLevel::AlwaysOnTop,
                        position: WindowPosition::Centered(MonitorSelection::Primary),
                        mode: WindowMode::Windowed,
                        ..default()
                    }),
                    ..default()
                }),
        )
        .insert_resource(ClearColor(Color::srgba(0.0, 0.0, 0.0, 0.0)))
        .insert_resource(SheetInfo::default())
        .add_systems(Startup, (setup_camera, load_assets, spawn_pet))
        .add_systems(
            Update,
            (finalize_after_load, animate_sprite, pet_fsm_and_move_window),
        )
        .run();
}

/// Camera so sprites can be drawn
fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

/// Queue the texture and make an atlas layout (grid).
fn load_assets(
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut sheet: ResMut<SheetInfo>,
) {
    sheet.texture = asset_server.load("pet.png");
    // placeholder cell size; overwritten after image loads
    let layout = TextureAtlasLayout::from_grid(
        UVec2::new(1, 1),
        SHEET_COLS as u32,
        SHEET_ROWS as u32,
        None,
        None,
    );
    sheet.atlas_layout = layouts.add(layout);
}

fn spawn_pet(mut commands: Commands, sheet: Res<SheetInfo>) {
    commands.spawn((
        SpriteBundle {
            texture: sheet.texture.clone(),
            transform: Transform::from_xyz(0.0, 0.0, 0.0),
            ..default()
        },
        TextureAtlas {
            layout: sheet.atlas_layout.clone(),
            index: row_col_to_index(ROW_IDLE1, 0),
        },
        Pet,
        Anim::new(row_start(ROW_IDLE1), ROW_FRAMES[ROW_IDLE1], FPS_IDLE),
        PetState {
            surface: Surface::Floor,
            action: Action::Move,
            dir: 1.0, // move right along floor initially
            t: 0.0,
            window_pos: IVec2::new(20, 20),
        },
    ));
}

/// Once the image is loaded, compute frame size, update atlas, and resize/reposition the window.
fn finalize_after_load(
    mut sheet: ResMut<SheetInfo>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    winit_windows: NonSend<WinitWindows>,
) {
    if sheet.ready {
        return;
    }
    let Some(img) = images.get(&sheet.texture) else {
        return;
    };

    let w = img.width();
    let h = img.height();
    let frame_w = (w as f32) / (SHEET_COLS as f32);
    let frame_h = (h as f32) / (SHEET_ROWS as f32);
    sheet.frame_w = frame_w;
    sheet.frame_h = frame_h;

    if let Some(layout) = layouts.get_mut(&sheet.atlas_layout) {
        *layout = TextureAtlasLayout::from_grid(
            UVec2::new(frame_w as u32, frame_h as u32),
            SHEET_COLS as u32,
            SHEET_ROWS as u32,
            None,
            None,
        );
    }

    if let Ok((entity, mut win)) = windows.get_single_mut() {
        win.resolution.set(frame_w, frame_h);
        if let Some(raw_win) = winit_windows.get_window(entity) {
            if let Some(mon) = raw_win.current_monitor() {
                let ms = mon.size();
                let floor_y = (ms.height as i32) - (frame_h as i32) - 20;
                win.position = WindowPosition::At(IVec2::new(20, floor_y));
            }
        }
    }

    sheet.ready = true;
}

fn row_col_to_index(row: usize, col: usize) -> usize {
    row * SHEET_COLS + col
}
fn row_start(row: usize) -> usize {
    row * SHEET_COLS
}

/// Only change the animation row/FPS when it actually changes.
/// When changed, snap atlas to the first frame of the new row so it's visible immediately.
fn set_anim_if_changed(anim: &mut Anim, atlas: &mut TextureAtlas, row: usize, fps: f32) {
    let start = row_start(row);
    let len = ROW_FRAMES[row];
    let spf = 1.0 / fps.max(1.0);

    let needs_change = anim.start_index != start
        || anim.len != len
        || (anim.timer.duration().as_secs_f32() - spf).abs() > f32::EPSILON;

    if needs_change {
        anim.start_index = start;
        anim.len = len;
        anim.timer.set_duration(Duration::from_secs_f32(spf));
        anim.timer.reset();
        atlas.index = start; // snap to first column of the row
    }
}

/// Advance the frame within the current row safely.
fn animate_sprite(time: Res<Time>, mut q: Query<(&mut TextureAtlas, &mut Anim), With<Pet>>) {
    for (mut atlas, mut anim) in &mut q {
        anim.timer.tick(time.delta());
        if anim.timer.just_finished() && anim.len > 0 {
            if atlas.index < anim.start_index || atlas.index >= anim.start_index + anim.len {
                atlas.index = anim.start_index;
            }
            let local = atlas.index.saturating_sub(anim.start_index);
            let next_local = if local >= anim.len.saturating_sub(1) {
                0
            } else {
                local + 1
            };
            atlas.index = anim.start_index + next_local;
        }
    }
}

/// Move the window around the desktop edges and pick the right animation/orientation.
fn pet_fsm_and_move_window(
    time: Res<Time>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<(&mut TextureAtlas, &mut Anim, &mut Transform, &mut PetState), With<Pet>>,
    winit_windows: NonSend<WinitWindows>,
    window_entity_q: Query<Entity, With<PrimaryWindow>>,
) {
    let Ok(mut win) = windows.get_single_mut() else {
        return;
    };
    let Ok((mut atlas, mut anim, mut tf, mut st)) = q.get_single_mut() else {
        return;
    };
    let Ok(win_entity) = window_entity_q.get_single() else {
        return;
    };

    // Screen size
    let (screen_w, screen_h) = if let Some(raw) = winit_windows.get_window(win_entity) {
        if let Some(mon) = raw.current_monitor() {
            let s = mon.size();
            (s.width as i32, s.height as i32)
        } else {
            (1280, 720)
        }
    } else {
        (1280, 720)
    };

    let fw: i32 = win.resolution.physical_width() as i32;
    let fh: i32 = win.resolution.physical_height() as i32;

    let mut pos = st.window_pos;
    let dt = time.delta_seconds();

    // Choose row/fps/orientation for the current state, only updating when needed.
    let mut set_surface_visual = |surface: Surface, action: Action, dir: f32| {
        // flip_x = mirror across Y axis; flip_y = mirror across X axis
        let (row, fps, rot, flip_x, flip_y) = match (surface, action) {
            (Surface::Floor, Action::Move) => (ROW_WALK_R, FPS_MOVE, 0.0, dir < 0.0, false),

            (Surface::RightWall, Action::Climb) => (ROW_CLIMB_R, FPS_CLIMB, 0.0, false, false),

            // Ceiling: +90°. Flip X only when moving LEFT -> RIGHT on ceiling.
            (Surface::Ceiling, Action::Climb) => (
                ROW_CLIMB_R,
                FPS_CLIMB,
                std::f32::consts::FRAC_PI_2, // +90°
                dir > 0.0,                   // flip X when L->R
                false,
            ),

            // Left wall: +180° always. If moving UP (dir > 0), mirror by X-axis => flip Y.
            (Surface::LeftWall, Action::Climb) => (
                ROW_CLIMB_R,
                FPS_CLIMB,
                std::f32::consts::PI, // +180°
                false,
                dir > 0.0, // flip Y when going up
            ),

            // (optional) other states can be mapped similarly
            _ => (ROW_WALK_R, FPS_MOVE, 0.0, false, false),
        };

        set_anim_if_changed(&mut anim, &mut atlas, row, fps);

        tf.rotation = Quat::from_rotation_z(rot);
        tf.scale = Vec3::new(
            if flip_x { -1.0 } else { 1.0 },
            if flip_y { -1.0 } else { 1.0 },
            1.0,
        );
    };

    // Clockwise loop: floor (→) -> right wall (↑) -> ceiling (←) -> left wall (↓) -> floor (→)
    match st.surface {
        Surface::Floor => {
            st.action = Action::Move;
            set_surface_visual(Surface::Floor, st.action, st.dir);
            let speed = SPEED_FLOOR * st.dir;
            pos.y = screen_h - fh;
            pos.x = (pos.x as f32 + speed * dt) as i32;

            if pos.x + fw >= screen_w {
                pos.x = screen_w - fw;
                st.surface = Surface::RightWall;
                st.action = Action::Climb;
                st.dir = 1.0; // up
                set_surface_visual(st.surface, st.action, st.dir);
            }
        }
        Surface::RightWall => {
            st.action = Action::Climb;
            set_surface_visual(Surface::RightWall, st.action, st.dir);
            let speed = SPEED_WALL * st.dir;
            pos.x = screen_w - fw;
            pos.y = (pos.y as f32 - speed * dt) as i32;

            if pos.y <= 0 {
                pos.y = 0;
                st.surface = Surface::Ceiling;
                st.action = Action::Climb;
                st.dir = -1.0; // left
                set_surface_visual(st.surface, st.action, st.dir);
            }
        }
        Surface::Ceiling => {
            st.action = Action::Climb;
            set_surface_visual(Surface::Ceiling, st.action, st.dir);
            let speed = SPEED_CEIL * st.dir;
            pos.y = 0;
            pos.x = (pos.x as f32 + speed * dt) as i32;

            if pos.x <= 0 {
                pos.x = 0;
                st.surface = Surface::LeftWall;
                st.action = Action::Climb;
                st.dir = -1.0; // down
                set_surface_visual(st.surface, st.action, st.dir);
            }
        }
        Surface::LeftWall => {
            st.action = Action::Climb;
            set_surface_visual(Surface::LeftWall, st.action, st.dir);
            let speed = SPEED_WALL * st.dir;
            pos.x = 0;
            pos.y = (pos.y as f32 - speed * dt) as i32;

            if pos.y + fh >= screen_h {
                pos.y = screen_h - fh;
                st.surface = Surface::Floor;
                st.action = Action::Move;
                st.dir = 1.0; // right
                set_surface_visual(st.surface, st.action, st.dir);
            }
        }
    }

    // Clamp + apply
    pos.x = pos.x.clamp(0, screen_w.saturating_sub(fw));
    pos.y = pos.y.clamp(0, screen_h.saturating_sub(fh));
    st.window_pos = pos;
    win.position = WindowPosition::At(pos);
}
