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
const ROW_GIVING_FLOWERS: usize = 3; // was IDLE3
const ROW_JUMP_R: usize = 4;
const ROW_LAND_R: usize = 5;
const ROW_SLEEP: usize = 6; // was ROLL
const ROW_HIDE: usize = 7;
const ROW_CLIMB_R: usize = 8;

const FPS_IDLE: f32 = 10.0;
const FPS_MOVE: f32 = 14.0;
const FPS_CLIMB: f32 = 12.0;
const FPS_HIDE: f32 = 10.0;
const FPS_SLEEP: f32 = 8.0;
// slower “romantic” giving-flowers animation:
const FPS_GIVING_FLOWERS: f32 = 6.0;
const FPS_JUMP: f32 = 1.0; // we hold this pose during flight
const FPS_LAND: f32 = 20.0;

const SPEED_FLOOR: f32 = 160.0;
const SPEED_WALL: f32 = 120.0;
const SPEED_CEIL: f32 = 160.0;

// ===== Jump physics =====
const GRAVITY: f32 = 1800.0; // px/s^2 downward (+)
const FLOOR_JUMP_VY0: f32 = -900.0; // px/s (negative = up)
const WALL_JUMP_VY0: f32 = -880.0; // px/s (initial up)

// ===== Test sequencer config =====
const CASE_DUR: f32 = 1.5; // seconds per case (paused during Jump/Land)
const START_MARGIN: i32 = 40;
// Let GivingFlowers play its full 27 frames at the chosen FPS (+ small padding)
const DUR_GIVING_FLOWERS: f32 = (ROW_FRAMES[ROW_GIVING_FLOWERS] as f32) / FPS_GIVING_FLOWERS + 0.5;

// Landing behavior
const LANDING_HOLD: f32 = 0.5; // animation hold on floor
const LANDING_DRIFT: f32 = 140.0; // px/s slide along floor during landing

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
    Sleeping,      // row 6
    Hiding,        // row 7
    GivingFlowers, // row 3, floor-only in place
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum FlightKind {
    None,
    Parabola, // used for floor & wall jumps
}

#[derive(Component)]
struct PetState {
    surface: Surface,
    action: Action,
    dir: f32,          // +1 or -1 for facing/motion on current surface
    window_pos: IVec2, // top-left px

    // Flight state
    flight: FlightKind,
    flight_from: Surface, // takeoff surface for visuals during flight
    vx: f32,              // px/s
    vy: f32,              // px/s (positive downward)
    landing_left: f32,    // seconds to hold landing anim

    // Targeting
    target_x: i32, // target X on the floor (for floor & wall jumps)
}

// === Test driver types ===

#[derive(Clone, Copy)]
enum JumpPreset {
    // Floor jump: start %, target % of [0..max_x]
    FloorPct { start_pct: f32, target_pct: f32 },
    // Wall -> floor jump: target % of [0..max_x]
    WallToFloorPct { target_pct: f32 },
    None,
}

#[derive(Clone, Copy)]
struct TestCase {
    surface: Surface,
    action: Action,
    dir: f32, // usually movement sense; for jumps we keep it for facing
    dur: f32,
    preset: JumpPreset,
}

#[derive(Resource)]
struct TestSeq {
    cases: Vec<TestCase>,
    i: usize,
    left: f32,
}

impl Default for TestSeq {
    fn default() -> Self {
        let mut cases = Vec::new();

        // ===== Floor movement / idle / sleeping / giving flowers / hiding =====
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Move,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Move,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Idle,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Sleeping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::GivingFlowers,
            dir: 1.0,
            dur: DUR_GIVING_FLOWERS,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Hiding,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });

        // ===== Floor → Floor jumps (arbitrary distances) =====
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.10,
                target_pct: 0.90,
            },
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.90,
                target_pct: 0.10,
            },
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.50,
                target_pct: 0.10,
            },
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.50,
                target_pct: 0.90,
            },
        });

        // ===== Right wall =====
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Climb,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Climb,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Hiding,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        // Wall → floor jumps from right wall to arbitrary floor X
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::WallToFloorPct { target_pct: 0.20 },
        });
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::WallToFloorPct { target_pct: 0.50 },
        });

        // ===== Ceiling (no jumps) =====
        cases.push(TestCase {
            surface: Surface::Ceiling,
            action: Action::Climb,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Ceiling,
            action: Action::Climb,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        cases.push(TestCase {
            surface: Surface::Ceiling,
            action: Action::Hiding,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });

        // ===== Left wall =====
        cases.push(TestCase {
            surface: Surface::LeftWall,
            action: Action::Climb,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        }); // down
        cases.push(TestCase {
            surface: Surface::LeftWall,
            action: Action::Climb,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        }); // up
        cases.push(TestCase {
            surface: Surface::LeftWall,
            action: Action::Hiding,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::None,
        });
        // Wall → floor jumps from left wall to arbitrary floor X
        cases.push(TestCase {
            surface: Surface::LeftWall,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::WallToFloorPct { target_pct: 0.80 },
        });
        cases.push(TestCase {
            surface: Surface::LeftWall,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::WallToFloorPct { target_pct: 0.40 },
        });

        Self {
            cases,
            i: 0,
            left: CASE_DUR,
        }
    }
}

#[derive(Component)]
struct TestTag;

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: ".".into(), // load pet.png from project root
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Pet".into(),
                        resolution: WindowResolution::new(64., 64.), // overwritten after image load
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
        .insert_resource(TestSeq::default())
        .add_systems(Startup, (setup_camera, load_assets, spawn_pet))
        .add_systems(
            Update,
            (
                finalize_after_load,
                animate_sprite,
                test_driver,
                apply_motion_and_orientation,
            ),
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
        TestTag,
        Anim::new(row_start(ROW_IDLE1), ROW_FRAMES[ROW_IDLE1], FPS_IDLE),
        PetState {
            surface: Surface::Floor,
            action: Action::Move,
            dir: 1.0,
            window_pos: IVec2::new(20, 20),
            flight: FlightKind::None,
            flight_from: Surface::Floor,
            vx: 0.0,
            vy: 0.0,
            landing_left: 0.0,
            target_x: 0,
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
                let floor_y = (ms.height as i32) - (frame_h as i32) - START_MARGIN;
                win.position = WindowPosition::At(IVec2::new(START_MARGIN, floor_y));
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

/// Drive the sequence: set PetState to the current case, advance when allowed.
/// IMPORTANT: Pause while Jumping/flight and during Landing to keep Jump visible until floor.
fn test_driver(
    time: Res<Time>,
    mut seq: ResMut<TestSeq>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<&mut PetState, With<TestTag>>,
    winit_windows: NonSend<WinitWindows>,
    window_entity_q: Query<Entity, With<PrimaryWindow>>,
    sheet: Res<SheetInfo>,
) {
    let Ok(mut st) = q.get_single_mut() else {
        return;
    };
    let Ok(mut win) = windows.get_single_mut() else {
        return;
    };
    let Ok(win_entity) = window_entity_q.get_single() else {
        return;
    };

    // Pause the sequencer while in air or landing
    if st.flight != FlightKind::None || matches!(st.action, Action::Jumping | Action::Landing) {
        return;
    }

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

    let fw = win.resolution.physical_width() as i32;
    let fh = win.resolution.physical_height() as i32;

    // If the cell size isn't known yet, wait
    if sheet.frame_w == 0.0 || sheet.frame_h == 0.0 {
        return;
    }

    seq.left -= time.delta_seconds();
    if seq.left <= 0.0 {
        seq.i = (seq.i + 1) % seq.cases.len();
        let case = seq.cases[seq.i];
        seq.left = case.dur;

        // Apply case
        st.surface = case.surface;
        st.action = case.action;
        st.dir = case.dir;

        // reset flight/landing state on case change
        st.flight = FlightKind::None;
        st.flight_from = st.surface;
        st.vx = 0.0;
        st.vy = 0.0;
        st.landing_left = 0.0;
        st.target_x = 0;

        // Bounds helpers
        let max_x = (screen_w - fw).max(0);
        let max_y = (screen_h - fh).max(0);
        let mid_y = (screen_h - fh) / 2;

        // Position window to a reasonable start for each surface/direction
        let mut pos = st.window_pos;

        match st.surface {
            Surface::Floor => {
                let y = max_y;
                if matches!(st.action, Action::Jumping) {
                    let (start_x, target_x) = match case.preset {
                        JumpPreset::FloorPct {
                            start_pct,
                            target_pct,
                        } => {
                            let sx = ((max_x as f32) * start_pct).round() as i32;
                            let tx = ((max_x as f32) * target_pct).round() as i32;
                            (sx.clamp(0, max_x), tx.clamp(0, max_x))
                        }
                        _ => {
                            let sx = START_MARGIN;
                            let tx = max_x - START_MARGIN;
                            (sx.clamp(0, max_x), tx.clamp(0, max_x))
                        }
                    };
                    pos = IVec2::new(start_x, y);
                    st.target_x = target_x;
                    st.dir = if target_x >= start_x { 1.0 } else { -1.0 };
                } else {
                    let x = if st.dir >= 0.0 {
                        START_MARGIN
                    } else {
                        max_x - START_MARGIN
                    };
                    pos = IVec2::new(x, y);
                }
            }
            Surface::RightWall => {
                let x = max_x;
                let y = if matches!(st.action, Action::Jumping) {
                    mid_y
                } else if st.dir >= 0.0 {
                    max_y - START_MARGIN
                } else {
                    START_MARGIN
                };
                pos = IVec2::new(x, y.clamp(0, max_y));
                if matches!(st.action, Action::Jumping) {
                    st.target_x = match case.preset {
                        JumpPreset::WallToFloorPct { target_pct } => {
                            ((max_x as f32) * target_pct).round() as i32
                        }
                        _ => START_MARGIN,
                    }
                    .clamp(0, max_x);
                    // face left on landing from right wall
                    st.dir = -1.0;
                }
            }
            Surface::Ceiling => {
                let y = 0;
                let x = if st.dir < 0.0 {
                    max_x - START_MARGIN
                } else {
                    START_MARGIN
                };
                pos = IVec2::new(x.clamp(0, max_x), y);
            }
            Surface::LeftWall => {
                let x = 0;
                let y = if matches!(st.action, Action::Jumping) {
                    mid_y
                } else if st.dir < 0.0 {
                    START_MARGIN
                } else {
                    max_y - START_MARGIN
                };
                pos = IVec2::new(x, y.clamp(0, max_y));
                if matches!(st.action, Action::Jumping) {
                    st.target_x = match case.preset {
                        JumpPreset::WallToFloorPct { target_pct } => {
                            ((max_x as f32) * target_pct).round() as i32
                        }
                        _ => max_x - START_MARGIN,
                    }
                    .clamp(0, max_x);
                    // face right on landing from left wall
                    st.dir = 1.0;
                }
            }
        }

        st.window_pos = pos;
        win.position = WindowPosition::At(pos);
    }
}

/// Decide visuals (row, fps, rotation, flips) for (surface, action, dir).
/// flip_x = mirror across Y axis (left/right); flip_y = mirror across X axis (up/down)
fn set_visual_for(
    surface: Surface,
    action: Action,
    dir: f32,
    anim: &mut Anim,
    atlas: &mut TextureAtlas,
    tf: &mut Transform,
) {
    let (row, fps, rot, flip_x, flip_y) = match (surface, action) {
        // Floor
        (Surface::Floor, Action::Move) => (ROW_WALK_R, FPS_MOVE, 0.0, dir < 0.0, false),
        (Surface::Floor, Action::Idle) => (ROW_IDLE1, FPS_IDLE, 0.0, false, false),
        (Surface::Floor, Action::Sleeping) => (ROW_SLEEP, FPS_SLEEP, 0.0, false, false),
        (Surface::Floor, Action::GivingFlowers) => {
            (ROW_GIVING_FLOWERS, FPS_GIVING_FLOWERS, 0.0, false, false)
        }
        (Surface::Floor, Action::Hiding) => (ROW_HIDE, FPS_HIDE, 0.0, false, true),
        (Surface::Floor, Action::Jumping) => (ROW_JUMP_R, FPS_JUMP, 0.0, dir < 0.0, false),
        (Surface::Floor, Action::Landing) => (ROW_LAND_R, FPS_LAND, 0.0, dir < 0.0, false),

        // Right wall
        (Surface::RightWall, Action::Climb) => (ROW_CLIMB_R, FPS_CLIMB, 0.0, false, dir < 0.0),
        (Surface::RightWall, Action::Hiding) => (
            ROW_HIDE,
            FPS_HIDE,
            -std::f32::consts::FRAC_PI_2,
            false,
            false,
        ),
        (Surface::RightWall, Action::Jumping) => (ROW_JUMP_R, FPS_JUMP, 0.0, true, false), // mirror Y

        // Ceiling (no jumping)
        (Surface::Ceiling, Action::Climb) => (
            ROW_CLIMB_R,
            FPS_CLIMB,
            std::f32::consts::FRAC_PI_2,
            false,
            dir > 0.0,
        ),
        (Surface::Ceiling, Action::Hiding) => (ROW_HIDE, FPS_HIDE, 0.0, false, false),

        // Left wall
        (Surface::LeftWall, Action::Climb) => (
            ROW_CLIMB_R,
            FPS_CLIMB,
            std::f32::consts::PI,
            false,
            dir > 0.0,
        ),
        (Surface::LeftWall, Action::Hiding) => (
            ROW_HIDE,
            FPS_HIDE,
            std::f32::consts::FRAC_PI_2,
            false,
            false,
        ),
        (Surface::LeftWall, Action::Jumping) => (ROW_JUMP_R, FPS_JUMP, 0.0, false, false),

        _ => (ROW_IDLE1, FPS_IDLE, 0.0, false, false),
    };

    set_anim_if_changed(anim, atlas, row, fps);
    tf.rotation = Quat::from_rotation_z(rot);
    tf.scale = Vec3::new(
        if flip_x { -1.0 } else { 1.0 },
        if flip_y { -1.0 } else { 1.0 },
        1.0,
    );
}

/// Physics + window motion + ensuring correct visuals.
fn apply_motion_and_orientation(
    time: Res<Time>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<(&mut TextureAtlas, &mut Anim, &mut Transform, &mut PetState), With<TestTag>>,
) {
    let Ok(mut win) = windows.get_single_mut() else {
        return;
    };
    let Ok((mut atlas, mut anim, mut tf, mut st)) = q.get_single_mut() else {
        return;
    };

    let fw: i32 = win.resolution.physical_width() as i32;
    let fh: i32 = win.resolution.physical_height() as i32;
    let dt = time.delta_seconds();

    // A consistent virtual desktop rectangle (fallback if monitor query isn't available here)
    let (screen_w, screen_h) = (
        1920.max(fw + 2 * START_MARGIN),
        1080.max(fh + 2 * START_MARGIN),
    );
    let max_x = screen_w.saturating_sub(fw);
    let max_y = screen_h.saturating_sub(fh); // "floor" y
    let mut pos = st.window_pos;

    // ENTER FLIGHT on Jumping (ceiling jumps disabled)
    if matches!(st.action, Action::Jumping) && st.flight == FlightKind::None {
        if matches!(st.surface, Surface::Ceiling) {
            // still disabled
            set_visual_for(
                st.surface, st.action, st.dir, &mut anim, &mut atlas, &mut tf,
            );
        } else {
            st.flight_from = st.surface;
            set_visual_for(
                st.flight_from,
                Action::Jumping,
                st.dir,
                &mut anim,
                &mut atlas,
                &mut tf,
            );

            match st.surface {
                Surface::Floor => {
                    // Same start/end height: T = 2*|vy0| / g
                    let t = 2.0 * (-FLOOR_JUMP_VY0) / GRAVITY;
                    let dx = (st.target_x - pos.x) as f32;
                    st.vx = if t > 0.0 { dx / t } else { 0.0 };
                    st.vy = FLOOR_JUMP_VY0;
                }
                Surface::RightWall | Surface::LeftWall => {
                    // Time to floor from current height (quadratic)
                    let y0 = pos.y as f32;
                    let c = y0 - (max_y as f32);
                    let a = 0.5 * GRAVITY;
                    let b = WALL_JUMP_VY0;
                    let disc = b * b - 4.0 * a * c;
                    let t = if disc >= 0.0 {
                        (-b + disc.sqrt()) / (2.0 * a)
                    } else {
                        1.0
                    };

                    let dx = (st.target_x - pos.x) as f32;
                    st.vx = if t > 0.0 { dx / t } else { 0.0 };
                    st.vy = WALL_JUMP_VY0;
                }
                Surface::Ceiling => {}
            }
            st.flight = FlightKind::Parabola;
            st.landing_left = 0.0;
        }
    }

    // Flight step: keep Jump sprite until floor touch
    if st.flight != FlightKind::None {
        st.vy += GRAVITY * dt; // gravity downward (+)
        pos.x = (pos.x as f32 + st.vx * dt) as i32;
        pos.y = (pos.y as f32 + st.vy * dt) as i32;

        // Bounds
        pos.x = pos.x.clamp(0, max_x);
        pos.y = pos.y.clamp(0, max_y);

        // Keep jump visuals from the takeoff surface
        set_visual_for(
            st.flight_from,
            Action::Jumping,
            st.dir,
            &mut anim,
            &mut atlas,
            &mut tf,
        );

        // Land on floor
        if pos.y >= max_y {
            st.flight = FlightKind::None;
            st.surface = Surface::Floor;
            st.action = Action::Landing;

            // Heading rules:
            // - RightWall -> land heading LEFT
            // - LeftWall  -> land heading RIGHT
            // - Floor     -> face towards target (vx sign)
            st.dir = match st.flight_from {
                Surface::RightWall => -1.0,
                Surface::LeftWall => 1.0,
                _ => {
                    if st.vx >= 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }
            };

            // Snap X to exact target for floor or wall -> floor jumps
            pos.x = st.target_x.clamp(0, max_x);

            st.landing_left = LANDING_HOLD;
            set_visual_for(
                Surface::Floor,
                Action::Landing,
                st.dir,
                &mut anim,
                &mut atlas,
                &mut tf,
            );
        }
    } else {
        // Not in flight: normal motions + visuals
        set_visual_for(
            st.surface, st.action, st.dir, &mut anim, &mut atlas, &mut tf,
        );

        match st.surface {
            Surface::Floor => {
                match st.action {
                    Action::Move => {
                        pos.x = (pos.x as f32 + SPEED_FLOOR * st.dir * dt) as i32;
                    }
                    Action::Landing => {
                        // Slide during landing
                        pos.x = (pos.x as f32 + LANDING_DRIFT * st.dir * dt) as i32;
                    }
                    // No movement while Sleeping, Idle, GivingFlowers, Hiding
                    Action::Sleeping
                    | Action::Idle
                    | Action::GivingFlowers
                    | Action::Hiding
                    | Action::Climb
                    | Action::Jumping => {}
                }
                pos.y = max_y;
            }
            Surface::RightWall => {
                if matches!(st.action, Action::Climb) {
                    pos.x = max_x;
                    pos.y = (pos.y as f32 - SPEED_WALL * st.dir * dt) as i32; // up when dir>0
                }
                pos.x = max_x;
                pos.y = pos.y.clamp(0, max_y);
            }
            Surface::Ceiling => {
                if matches!(st.action, Action::Climb) {
                    pos.y = 0;
                    pos.x = (pos.x as f32 + SPEED_CEIL * st.dir * dt) as i32; // left when dir<0, right when dir>0
                }
                pos.y = 0;
                pos.x = pos.x.clamp(0, max_x);
            }
            Surface::LeftWall => {
                if matches!(st.action, Action::Climb) {
                    pos.x = 0;
                    pos.y = (pos.y as f32 - SPEED_WALL * st.dir * dt) as i32; // down when dir<0
                }
                pos.x = 0;
                pos.y = pos.y.clamp(0, max_y);
            }
        }
    }

    // Landing hold timer
    if matches!(st.action, Action::Landing) {
        st.landing_left -= dt;
        if st.landing_left <= 0.0 {
            st.action = Action::Move; // continue walking on floor
        }
    }

    st.window_pos = IVec2::new(pos.x.clamp(0, max_x), pos.y.clamp(0, max_y));
    win.position = WindowPosition::At(st.window_pos);
}
