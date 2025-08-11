use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowLevel, WindowMode, WindowPosition, WindowResolution};
use bevy::winit::WinitWindows;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ===== Scale (5x smaller window & sprite) =====
const SCALE: f32 = 1.0 / 5.0;

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

// ===== Speeds (slowed down) =====
const SPEED_FLOOR: f32 = 90.0;
const SPEED_WALL: f32 = 70.0;
const SPEED_CEIL: f32 = 90.0;

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
const LANDING_DRIFT: f32 = 100.0; // px/s slide along floor during landing (slightly reduced)

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

    // Targets
    target_x: i32,                       // floor target X
    wall_target: Option<(Surface, i32)>, // (Left/Right wall, target Y)
}

// === Test driver types ===

#[derive(Clone, Copy)]
enum JumpPreset {
    // Floor jump: start %, target % of [0..max_x]
    FloorPct {
        start_pct: f32,
        target_pct: f32,
    },
    // Floor -> Wall jump: choose wall, start % on floor, and target Y % on wall height
    FloorToWall {
        wall: Surface,
        start_pct: f32,
        target_y_pct: f32,
    },
    // Wall -> floor jump: target % of [0..max_x]
    WallToFloorPct {
        target_pct: f32,
    },
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

        // ===== Floor → Floor jumps =====
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.10,
                target_pct: 0.85,
            },
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorPct {
                start_pct: 0.85,
                target_pct: 0.15,
            },
        });

        // ===== Floor → Wall jumps (TEST) =====
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorToWall {
                wall: Surface::RightWall,
                start_pct: 0.30,
                target_y_pct: 0.40,
            },
        });
        cases.push(TestCase {
            surface: Surface::Floor,
            action: Action::Jumping,
            dir: -1.0,
            dur: CASE_DUR,
            preset: JumpPreset::FloorToWall {
                wall: Surface::LeftWall,
                start_pct: 0.70,
                target_y_pct: 0.60,
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
        // Wall → floor jump from right wall
        cases.push(TestCase {
            surface: Surface::RightWall,
            action: Action::Jumping,
            dir: 1.0,
            dur: CASE_DUR,
            preset: JumpPreset::WallToFloorPct { target_pct: 0.25 },
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

        Self {
            cases,
            i: 0,
            left: CASE_DUR,
        }
    }
}

#[derive(Component)]
struct TestTag;

// ----------------- Run Modes -----------------
#[derive(Clone, Copy)]
enum RunMode {
    Test,
    Random,
}

#[derive(Resource)]
struct Mode(RunMode);

// Simple xorshift RNG (no external crates)
#[derive(Resource)]
struct TinyRng(u32);
impl TinyRng {
    fn seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(1))
            .subsec_nanos() as u32
            ^ 0xA3C59AC3;
        Self(seed)
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    fn f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
    fn range_f32(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }
    fn range_i32(&mut self, a: i32, b: i32) -> i32 {
        if b <= a {
            a
        } else {
            a + (self.f32() * ((b - a + 1) as f32)).floor() as i32
        }
    }
    fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

// Random controller
#[derive(Resource)]
struct RandomCtrl {
    left: f32,
}

impl Default for RandomCtrl {
    fn default() -> Self {
        // Longer action durations overall (slower changes)
        Self { left: 0.6 }
    }
}

fn main() {
    // Mode selection
    let args: Vec<String> = std::env::args().collect();
    let run_mode = if args.iter().any(|a| a == "--test") {
        RunMode::Test
    } else {
        RunMode::Random
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: ".".into(), // load pet.png from project root
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "tovaras".into(),
                    name: Some("tovaras".into()),
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
    .insert_resource(Mode(run_mode))
    .add_systems(Startup, (setup_camera, load_assets, spawn_pet))
    .add_systems(
        Update,
        (
            finalize_after_load,
            animate_sprite,
            apply_motion_and_orientation,
        ),
    );

    match run_mode {
        RunMode::Test => {
            app.insert_resource(TestSeq::default())
                .add_systems(Update, test_driver);
            info!("Running in TEST mode (pass --random to switch to random mode).");
        }
        RunMode::Random => {
            app.insert_resource(TinyRng::seeded())
                .insert_resource(RandomCtrl::default())
                .add_systems(Update, random_driver);
            info!("Running in RANDOM mode (pass --test to run deterministic test cases).");
        }
    }

    app.run();
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
            // Start scaled down so the sprite matches the smaller window
            transform: Transform {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
                scale: Vec3::splat(SCALE),
            },
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
            dir: 1.0,
            window_pos: IVec2::new(20, 20),
            flight: FlightKind::None,
            flight_from: Surface::Floor,
            vx: 0.0,
            vy: 0.0,
            landing_left: 0.0,
            target_x: 0,
            wall_target: None,
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
        // Window is 5x smaller than the sprite frame
        win.resolution.set(frame_w * SCALE, frame_h * SCALE);
        if let Some(raw_win) = winit_windows.get_window(entity) {
            if let Some(mon) = raw_win.current_monitor() {
                let ms = mon.size();
                // Floor Y must use the scaled window height
                let floor_y = (ms.height as i32) - (frame_h * SCALE) as i32 - START_MARGIN;
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
            dir > 0.0,
            false,
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
    // Preserve base SCALE when flipping
    let sx = if flip_x { -SCALE } else { SCALE };
    let sy = if flip_y { -SCALE } else { SCALE };
    tf.rotation = Quat::from_rotation_z(rot);
    tf.scale = Vec3::new(sx, sy, 1.0);
}

/// Physics + window motion + ensuring correct visuals.
fn apply_motion_and_orientation(
    time: Res<Time>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<(&mut TextureAtlas, &mut Anim, &mut Transform, &mut PetState)>,
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

    // A consistent virtual desktop rectangle (fallback)
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
            // disabled by spec
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
                    // Two possibilities: floor->floor OR floor->wall (left/right)
                    if let Some((wall, ty)) = st.wall_target.take() {
                        // We already have a wall target (set by driver). Solve for t using Y(t) = ty.
                        let y0 = max_y as f32;
                        let c = y0 - (ty as f32);
                        let a = 0.5 * GRAVITY;
                        let b = FLOOR_JUMP_VY0;
                        let disc = b * b - 4.0 * a * c;
                        let t = if disc >= 0.0 {
                            // pick positive root
                            (-b + disc.sqrt()) / (2.0 * a)
                        } else {
                            1.0
                        };

                        // vx to reach target wall X at that time
                        let wall_x = if matches!(wall, Surface::LeftWall) {
                            0
                        } else {
                            max_x
                        };
                        let dx = (wall_x - pos.x) as f32;
                        st.vx = if t > 0.0 { dx / t } else { 0.0 };
                        st.vy = FLOOR_JUMP_VY0;
                    } else {
                        // Default floor->floor (use target_x)
                        let t = 2.0 * (-FLOOR_JUMP_VY0) / GRAVITY;
                        let dx = (st.target_x - pos.x) as f32;
                        st.vx = if t > 0.0 { dx / t } else { 0.0 };
                        st.vy = FLOOR_JUMP_VY0;
                    }
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

    // Flight step: keep Jump sprite until floor/wall touch
    if st.flight != FlightKind::None {
        st.vy += GRAVITY * dt; // gravity downward (+)
        pos.x = (pos.x as f32 + st.vx * dt) as i32;
        pos.y = (pos.y as f32 + st.vy * dt) as i32;

        // Bounds temp clamp
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

        // Hit wall target?
        if let Some((wall, ty)) = st.wall_target {
            match wall {
                Surface::LeftWall if pos.x <= 0 => {
                    // stick to wall at target y (clamped), start climbing
                    pos.x = 0;
                    pos.y = ty.clamp(0, max_y);
                    st.flight = FlightKind::None;
                    st.surface = Surface::LeftWall;
                    st.action = Action::Climb;
                    // choose climb dir from current vertical velocity: up if still going up, else down
                    st.dir = if st.vy <= 0.0 { 1.0 } else { -1.0 };
                    st.wall_target = None;
                }
                Surface::RightWall if pos.x >= max_x => {
                    pos.x = max_x;
                    pos.y = ty.clamp(0, max_y);
                    st.flight = FlightKind::None;
                    st.surface = Surface::RightWall;
                    st.action = Action::Climb;
                    st.dir = if st.vy <= 0.0 { 1.0 } else { -1.0 };
                    st.wall_target = None;
                }
                _ => {}
            }
        }

        // Land on floor if we reached it (and no wall capture happened)
        if st.flight != FlightKind::None && pos.y >= max_y {
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

            // Snap X to exact floor target if it exists
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
            st.wall_target = None;
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

                        // Auto-climb when reaching corners (continuous)
                        if pos.x <= 0 {
                            pos.x = 0;
                            st.surface = Surface::LeftWall;
                            st.action = Action::Climb;
                            st.dir = 1.0; // start climbing up
                        } else if pos.x >= max_x {
                            pos.x = max_x;
                            st.surface = Surface::RightWall;
                            st.action = Action::Climb;
                            st.dir = 1.0; // start climbing up
                        }
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
                    // up when dir>0, down when dir<0 (Y decreases upward)
                    pos.y = (pos.y as f32 - SPEED_WALL * st.dir * dt) as i32;

                    // --- NEW: transitions at corners ---
                    if pos.y <= 0 && st.dir > 0.0 {
                        // climbed up to the top-right corner -> onto the ceiling moving left
                        pos.y = 0;
                        st.surface = Surface::Ceiling;
                        st.action = Action::Climb;
                        st.dir = -1.0; // move left on ceiling
                    } else if pos.y >= max_y && st.dir < 0.0 {
                        // climbed down to the floor at right corner -> onto floor moving left
                        pos.y = max_y;
                        st.surface = Surface::Floor;
                        st.action = Action::Move;
                        st.dir = -1.0; // move left on floor
                    }
                }
                pos.x = max_x;
                pos.y = pos.y.clamp(0, max_y);
            }
            Surface::Ceiling => {
                if matches!(st.action, Action::Climb) {
                    pos.y = 0;
                    pos.x = (pos.x as f32 + SPEED_CEIL * st.dir * dt) as i32; // left when dir<0, right when dir>0

                    // Keep ceiling->wall transitions so the loop is continuous
                    if pos.x <= 0 && st.dir < 0.0 {
                        // reached top-left corner -> down the left wall
                        pos.x = 0;
                        st.surface = Surface::LeftWall;
                        st.action = Action::Climb;
                        st.dir = -1.0; // climb down
                    } else if pos.x >= max_x && st.dir > 0.0 {
                        // reached top-right corner -> down the right wall
                        pos.x = max_x;
                        st.surface = Surface::RightWall;
                        st.action = Action::Climb;
                        st.dir = -1.0; // climb down
                    }
                }
                pos.y = 0;
                pos.x = pos.x.clamp(0, max_x);
            }
            Surface::LeftWall => {
                if matches!(st.action, Action::Climb) {
                    pos.x = 0;
                    // up when dir>0, down when dir<0 (Y decreases upward)
                    pos.y = (pos.y as f32 - SPEED_WALL * st.dir * dt) as i32;

                    // --- NEW: transitions at corners ---
                    if pos.y <= 0 && st.dir > 0.0 {
                        // climbed up to the top-left corner -> onto the ceiling moving right
                        pos.y = 0;
                        st.surface = Surface::Ceiling;
                        st.action = Action::Climb;
                        st.dir = 1.0; // move right on ceiling
                    } else if pos.y >= max_y && st.dir < 0.0 {
                        // climbed down to the floor at left corner -> onto floor moving right
                        pos.y = max_y;
                        st.surface = Surface::Floor;
                        st.action = Action::Move;
                        st.dir = 1.0; // move right on floor
                    }
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

// ----------------- TEST MODE DRIVER -----------------
fn test_driver(
    time: Res<Time>,
    mut seq: ResMut<TestSeq>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<&mut PetState>,
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

        apply_case_deterministic(&mut st, &mut win, screen_w, screen_h, fw, fh, case);
    }
}

// ----------------- RANDOM MODE DRIVER (continuous) -----------------
fn random_driver(
    time: Res<Time>,
    mut rnd: ResMut<TinyRng>,
    mut ctrl: ResMut<RandomCtrl>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut q: Query<&mut PetState>,
) {
    let Ok(mut win) = windows.get_single_mut() else {
        return;
    };
    let Ok(mut st) = q.get_single_mut() else {
        return;
    };

    // Pause while in flight / landing
    if st.flight != FlightKind::None || matches!(st.action, Action::Jumping | Action::Landing) {
        return;
    }

    let fw = win.resolution.physical_width() as i32;
    let fh = win.resolution.physical_height() as i32;
    let screen_w = 1920.max(fw + 2 * START_MARGIN);
    let screen_h = 1080.max(fh + 2 * START_MARGIN);

    ctrl.left -= time.delta_seconds();
    if ctrl.left > 0.0 {
        return;
    }

    // ----- pick next random case respecting rules (longer durations) -----
    let mut case = pick_random_case(&mut rnd, st.surface);

    // duration per action (randomized ranges) — longer to keep actions longer
    let dur = match case.action {
        Action::GivingFlowers => DUR_GIVING_FLOWERS,
        Action::Sleeping => rnd.range_f32(3.0, 6.0),
        Action::Hiding => rnd.range_f32(1.2, 2.0),
        Action::Idle => rnd.range_f32(2.0, 4.0),
        Action::Move => rnd.range_f32(2.0, 4.0),
        Action::Climb => rnd.range_f32(2.0, 4.0),
        Action::Jumping => 0.2, // ignored during flight
        Action::Landing => 0.2, // ignored (landing hold separate)
    };
    ctrl.left = dur;

    // Continuous: never reposition. Only set targets if jumping and clamp to legal edge for the current surface.
    apply_case_continuous(
        &mut st, &mut win, screen_w, screen_h, fw, fh, &mut rnd, &mut case,
    );
}

// Build a random case for the given surface
fn pick_random_case(rng: &mut TinyRng, current_surface: Surface) -> TestCase {
    let action = match current_surface {
        Surface::Floor => {
            // Allow: Move, Idle, Sleeping, GivingFlowers, Hiding, Jumping
            let roll = rng.next_u32() % 6;
            match roll {
                0 => Action::Move,
                1 => Action::Idle,
                2 => Action::Sleeping,
                3 => Action::GivingFlowers,
                4 => Action::Hiding,
                _ => Action::Jumping,
            }
        }
        Surface::RightWall | Surface::LeftWall => {
            // Allow: Climb, Hiding, Jumping (to floor)
            if rng.chance(0.20) {
                Action::Hiding
            } else if rng.chance(0.30) {
                Action::Jumping
            } else {
                Action::Climb
            }
        }
        Surface::Ceiling => {
            // Allow: Climb, Hiding (no jumping)
            if rng.chance(0.30) {
                Action::Hiding
            } else {
                Action::Climb
            }
        }
    };

    let dir = match (current_surface, action) {
        // Floor move left/right randomly
        (Surface::Floor, Action::Move) => {
            if rng.chance(0.5) {
                -1.0
            } else {
                1.0
            }
        }
        (Surface::Floor, Action::Jumping) => {
            if rng.chance(0.5) {
                -1.0
            } else {
                1.0
            }
        }
        // Climb direction: up or down depending on surface
        (Surface::RightWall, Action::Climb) => {
            if rng.chance(0.5) {
                1.0
            } else {
                -1.0
            }
        }
        (Surface::LeftWall, Action::Climb) => {
            if rng.chance(0.5) {
                1.0
            } else {
                -1.0
            }
        }
        (Surface::Ceiling, Action::Climb) => {
            if rng.chance(0.5) {
                1.0
            } else {
                -1.0
            }
        } // right or left on ceiling
        // Other actions ignore dir or use facing only
        _ => 1.0,
    };

    let preset = match (current_surface, action) {
        (Surface::Floor, Action::Jumping) => {
            // target will be derived later (could be floor or wall in random driver)
            JumpPreset::FloorPct {
                start_pct: 0.0,
                target_pct: 0.0,
            }
        }
        (Surface::RightWall, Action::Jumping) | (Surface::LeftWall, Action::Jumping) => {
            JumpPreset::WallToFloorPct { target_pct: 0.0 }
        }
        _ => JumpPreset::None,
    };

    TestCase {
        surface: current_surface,
        action,
        dir,
        dur: 1.0,
        preset,
    }
}

// Deterministic test: positions are explicitly set for clarity (teleport OK in TEST mode)
fn apply_case_deterministic(
    st: &mut PetState,
    win: &mut Window,
    screen_w: i32,
    screen_h: i32,
    fw: i32,
    fh: i32,
    case: TestCase,
) {
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
    st.wall_target = None;

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
                match case.preset {
                    JumpPreset::FloorPct {
                        start_pct,
                        target_pct,
                    } => {
                        let start_x = ((max_x as f32) * start_pct).round() as i32;
                        let target_x = ((max_x as f32) * target_pct).round() as i32;
                        pos = IVec2::new(start_x.clamp(0, max_x), y);
                        st.target_x = target_x.clamp(0, max_x);
                        st.dir = if st.target_x >= pos.x { 1.0 } else { -1.0 };
                    }
                    JumpPreset::FloorToWall {
                        wall,
                        start_pct,
                        target_y_pct,
                    } => {
                        let start_x = ((max_x as f32) * start_pct).round() as i32;
                        pos = IVec2::new(start_x.clamp(0, max_x), y);
                        let ty = ((max_y as f32) * target_y_pct).round() as i32;
                        // store wall target for flight solver
                        st.wall_target = Some((wall, ty.clamp(0, max_y)));
                        // face toward the chosen wall
                        let wall_x = if matches!(wall, Surface::LeftWall) {
                            0
                        } else {
                            max_x
                        };
                        st.dir = if wall_x >= pos.x { 1.0 } else { -1.0 };
                    }
                    _ => {}
                }
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
                if let JumpPreset::WallToFloorPct { target_pct } = case.preset {
                    st.target_x = ((max_x as f32) * target_pct).round() as i32;
                }
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
                if let JumpPreset::WallToFloorPct { target_pct } = case.preset {
                    st.target_x = ((max_x as f32) * target_pct).round() as i32;
                }
                // face right on landing from left wall
                st.dir = 1.0;
            }
        }
    }

    st.window_pos = pos;
    win.position = WindowPosition::At(pos);
}

// Continuous random: do NOT reposition; only set targets and ensure we remain on valid edges
fn apply_case_continuous(
    st: &mut PetState,
    win: &mut Window,
    screen_w: i32,
    screen_h: i32,
    fw: i32,
    fh: i32,
    rng: &mut TinyRng,
    case: &mut TestCase,
) {
    st.surface = case.surface;
    st.action = case.action;
    st.dir = case.dir;

    // keep current position
    let mut pos = st.window_pos;

    // reset flight/landing
    st.flight = FlightKind::None;
    st.flight_from = st.surface;
    st.vx = 0.0;
    st.vy = 0.0;
    st.landing_left = 0.0;
    st.target_x = 0;
    st.wall_target = None;

    let max_x = (screen_w - fw).max(0);
    let max_y = (screen_h - fh).max(0);

    match st.surface {
        Surface::Floor => {
            // stick to floor
            pos.y = max_y;
            pos.x = pos.x.clamp(0, max_x);

            if matches!(st.action, Action::Jumping) {
                // 50% chance: jump to wall; 50%: jump to floor
                if rng.chance(0.5) {
                    // Floor -> Wall
                    let to_left = rng.chance(0.5);
                    let wall = if to_left {
                        Surface::LeftWall
                    } else {
                        Surface::RightWall
                    };
                    let wall_x = if to_left { 0 } else { max_x };
                    let target_y = rng.range_i32(
                        (0.10 * (max_y as f32)) as i32,
                        (0.90 * (max_y as f32)) as i32,
                    );

                    // Store wall target; vx/vy will be computed when flight starts
                    st.wall_target = Some((wall, target_y));
                    // Face toward the wall
                    st.dir = if wall_x >= pos.x { 1.0 } else { -1.0 };
                } else {
                    // Floor -> Floor (choose a target relative to current x)
                    let min_dx = (screen_w as f32 * 0.10) as i32;
                    let max_dx = (screen_w as f32 * 0.35) as i32;
                    let dx = rng.range_i32(min_dx, max_dx) * if st.dir >= 0.0 { 1 } else { -1 };
                    let tx = (pos.x + dx).clamp(0, max_x);
                    st.target_x = tx;
                    st.dir = if tx >= pos.x { 1.0 } else { -1.0 };
                    st.wall_target = None;
                }
            }
        }
        Surface::RightWall => {
            // lock to right edge
            pos.x = max_x;
            pos.y = pos.y.clamp(0, max_y);

            if matches!(st.action, Action::Jumping) {
                // pick any floor x; keep y to start from current height
                st.target_x = rng.range_i32(0, max_x);
                // land heading left from right wall
                st.dir = -1.0;
            }
        }
        Surface::Ceiling => {
            // lock to top
            pos.y = 0;
            pos.x = pos.x.clamp(0, max_x);
            // no jumps on ceiling
        }
        Surface::LeftWall => {
            // lock to left edge
            pos.x = 0;
            pos.y = pos.y.clamp(0, max_y);

            if matches!(st.action, Action::Jumping) {
                st.target_x = rng.range_i32(0, max_x);
                // land heading right from left wall
                st.dir = 1.0;
            }
        }
    }

    st.window_pos = pos;
    win.position = WindowPosition::At(pos);
}
