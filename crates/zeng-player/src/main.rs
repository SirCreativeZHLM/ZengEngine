#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod blocks;
mod game;
mod scene;
mod value;
mod vm;

use avian3d::prelude::*;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use game::*;
use std::collections::HashSet;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::path::PathBuf;
use std::time::SystemTime;
use vm::{step, Ctx, Status, Thread};
use bevy::camera::RenderTarget;
use bevy::render::render_resource::TextureUsages;
use bevy::ui::IsDefaultUiCamera;

#[derive(Resource)]
struct Args {
    #[allow(dead_code)]
    stage: Option<(u32, u32)>,
    screenshot: Option<PathBuf>,
    press: Vec<(String, u32)>,
    quality: Option<String>,
}

/// Mouse state, fed from the window (normal play) or from stdin (editor stage).
#[derive(Resource, Default)]
struct Pointer {
    pos: Option<Vec2>,
    size: Vec2,
    delta: Vec2,
    zoom: f32,
    down: bool,
    just_pressed: bool,
}

/// Headless mode: render to an image and stream JPEG frames to stdout for the editor's stage.
#[derive(Resource)]
struct Stage {
    handle: Handle<Image>,
    in_flight: Arc<AtomicUsize>,
}

#[derive(Resource)]
struct StageRx(Mutex<mpsc::Receiver<String>>);

#[derive(Resource, Default)]
struct Env {
    bloom: bool,
    fog: Option<scene::FogDef>,
    quality: String,
}

#[derive(Resource, Default)]
struct Watch {
    stamp: Option<SystemTime>,
    since: f32,
}

#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
}

#[derive(Component)]
struct Fx;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut dir, mut shot, mut press, mut quality, mut stage) = (None, None, vec![], None, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--screenshot" => shot = args.next().map(PathBuf::from),
            "--press" => {
                if let Some((k, f)) = args.next().and_then(|v| v.split_once(':').map(|(k, f)| (k.to_string(), f.parse().unwrap_or(1)))) {
                    press.push((k, f));
                }
            }
            "--quality" => quality = args.next(),
            "--stage" => {
                stage = args.next().and_then(|v| v.split_once('x').and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))));
            }
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let dir = dir.unwrap_or_else(|| {
        // An exported game keeps its project next to the executable.
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
        let candidates = exe.into_iter().flat_map(|e| [e.join("project"), e.join("../Resources/project")]);
        candidates.into_iter().find(|p| p.join("project.json").exists()).unwrap_or_else(|| {
            eprintln!("usage: zeng-player <project folder> [--screenshot out.png] [--quality low|medium|high]");
            std::process::exit(2);
        })
    });
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);

    let mut app = App::new();
    let assets = AssetPlugin { file_path: dir.to_string_lossy().into_owned(), ..default() };
    if let Some((w, h)) = stage {
        // No window: render into an image, read commands from stdin, write frames to stdout.
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: None, exit_condition: bevy::window::ExitCondition::DontExit, close_when_requested: false, ..default() })
                .set(assets)
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::log::LogPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::from_secs_f64(1.0 / 30.0)));
        let mut img = Image::new_target_texture(w, h, bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb, None);
        img.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = app.world_mut().resource_mut::<Assets<Image>>().add(img);
        app.insert_resource(Stage { handle, in_flight: Arc::new(AtomicUsize::new(0)) });
        app.insert_resource(Pointer { size: Vec2::new(w as f32, h as f32), ..default() });
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
            let _ = tx.send("Q".into());
        });
        app.insert_resource(StageRx(Mutex::new(rx)));
        app.add_systems(Update, (stage_input.before(track_mouse), stage_capture.after(run_scripts)));
    } else {
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: Some(Window { title: "ZengEngine".into(), ..default() }), ..default() })
                .set(assets),
        );
        app.init_resource::<Pointer>();
        app.add_systems(Update, gather_pointer.before(track_mouse));
    }
    app.add_plugins(PhysicsPlugins::default())
        .insert_resource(Game { dir: dir.clone(), ..default() })
        .insert_resource(Env::default())
        .insert_resource(Watch::default())
        .insert_resource(Args { stage, screenshot: shot.clone(), press, quality })
        .add_systems(Startup, |world: &mut World| load_project(world))
        .add_systems(
            Update,
            (hot_reload, fake_keys, track_mouse, run_scripts, camera_rig, camera_effects, hud, quit_if_asked).chain(),
        );
    if shot.is_some() {
        app.add_systems(Update, auto_screenshot);
    }
    app.run();
}

fn project_stamp(dir: &std::path::Path) -> Option<SystemTime> {
    fn walk(p: &std::path::Path, depth: u32, best: &mut Option<SystemTime>) {
        let Ok(rd) = std::fs::read_dir(p) else { return };
        for e in rd.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if depth < 3 && name != "assets" {
                    walk(&path, depth + 1, best);
                }
            } else if let Ok(t) = e.metadata().and_then(|m| m.modified()) {
                if best.map_or(true, |b| t > b) {
                    *best = Some(t);
                }
            }
        }
    }
    let mut best = None;
    walk(dir, 0, &mut best);
    best
}

fn load_project(world: &mut World) {
    let dir = world.resource::<Game>().dir.clone();
    let ents: Vec<Entity> = world.query_filtered::<Entity, With<GameObject>>().iter(world).collect();
    for e in ents {
        world.despawn(e);
    }
    {
        let mut g = world.resource_mut::<Game>();
        g.vars.clear();
        g.time = 0.0;
        g.messages.clear();
        g.next_messages.clear();
        g.hud.clear();
        g.follow = None;
        g.defs.clear();
        g.scripts.clear();
        g.reported.clear();
        g.clicked.clear();
        g.has_scene_camera = false;
    }
    world.resource_mut::<Watch>().stamp = project_stamp(&dir);

    let (project, scene) = match scene::load(&dir) {
        Ok(v) => v,
        Err(e) => {
            world.resource_mut::<Game>().report("", &e);
            return;
        }
    };
    let quality = world.resource::<Args>().quality.clone().unwrap_or(project.quality.clone());
    {
        let mut env = world.resource_mut::<Env>();
        env.bloom = scene.environment.bloom;
        env.fog = scene.environment.fog.clone();
        env.quality = quality;
    }
    if let Some(mut w) = world.query::<&mut Window>().iter_mut(world).next() {
        w.title = format!("{} - ZengEngine", project.name);
    }
    let sky = scene.environment.sky;
    world.insert_resource(ClearColor(Color::srgb(sky[0], sky[1], sky[2])));
    world.insert_resource(GlobalAmbientLight { brightness: scene.environment.ambient, ..default() });
    world.insert_resource(Gravity(Vec3::new(0.0, scene.environment.gravity, 0.0)));

    world.resource_scope(|world, mut game: Mut<Game>| {
        for def in &scene.entities {
            game.defs.insert(def.name.clone(), def.clone());
            if def.camera.is_some() {
                game.has_scene_camera = true;
            }
            spawn_entity(world, &mut game, def, None, false);
        }
    });
    if !world.resource::<Game>().has_scene_camera {
        world.spawn((
            GameObject,
            Camera3d::default(),
            Transform::default(),
            OrbitCamera { focus: Vec3::new(0.0, 0.5, 0.0), yaw: 0.6, pitch: 0.4, distance: 8.0 },
        ));
    }
    println!("ZENGRELOAD");
}

fn hot_reload(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let mut w = world.resource_mut::<Watch>();
    w.since += dt;
    if w.since < 0.5 {
        return;
    }
    w.since = 0.0;
    let known = w.stamp;
    let dir = world.resource::<Game>().dir.clone();
    let now = project_stamp(&dir);
    if now != known {
        load_project(world);
    }
}

fn fake_keys(args: Res<Args>, mut keys: ResMut<ButtonInput<KeyCode>>, mut frame: Local<u32>) {
    *frame += 1;
    for (k, f) in &args.press {
        if *f == *frame {
            if let Some(code) = vm::key_from_name(k) {
                keys.press(code);
            }
        }
    }
}

fn gather_pointer(
    windows: Query<&Window>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    mut p: ResMut<Pointer>,
) {
    if let Ok(w) = windows.single() {
        p.size = Vec2::new(w.width(), w.height());
        p.pos = w.cursor_position();
    }
    p.delta = motion.delta;
    // Mouse wheels report lines, trackpads report pixels.
    p.zoom = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y * 0.5,
        MouseScrollUnit::Pixel => scroll.delta.y * 0.02,
    };
    p.down = buttons.pressed(MouseButton::Left);
    p.just_pressed = buttons.just_pressed(MouseButton::Left);
}

/// Commands from the editor, one per line: `K name 1|0`, `M x y` (0..1), `B 1|0`, `D dx dy`, `S dy`, `Q`.
fn stage_input(
    rx: Res<StageRx>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut p: ResMut<Pointer>,
    mut exit: MessageWriter<AppExit>,
) {
    p.delta = Vec2::ZERO;
    p.zoom = 0.0;
    p.just_pressed = false;
    let rx = rx.0.lock().unwrap();
    while let Ok(line) = rx.try_recv() {
        let mut it = line.split_whitespace();
        let num = |s: Option<&str>| s.and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        match it.next() {
            Some("K") => {
                if let (Some(code), Some(down)) = (it.next().and_then(vm::key_from_name), it.next()) {
                    if down == "1" {
                        keys.press(code);
                    } else {
                        keys.release(code);
                    }
                }
            }
            Some("M") => {
                let (x, y) = (num(it.next()), num(it.next()));
                p.pos = Some(Vec2::new(x * p.size.x, y * p.size.y));
            }
            Some("B") => {
                let d = it.next() == Some("1");
                p.just_pressed = d && !p.down;
                p.down = d;
            }
            Some("D") => p.delta += Vec2::new(num(it.next()), num(it.next())),
            Some("S") => p.zoom += num(it.next()) * 0.02,
            Some("Q") => {
                exit.write(AppExit::Success);
            }
            _ => {}
        }
    }
}

/// Reads the stage image back and writes `ZENGFRAME <len>\n<jpeg bytes>` to stdout, one frame at a time.
fn stage_capture(mut commands: Commands, stage: Res<Stage>, mut warm: Local<u32>) {
    *warm += 1;
    if *warm < 5 || stage.in_flight.load(Ordering::SeqCst) >= 3 {
        return;
    }
    stage.in_flight.fetch_add(1, Ordering::SeqCst);
    let busy = stage.in_flight.clone();
    commands.spawn(Screenshot::image(stage.handle.clone())).observe(move |cap: On<bevy::render::view::screenshot::ScreenshotCaptured>| {
        if let Ok(img) = cap.image.clone().try_into_dynamic() {
            let rgb = img.to_rgb8();
            let mut jpg = Vec::new();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 78);
            if rgb.write_with_encoder(enc).is_ok() {
                let mut out = std::io::stdout().lock();
                let _ = out.write_all(format!("ZENGFRAME {}\n", jpg.len()).as_bytes());
                let _ = out.write_all(&jpg);
                let _ = out.flush();
            }
        }
        busy.fetch_sub(1, Ordering::SeqCst);
    });
}

fn track_mouse(
    p: Res<Pointer>,
    cams: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    objects: Query<(Entity, &GlobalTransform, &Bounds), With<GameObject>>,
    mut game: ResMut<Game>,
) {
    if let Some(pos) = p.pos {
        game.mouse = Vec2::new(pos.x / p.size.x.max(1.0) * 2.0 - 1.0, 1.0 - pos.y / p.size.y.max(1.0) * 2.0);
    }
    game.mouse_down = p.down;
    if !p.just_pressed {
        return;
    }
    let (Some(pos), Some((cam, gt))) = (p.pos, cams.iter().next()) else { return };
    let Ok(ray) = cam.viewport_to_world(gt, pos) else { return };
    // Ray against each object's box (rotation ignored, like touching).
    let mut best: Option<(f32, Entity)> = None;
    for (e, t, b) in &objects {
        let (scale, _, pos) = t.to_scale_rotation_translation();
        let half = b.0 * scale.abs();
        let inv = Vec3::ONE / Vec3::from(ray.direction);
        let t1 = (pos - half - ray.origin) * inv;
        let t2 = (pos + half - ray.origin) * inv;
        let tmin = t1.min(t2).max_element();
        let tmax = t1.max(t2).min_element();
        if tmax >= tmin.max(0.0) && best.is_none_or(|(d, _)| tmin < d) {
            best = Some((tmin, e));
        }
    }
    if let Some((_, e)) = best {
        game.clicked.insert(e);
    }
}

fn run_scripts(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let (pressed, released): (Vec<KeyCode>, Vec<KeyCode>) = {
        let k = world.resource::<ButtonInput<KeyCode>>();
        (k.get_just_pressed().copied().collect(), k.get_just_released().copied().collect())
    };
    world.resource_scope(|world, mut game: Mut<Game>| {
        game.time += dt;
        game.names.clear();
        let all: Vec<(Entity, String, bool)> = {
            let mut q = world.query_filtered::<(Entity, &Name, Has<CloneMarker>), With<GameObject>>();
            q.iter(world).map(|(e, n, c)| (e, n.to_string(), c)).collect()
        };
        for (e, n, c) in &all {
            if !*c || !game.names.contains_key(n) {
                game.names.insert(n.clone(), *e);
            }
        }
        let msgs = std::mem::take(&mut game.messages);
        game.messages = std::mem::take(&mut game.next_messages);
        let clicked = std::mem::take(&mut game.clicked);

        let ents: Vec<Entity> = world.query_filtered::<Entity, With<Scripts>>().iter(world).collect();
        for e in ents {
            let Some(mut sc) = world.get_mut::<Scripts>(e).map(|mut s| std::mem::take(&mut *s)) else { continue };
            let script = sc.script.clone();
            let mut starts: Vec<usize> = vec![];
            for (i, h) in script.hats.iter().enumerate() {
                use blocks::HatKind::*;
                let fire = match &h.kind {
                    Start => !sc.started && !sc.is_clone,
                    Copy => !sc.started && sc.is_clone,
                    Key(k) => vm::key_from_name(k).is_some_and(|c| pressed.contains(&c)),
                    KeyUp(k) => vm::key_from_name(k).is_some_and(|c| released.contains(&c)),
                    Message(m) => msgs.contains(m),
                    Click => clicked.contains(&e),
                    Timer(secs) => {
                        sc.timers[i] += dt;
                        if sc.timers[i] >= *secs {
                            sc.timers[i] -= *secs;
                            true
                        } else {
                            false
                        }
                    }
                    Touch(_) => false,
                };
                if fire {
                    starts.push(i);
                }
            }
            // Touch hats fire when contact begins.
            let mut now_touching: HashSet<Entity> = HashSet::new();
            for (i, h) in script.hats.iter().enumerate() {
                if let blocks::HatKind::Touch(name) = &h.kind {
                    for (o, n, _) in &all {
                        if *o != e && (name == "*" || n == name) && vm::overlaps(world, e, *o) {
                            if !sc.touching.contains(o) {
                                starts.push(i);
                            }
                            now_touching.insert(*o);
                        }
                    }
                }
            }
            sc.touching = now_touching;
            sc.started = true;
            for i in starts {
                sc.threads.retain(|t| t.hat != i);
                sc.threads.push(Thread::new(i, script.hats[i].body.clone()));
            }

            let mut threads = std::mem::take(&mut sc.threads);
            let deleted;
            {
                let mut ctx = Ctx { world, game: &mut game, me: e, script, deleted: false };
                threads.retain_mut(|th| !ctx.deleted && step(&mut ctx, th, false) == Status::Yielded);
                deleted = ctx.deleted;
            }
            if !deleted {
                sc.threads = threads;
                if let Some(mut slot) = world.get_mut::<Scripts>(e) {
                    *slot = sc;
                }
            }
        }
    });
}

fn camera_rig(
    p: Res<Pointer>,
    game: Res<Game>,
    targets: Query<&Transform, Without<Camera3d>>,
    mut cams: Query<(&mut Transform, Option<&mut OrbitCamera>), With<Camera3d>>,
) {
    for (mut tf, orbit) in &mut cams {
        if let Some((name, dist, height)) = &game.follow {
            if let Some(t) = game.names.get(name).and_then(|e| targets.get(*e).ok()) {
                tf.translation = t.translation + Vec3::new(0.0, *height, *dist);
                tf.look_at(t.translation + Vec3::Y * 0.5, Vec3::Y);
                continue;
            }
        }
        let Some(mut cam) = orbit else { continue };
        if p.down {
            cam.yaw -= p.delta.x * 0.005;
            cam.pitch = (cam.pitch + p.delta.y * 0.005).clamp(-1.5, 1.5);
        }
        cam.distance = (cam.distance - p.zoom).clamp(1.0, 50.0);
        let offset = Vec3::new(cam.pitch.cos() * cam.yaw.sin(), cam.pitch.sin(), cam.pitch.cos() * cam.yaw.cos());
        tf.translation = cam.focus + offset * cam.distance;
        tf.look_at(cam.focus, Vec3::Y);
    }
}

/// Quality presets: low = plain, medium = MSAA + optional bloom/fog, high = adds ambient occlusion.
fn camera_effects(mut commands: Commands, env: Res<Env>, stage: Option<Res<Stage>>, cams: Query<Entity, (With<Camera3d>, Without<Fx>)>) {
    for e in &cams {
        let mut c = commands.entity(e);
        c.insert(Fx);
        if let Some(st) = &stage {
            c.insert((RenderTarget::Image(st.handle.clone().into()), IsDefaultUiCamera));
        }
        match env.quality.as_str() {
            "low" => {
                c.insert(Msaa::Off);
            }
            "high" => {
                c.insert((Msaa::Off, bevy::pbr::ScreenSpaceAmbientOcclusion::default()));
                if env.bloom {
                    c.insert(bevy::post_process::bloom::Bloom::NATURAL);
                }
            }
            _ => {
                c.insert(Msaa::Sample4);
                if env.bloom {
                    c.insert(bevy::post_process::bloom::Bloom::NATURAL);
                }
            }
        }
        if let Some(f) = &env.fog {
            c.insert(bevy::pbr::DistanceFog {
                color: Color::srgb(f.color[0], f.color[1], f.color[2]),
                falloff: bevy::pbr::FogFalloff::Linear { start: f.start, end: f.end },
                ..default()
            });
        }
    }
}

fn hud(mut commands: Commands, mut game: ResMut<Game>, mut texts: Query<&mut Text, With<HudText>>, mut made: Local<bool>) {
    let now = game.time;
    game.hud.retain(|_, (_, until)| until.is_none_or(|u| u > now));
    let body = game.hud.values().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join("\n");
    if !*made {
        *made = true;
        commands.spawn((
            HudText,
            Text::new(body),
            TextFont { font_size: 26.0.into(), ..default() },
            TextColor(Color::WHITE),
            Node { position_type: PositionType::Absolute, top: Val::Px(12.0), left: Val::Px(16.0), ..default() },
        ));
    } else if let Ok(mut t) = texts.single_mut() {
        if t.0 != body {
            t.0 = body;
        }
    }
}

fn quit_if_asked(game: Res<Game>, mut exit: MessageWriter<AppExit>) {
    if game.quit {
        exit.write(AppExit::Success);
    }
}

/// Capture the window after the first frames have rendered, then quit. Used for CI and quick checks.
fn auto_screenshot(
    mut commands: Commands,
    args: Res<Args>,
    mut frame: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    *frame += 1;
    if *frame == 150 {
        if let Some(p) = &args.screenshot {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(p.clone()));
        }
    }
    if *frame == 210 {
        exit.write(AppExit::Success);
    }
}
