//! Cooperative script threads: each hat block starts a thread that runs until it waits or ends its loop turn.
use crate::blocks::*;
use crate::game::*;
use crate::value::Value;
use avian3d::prelude::*;
use bevy::prelude::*;
use rand::Rng;
use std::collections::HashMap;
use std::sync::Arc;

pub enum Wait {
    None,
    Until(f64),
    Cond(Expr),
    Glide { t0: f64, dur: f64, from: Vec3, to: Vec3 },
}

pub enum FrameKind {
    Block,
    Forever,
    Repeat(i64),
    RepeatUntil(Expr),
}

pub struct Frame {
    pub body: Body,
    pub idx: usize,
    pub kind: FrameKind,
    pub locals: Option<HashMap<String, Value>>,
}

pub struct Thread {
    pub hat: usize,
    pub stack: Vec<Frame>,
    pub wait: Wait,
}

impl Thread {
    pub fn new(hat: usize, body: Body) -> Thread {
        Thread { hat, stack: vec![Frame { body, idx: 0, kind: FrameKind::Block, locals: None }], wait: Wait::None }
    }
}

pub struct Ctx<'a> {
    pub world: &'a mut World,
    pub game: &'a mut Game,
    pub me: Entity,
    pub script: Arc<Script>,
    pub deleted: bool,
}

#[derive(PartialEq)]
pub enum Status {
    Yielded,
    Finished,
}

const STEP_BUDGET: u32 = 20_000;

fn lookup_local<'a>(stack: &'a [Frame], name: &str) -> Option<&'a Value> {
    stack.iter().rev().find_map(|f| f.locals.as_ref().and_then(|l| l.get(name)))
}

fn set_var(ctx: &mut Ctx, stack: &mut [Frame], name: &str, v: Value) {
    for f in stack.iter_mut().rev() {
        if let Some(l) = f.locals.as_mut() {
            if l.contains_key(name) {
                l.insert(name.to_string(), v);
                return;
            }
        }
    }
    ctx.game.vars.insert(name.to_string(), v);
}

fn get_var(ctx: &Ctx, stack: &[Frame], name: &str) -> Value {
    lookup_local(stack, name).cloned().or_else(|| ctx.game.vars.get(name).cloned()).unwrap_or(Value::Num(0.0))
}

fn key_code(name: &str) -> Option<KeyCode> {
    Some(match name {
        "SPACE" => KeyCode::Space,
        "ENTER" => KeyCode::Enter,
        "SHIFT" => KeyCode::ShiftLeft,
        "CTRL" => KeyCode::ControlLeft,
        "ESC" => KeyCode::Escape,
        "UP" => KeyCode::ArrowUp,
        "DOWN" => KeyCode::ArrowDown,
        "LEFT" => KeyCode::ArrowLeft,
        "RIGHT" => KeyCode::ArrowRight,
        "A" => KeyCode::KeyA, "B" => KeyCode::KeyB, "C" => KeyCode::KeyC, "D" => KeyCode::KeyD,
        "E" => KeyCode::KeyE, "F" => KeyCode::KeyF, "G" => KeyCode::KeyG, "H" => KeyCode::KeyH,
        "I" => KeyCode::KeyI, "J" => KeyCode::KeyJ, "K" => KeyCode::KeyK, "L" => KeyCode::KeyL,
        "M" => KeyCode::KeyM, "N" => KeyCode::KeyN, "O" => KeyCode::KeyO, "P" => KeyCode::KeyP,
        "Q" => KeyCode::KeyQ, "R" => KeyCode::KeyR, "S" => KeyCode::KeyS, "T" => KeyCode::KeyT,
        "U" => KeyCode::KeyU, "V" => KeyCode::KeyV, "W" => KeyCode::KeyW, "X" => KeyCode::KeyX,
        "Y" => KeyCode::KeyY, "Z" => KeyCode::KeyZ,
        "0" => KeyCode::Digit0, "1" => KeyCode::Digit1, "2" => KeyCode::Digit2, "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4, "5" => KeyCode::Digit5, "6" => KeyCode::Digit6, "7" => KeyCode::Digit7,
        "8" => KeyCode::Digit8, "9" => KeyCode::Digit9,
        _ => return None,
    })
}

pub fn key_from_name(name: &str) -> Option<KeyCode> {
    key_code(name)
}

fn axis_vec(axis: &str, n: f32) -> Vec3 {
    match axis {
        "X" => Vec3::new(n, 0.0, 0.0),
        "Y" => Vec3::new(0.0, n, 0.0),
        _ => Vec3::new(0.0, 0.0, n),
    }
}

fn find_named(ctx: &Ctx, name: &str) -> Option<Entity> {
    ctx.game.names.get(name).copied()
}

fn position_of(ctx: &Ctx, e: Entity) -> Option<Vec3> {
    ctx.world.get::<Transform>(e).map(|t| t.translation)
}

pub fn overlaps(world: &World, a: Entity, b: Entity) -> bool {
    let (Some(ta), Some(tb)) = (world.get::<Transform>(a), world.get::<Transform>(b)) else { return false };
    let (Some(ba), Some(bb)) = (world.get::<Bounds>(a), world.get::<Bounds>(b)) else { return false };
    let ha = ba.0 * ta.scale.abs();
    let hb = bb.0 * tb.scale.abs();
    let d = (ta.translation - tb.translation).abs();
    d.x <= ha.x + hb.x && d.y <= ha.y + hb.y && d.z <= ha.z + hb.z
}

fn touching_named(ctx: &mut Ctx, name: &str) -> bool {
    let others: Vec<Entity> = {
        let mut q = ctx.world.query_filtered::<(Entity, &Name), With<GameObject>>();
        q.iter(ctx.world).filter(|(e, n)| n.as_str() == name && *e != ctx.me).map(|(e, _)| e).collect()
    };
    others.into_iter().any(|o| overlaps(ctx.world, ctx.me, o))
}

fn axis_of(t: Vec3, axis: &str) -> f64 {
    (match axis {
        "X" => t.x,
        "Y" => t.y,
        _ => t.z,
    }) as f64
}

pub fn eval(ctx: &mut Ctx, e: &Expr, stack: &[Frame]) -> Value {
    match e {
        Expr::Lit(v) => v.clone(),
        Expr::Var(n) => get_var(ctx, stack, n),
        Expr::Arith(op, a, b) => {
            let (x, y) = (eval(ctx, a, stack), eval(ctx, b, stack));
            let (x, y) = (x.num(), y.num());
            Value::Num(match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => {
                    if y == 0.0 { 0.0 } else { x / y }
                }
                _ => {
                    if y == 0.0 { 0.0 } else { x.rem_euclid(y) }
                }
            })
        }
        Expr::Cmp(op, a, b) => {
            let (x, y) = (eval(ctx, a, stack), eval(ctx, b, stack));
            let numeric = !matches!((&x, &y), (Value::Str(_), Value::Str(_)));
            let ord = if numeric {
                x.num().partial_cmp(&y.num())
            } else {
                Some(x.to_string().cmp(&y.to_string()))
            };
            use std::cmp::Ordering::*;
            Value::Bool(match (op.as_str(), ord) {
                ("EQ", Some(Equal)) | ("NEQ", Some(Less | Greater)) | ("LT", Some(Less)) | ("GT", Some(Greater)) => true,
                ("LTE", Some(Less | Equal)) | ("GTE", Some(Greater | Equal)) => true,
                _ => false,
            })
        }
        Expr::And(a, b) => Value::Bool(eval(ctx, a, stack).truthy() && eval(ctx, b, stack).truthy()),
        Expr::Or(a, b) => Value::Bool(eval(ctx, a, stack).truthy() || eval(ctx, b, stack).truthy()),
        Expr::Not(a) => Value::Bool(!eval(ctx, a, stack).truthy()),
        Expr::Rand(a, b) => {
            let (x, y) = (eval(ctx, a, stack).num(), eval(ctx, b, stack).num());
            let (lo, hi) = if x <= y { (x, y) } else { (y, x) };
            if lo.fract() == 0.0 && hi.fract() == 0.0 {
                Value::Num(rand::rng().random_range(lo as i64..=hi as i64) as f64)
            } else {
                Value::Num(rand::rng().random_range(lo..=hi))
            }
        }
        Expr::Fn(f, a) => {
            let x = eval(ctx, a, stack).num();
            Value::Num(match f.as_str() {
                "ABS" => x.abs(),
                "SQRT" => x.max(0.0).sqrt(),
                "SIN" => x.to_radians().sin(),
                "COS" => x.to_radians().cos(),
                "ROUND" => x.round(),
                "FLOOR" => x.floor(),
                "CEIL" => x.ceil(),
                _ => x,
            })
        }
        Expr::Join(a, b) => {
            let (x, y) = (eval(ctx, a, stack), eval(ctx, b, stack));
            Value::Str(format!("{x}{y}"))
        }
        Expr::Key(k) => {
            let Some(code) = key_code(k) else { return Value::Bool(false) };
            Value::Bool(ctx.world.resource::<ButtonInput<KeyCode>>().pressed(code))
        }
        Expr::Pos(axis) => Value::Num(position_of(ctx, ctx.me).map_or(0.0, |t| axis_of(t, axis))),
        Expr::PosOf(axis, obj) => {
            Value::Num(find_named(ctx, obj).and_then(|e| position_of(ctx, e)).map_or(0.0, |t| axis_of(t, axis)))
        }
        Expr::Dist(obj) => {
            let me = position_of(ctx, ctx.me);
            let other = find_named(ctx, obj).and_then(|e| position_of(ctx, e));
            Value::Num(match (me, other) {
                (Some(a), Some(b)) => a.distance(b) as f64,
                _ => 9999.0,
            })
        }
        Expr::Touching(obj) => Value::Bool(touching_named(ctx, obj)),
        Expr::Timer => Value::Num(ctx.game.time),
        Expr::Mouse(what) => Value::Num(match what.as_str() {
            "X" => ctx.game.mouse.x as f64,
            "Y" => ctx.game.mouse.y as f64,
            _ => ctx.game.mouse_down as i32 as f64,
        }),
        Expr::ListGet(n, i) => {
            let i = eval(ctx, i, stack).num() as usize;
            match get_var(ctx, stack, n) {
                Value::List(l) if i >= 1 && i <= l.len() => l[i - 1].clone(),
                _ => Value::Str(String::new()),
            }
        }
        Expr::ListLen(n) => match get_var(ctx, stack, n) {
            Value::List(l) => Value::Num(l.len() as f64),
            _ => Value::Num(0.0),
        },
        Expr::ListHas(n, item) => {
            let item = eval(ctx, item, stack);
            Value::Bool(matches!(get_var(ctx, stack, n), Value::List(l) if l.contains(&item)))
        }
        Expr::Call(name, args) => {
            let script = ctx.script.clone();
            let Some(p) = script.procs.get(name) else {
                ctx.game.report("", &format!("There is no function called \"{name}\""));
                return Value::Num(0.0);
            };
            let mut locals = HashMap::new();
            for (pn, a) in p.params.iter().zip(args) {
                locals.insert(pn.clone(), eval(ctx, a, stack));
            }
            let mut th = Thread {
                hat: 0,
                stack: vec![Frame { body: p.body.clone(), idx: 0, kind: FrameKind::Block, locals: Some(locals.clone()) }],
                wait: Wait::None,
            };
            let mut n = 0;
            while n < 200 && step(ctx, &mut th, true) == Status::Yielded {
                n += 1;
            }
            match &p.ret {
                Some(r) => {
                    let fake = [Frame { body: Arc::new(vec![]), idx: 0, kind: FrameKind::Block, locals: Some(locals) }];
                    eval(ctx, r, &fake)
                }
                None => Value::Num(0.0),
            }
        }
    }
}

fn tf_mut<'a>(ctx: &'a mut Ctx) -> Option<Mut<'a, Transform>> {
    ctx.world.get_mut::<Transform>(ctx.me)
}

/// Runs a thread until it waits, ends a loop turn, or finishes. `sync` ignores waits (used for function calls).
pub fn step(ctx: &mut Ctx, th: &mut Thread, sync: bool) -> Status {
    match &th.wait {
        Wait::None => {}
        Wait::Until(t) => {
            if ctx.game.time < *t && !sync {
                return Status::Yielded;
            }
            th.wait = Wait::None;
        }
        Wait::Cond(c) => {
            let c = c.clone();
            if !sync && !eval(ctx, &c, &th.stack).truthy() {
                return Status::Yielded;
            }
            th.wait = Wait::None;
        }
        Wait::Glide { t0, dur, from, to } => {
            let (t0, dur, from, to) = (*t0, *dur, *from, *to);
            let p = if dur <= 0.0 { 1.0 } else { ((ctx.game.time - t0) / dur).clamp(0.0, 1.0) as f32 };
            if let Some(mut t) = tf_mut(ctx) {
                t.translation = from.lerp(to, p);
            }
            if p < 1.0 && !sync {
                return Status::Yielded;
            }
            th.wait = Wait::None;
        }
    }

    let mut budget = STEP_BUDGET;
    loop {
        if budget == 0 {
            ctx.game.report("", "A loop is running without ever waiting. Add a \"wait\" block inside it.");
            return Status::Yielded;
        }
        budget -= 1;
        let Some(top) = th.stack.last_mut() else { return Status::Finished };
        if top.idx >= top.body.len() {
            match &mut top.kind {
                FrameKind::Block => {
                    th.stack.pop();
                    continue;
                }
                FrameKind::Forever => {
                    top.idx = 0;
                    return Status::Yielded;
                }
                FrameKind::Repeat(n) => {
                    *n -= 1;
                    if *n > 0 {
                        top.idx = 0;
                        return Status::Yielded;
                    }
                    th.stack.pop();
                    continue;
                }
                FrameKind::RepeatUntil(c) => {
                    let c = c.clone();
                    if eval(ctx, &c, &th.stack).truthy() {
                        th.stack.pop();
                        continue;
                    }
                    let top = th.stack.last_mut().unwrap();
                    top.idx = 0;
                    return Status::Yielded;
                }
            }
        }
        let body = top.body.clone();
        let st = &body[top.idx];
        top.idx += 1;
        let r = exec(ctx, th, st);
        if let Some(status) = r {
            return status;
        }
        if ctx.deleted {
            return Status::Finished;
        }
    }
}

fn push_block(th: &mut Thread, body: &Body, kind: FrameKind) {
    if !body.is_empty() {
        th.stack.push(Frame { body: body.clone(), idx: 0, kind, locals: None });
    }
}

fn exec(ctx: &mut Ctx, th: &mut Thread, st: &Stmt) -> Option<Status> {
    macro_rules! num {
        ($e:expr) => {
            eval(ctx, $e, &th.stack).num()
        };
    }
    match &st.kind {
        Kind::Wait(e) => {
            let secs = num!(e).max(0.0);
            th.wait = Wait::Until(ctx.game.time + secs);
            return Some(Status::Yielded);
        }
        Kind::Forever(b) => {
            if b.is_empty() {
                ctx.game.report(&st.id, "This \"forever\" has nothing inside it.");
                return Some(Status::Finished);
            }
            push_block(th, b, FrameKind::Forever);
        }
        Kind::Repeat(n, b) => {
            let n = num!(n) as i64;
            if n > 0 {
                push_block(th, b, FrameKind::Repeat(n));
            }
        }
        Kind::If(c, a, b) => {
            let branch = if eval(ctx, c, &th.stack).truthy() { a } else { b };
            push_block(th, branch, FrameKind::Block);
        }
        Kind::WaitUntil(c) => {
            if !eval(ctx, c, &th.stack).truthy() {
                th.wait = Wait::Cond(c.clone());
                return Some(Status::Yielded);
            }
        }
        Kind::RepeatUntil(c, b) => {
            if !eval(ctx, c, &th.stack).truthy() {
                push_block(th, b, FrameKind::RepeatUntil(c.clone()));
            }
        }
        Kind::Broadcast(m) => ctx.game.next_messages.push(m.clone()),
        Kind::DeleteMe => {
            ctx.world.despawn(ctx.me);
            ctx.deleted = true;
            return Some(Status::Finished);
        }
        Kind::CloneObj(name) => {
            let (def, at) = if name == "_ME_" {
                let n = ctx.world.get::<Name>(ctx.me).map(|n| n.to_string()).unwrap_or_default();
                (ctx.game.defs.get(&n).cloned(), ctx.world.get::<Transform>(ctx.me).copied())
            } else {
                let at = find_named(ctx, name).and_then(|e| ctx.world.get::<Transform>(e).copied());
                (ctx.game.defs.get(name).cloned(), at)
            };
            match def {
                Some(d) => {
                    spawn_entity(ctx.world, ctx.game, &d, at, true);
                }
                None => ctx.game.report(&st.id, &format!("There is no object called \"{name}\" to copy")),
            }
        }
        Kind::StopAll => {
            ctx.game.quit = true;
            return Some(Status::Finished);
        }
        Kind::Move(dir, n) => {
            let n = num!(n) as f32;
            if let Some(mut t) = tf_mut(ctx) {
                let v = match dir.as_str() {
                    "FORWARD" => t.forward() * n,
                    "BACK" => t.back() * n,
                    "LEFT" => t.left() * n,
                    "RIGHT" => t.right() * n,
                    "UP" => Vec3::Y * n,
                    _ => Vec3::NEG_Y * n,
                };
                t.translation += v;
            }
        }
        Kind::Change(axis, n) => {
            let v = axis_vec(axis, num!(n) as f32);
            if let Some(mut t) = tf_mut(ctx) {
                t.translation += v;
            }
        }
        Kind::Set(axis, n) => {
            let n = num!(n) as f32;
            if let Some(mut t) = tf_mut(ctx) {
                match axis.as_str() {
                    "X" => t.translation.x = n,
                    "Y" => t.translation.y = n,
                    _ => t.translation.z = n,
                }
            }
        }
        Kind::Goto(x, y, z) => {
            let v = Vec3::new(num!(x) as f32, num!(y) as f32, num!(z) as f32);
            if let Some(mut t) = tf_mut(ctx) {
                t.translation = v;
            }
        }
        Kind::Turn(dir, d) => {
            let a = (num!(d) as f32).to_radians();
            if let Some(mut t) = tf_mut(ctx) {
                match dir.as_str() {
                    "LEFT" => t.rotate_y(a),
                    "RIGHT" => t.rotate_y(-a),
                    "UP" => t.rotate_local_x(a),
                    "DOWN" => t.rotate_local_x(-a),
                    "ROLL_LEFT" => t.rotate_local_z(a),
                    _ => t.rotate_local_z(-a),
                }
            }
        }
        Kind::LookAt(name) => match find_named(ctx, name).and_then(|e| position_of(ctx, e)) {
            Some(target) => {
                if let Some(mut t) = tf_mut(ctx) {
                    t.look_at(target, Vec3::Y);
                }
            }
            None => ctx.game.report(&st.id, &format!("I can't find an object called \"{name}\"")),
        },
        Kind::Size(p) => {
            let p = (num!(p) as f32 / 100.0).max(0.001);
            let base = ctx.world.get::<BaseScale>(ctx.me).map_or(Vec3::ONE, |b| b.0);
            if let Some(mut t) = tf_mut(ctx) {
                t.scale = base * p;
            }
        }
        Kind::Glide(s, x, y, z) => {
            let dur = num!(s).max(0.0);
            let to = Vec3::new(num!(x) as f32, num!(y) as f32, num!(z) as f32);
            if let Some(from) = position_of(ctx, ctx.me) {
                th.wait = Wait::Glide { t0: ctx.game.time, dur, from, to };
                return Some(Status::Yielded);
            }
        }
        Kind::Push(x, y, z) => {
            let v = Vec3::new(num!(x) as f32, num!(y) as f32, num!(z) as f32);
            if let Some(mut lv) = ctx.world.get_mut::<LinearVelocity>(ctx.me) {
                lv.0 += v;
            } else {
                ctx.game.report(&st.id, "Only objects with physics can be pushed. Use \"make this object ... (moves with physics)\" first.");
            }
        }
        Kind::Velocity(x, y, z) => {
            let v = Vec3::new(num!(x) as f32, num!(y) as f32, num!(z) as f32);
            if let Some(mut lv) = ctx.world.get_mut::<LinearVelocity>(ctx.me) {
                lv.0 = v;
            } else {
                ctx.game.report(&st.id, "Only objects with physics have a speed. Use \"make this object ... (moves with physics)\" first.");
            }
        }
        Kind::Color(hex) => match parse_color(hex) {
            Some(c) => {
                let handle = ctx.world.get::<MeshMaterial3d<StandardMaterial>>(ctx.me).map(|m| m.0.clone());
                if let Some(h) = handle {
                    if let Some(mut m) = ctx.world.resource_mut::<Assets<StandardMaterial>>().get_mut(&h) {
                        m.base_color = c;
                    }
                }
            }
            None => ctx.game.report(&st.id, "Pick a colour for this block."),
        },
        Kind::Show(v) => {
            if let Some(mut vis) = ctx.world.get_mut::<Visibility>(ctx.me) {
                *vis = if *v { Visibility::Inherited } else { Visibility::Hidden };
            }
        }
        Kind::Say(text, secs) => {
            let text = eval(ctx, text, &th.stack).to_string();
            let secs = num!(secs);
            let me = ctx.world.get::<Name>(ctx.me).map(|n| n.to_string()).unwrap_or_default();
            let until = if secs > 0.0 { Some(ctx.game.time + secs) } else { None };
            ctx.game.hud.insert(format!("1 {me}"), (format!("{me}: {text}"), until));
        }
        Kind::Hud(label, value) => {
            let label = eval(ctx, label, &th.stack).to_string();
            let value = eval(ctx, value, &th.stack).to_string();
            ctx.game.hud.insert(format!("0 {label}"), (format!("{label}: {value}"), None));
        }
        Kind::Brightness(p) => {
            let p = num!(p) as f32 / 100.0;
            let base = ctx.world.get::<LightBase>(ctx.me).map(|b| b.0);
            if let Some(base) = base {
                if let Some(mut l) = ctx.world.get_mut::<DirectionalLight>(ctx.me) {
                    l.illuminance = base * p;
                }
                if let Some(mut l) = ctx.world.get_mut::<PointLight>(ctx.me) {
                    l.intensity = base * p;
                }
                if let Some(mut l) = ctx.world.get_mut::<SpotLight>(ctx.me) {
                    l.intensity = base * p;
                }
            } else {
                ctx.game.report(&st.id, "This object is not a light.");
            }
        }
        Kind::CamFollow(name, d, h) => {
            let (d, h) = (num!(d) as f32, num!(h) as f32);
            ctx.game.follow = if name.is_empty() { None } else { Some((name.clone(), d, h)) };
        }
        Kind::Sky(hex) => {
            if let Some(c) = parse_color(hex) {
                ctx.world.insert_resource(ClearColor(c));
            }
        }
        Kind::Sound(file) => {
            if file.is_empty() {
                ctx.game.report(&st.id, "Pick a sound for this block.");
            } else {
                let h: Handle<AudioSource> = ctx.world.resource::<AssetServer>().load(format!("assets/{file}"));
                ctx.world.spawn((AudioPlayer::new(h), PlaybackSettings::DESPAWN));
            }
        }
        Kind::PhysBody(ty) => {
            let mut e = ctx.world.entity_mut(ctx.me);
            match ty.as_str() {
                "dynamic" => {
                    e.insert((RigidBody::Dynamic, Restitution::new(0.3)));
                }
                "static" => {
                    e.insert(RigidBody::Static);
                }
                "kinematic" => {
                    e.insert(RigidBody::Kinematic);
                }
                _ => {
                    e.remove::<RigidBody>();
                }
            }
            if e.get::<Collider>().is_none() {
                ctx.game.report(&st.id, "This object has no shape to collide with.");
            }
        }
        Kind::Gravity(n) => {
            let g = num!(n) as f32;
            ctx.world.insert_resource(Gravity(Vec3::new(0.0, g, 0.0)));
        }
        Kind::Bounce(n) => {
            let b = num!(n) as f32;
            ctx.world.entity_mut(ctx.me).insert(Restitution::new(b.clamp(0.0, 1.0)));
        }
        Kind::SetVar(n, e) => {
            let v = eval(ctx, e, &th.stack);
            set_var(ctx, &mut th.stack, n, v);
        }
        Kind::ChangeVar(n, e) => {
            let d = num!(e);
            let cur = get_var(ctx, &th.stack, n).num();
            set_var(ctx, &mut th.stack, n, Value::Num(cur + d));
        }
        Kind::ListAdd(n, e) => {
            let v = eval(ctx, e, &th.stack);
            let mut l = match get_var(ctx, &th.stack, n) {
                Value::List(l) => l,
                _ => vec![],
            };
            l.push(v);
            set_var(ctx, &mut th.stack, n, Value::List(l));
        }
        Kind::ListRemove(n, e) => {
            let i = num!(e) as usize;
            if let Value::List(mut l) = get_var(ctx, &th.stack, n) {
                if i >= 1 && i <= l.len() {
                    l.remove(i - 1);
                }
                set_var(ctx, &mut th.stack, n, Value::List(l));
            }
        }
        Kind::ListClear(n) => set_var(ctx, &mut th.stack, n, Value::List(vec![])),
        Kind::Call(name, args) => {
            let script = ctx.script.clone();
            match script.procs.get(name) {
                None => ctx.game.report(&st.id, &format!("There is no function called \"{name}\"")),
                Some(p) => {
                    if th.stack.len() > 200 {
                        ctx.game.report(&st.id, "This function calls itself too many times.");
                        return Some(Status::Finished);
                    }
                    let mut locals = HashMap::new();
                    for (pn, a) in p.params.iter().zip(args) {
                        locals.insert(pn.clone(), eval(ctx, a, &th.stack));
                    }
                    th.stack.push(Frame { body: p.body.clone(), idx: 0, kind: FrameKind::Block, locals: Some(locals) });
                }
            }
        }
    }
    None
}
