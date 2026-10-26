//! Shared game state, components, and spawning from scene definitions.
use crate::blocks::{self, Script};
use crate::scene::*;
use avian3d::prelude::*;
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use crate::value::Value;
use crate::vm::Thread;

#[derive(Resource, Default)]
pub struct Game {
    pub dir: PathBuf,
    pub vars: HashMap<String, Value>,
    pub time: f64,
    pub messages: Vec<String>,
    pub next_messages: Vec<String>,
    pub hud: BTreeMap<String, (String, Option<f64>)>,
    pub follow: Option<(String, f32, f32)>,
    pub names: HashMap<String, Entity>,
    pub defs: HashMap<String, EntityDef>,
    pub scripts: HashMap<String, Arc<Script>>,
    pub quit: bool,
    pub reported: HashSet<String>,
    pub mouse: Vec2,
    pub mouse_down: bool,
    pub clicked: HashSet<Entity>,
    pub has_scene_camera: bool,
}

impl Game {
    /// Errors go to stdout as `ZENGERR<TAB>block id<TAB>message` so the editor can highlight the block.
    pub fn report(&mut self, block_id: &str, msg: &str) {
        let key = format!("{block_id}|{msg}");
        if self.reported.insert(key) {
            println!("ZENGERR\t{block_id}\t{msg}");
            eprintln!("script problem: {msg}");
        }
    }
}

#[derive(Component)]
pub struct GameObject;

#[derive(Component)]
pub struct SceneCamera;

#[derive(Component)]
pub struct CloneMarker;

#[derive(Component)]
pub struct BaseScale(pub Vec3);

#[derive(Component)]
pub struct LightBase(pub f32);

#[derive(Component)]
pub struct Bounds(pub Vec3);

#[derive(Component, Default)]
pub struct Scripts {
    pub script: Arc<Script>,
    pub threads: Vec<Thread>,
    pub timers: Vec<f64>,
    pub started: bool,
    pub is_clone: bool,
    pub touching: HashSet<Entity>,
}

pub fn parse_color(s: &str) -> Option<Color> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(Color::srgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn rgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

pub fn transform_from(t: &TransformDef) -> Transform {
    let mut tf = Transform::from_translation(Vec3::from(t.position))
        .with_rotation(Quat::from_euler(
            EulerRot::YXZ,
            t.rotation[0].to_radians(),
            t.rotation[1].to_radians(),
            t.rotation[2].to_radians(),
        ))
        .with_scale(Vec3::from(t.scale));
    if let Some(target) = t.look_at {
        tf.look_at(Vec3::from(target), Vec3::Y);
    }
    tf
}

pub fn load_script(game: &mut Game, path: &str) -> Option<Arc<Script>> {
    if let Some(s) = game.scripts.get(path) {
        return Some(s.clone());
    }
    let dir = game.dir.clone();
    let src = match read(&dir.join(path)) {
        Ok(s) => s,
        Err(e) => {
            game.report("", &e);
            return None;
        }
    };
    match blocks::compile(&src) {
        Ok((script, errors)) => {
            for (id, msg) in errors {
                game.report(&id, &msg);
            }
            let arc = Arc::new(script);
            game.scripts.insert(path.to_string(), arc.clone());
            Some(arc)
        }
        Err(e) => {
            game.report("", &format!("{path}: {e}"));
            None
        }
    }
}

/// Spawns one scene entity. `at` overrides the transform (used for copies).
pub fn spawn_entity(world: &mut World, game: &mut Game, def: &EntityDef, at: Option<Transform>, is_clone: bool) -> Entity {
    let transform = at.unwrap_or_else(|| transform_from(&def.transform));
    let mut ent = world.spawn((GameObject, Name::new(def.name.clone()), transform, BaseScale(transform.scale), Visibility::default()));
    let id = ent.id();
    if is_clone {
        ent.insert(CloneMarker);
    }

    let mut half = Vec3::splat(0.5);
    let mut collider: Option<Collider> = None;
    let mut mesh_handle: Option<Handle<Mesh>> = None;

    if let Some(shape) = &def.shape {
        let (mesh, col, h): (Mesh, Collider, Vec3) = match shape {
            ShapeDef::Cube { size } => (Cuboid::from_length(*size).into(), Collider::cuboid(*size, *size, *size), Vec3::splat(size / 2.0)),
            ShapeDef::Sphere { radius } => (Sphere::new(*radius).into(), Collider::sphere(*radius), Vec3::splat(*radius)),
            ShapeDef::Cylinder { radius, height } => (
                Cylinder::new(*radius, *height).into(),
                Collider::cylinder(*radius, *height),
                Vec3::new(*radius, height / 2.0, *radius),
            ),
            ShapeDef::Capsule { radius, length } => (
                Capsule3d::new(*radius, *length).into(),
                Collider::capsule(*radius, *length),
                Vec3::new(*radius, length / 2.0 + radius, *radius),
            ),
            ShapeDef::Plane { size } => (
                Plane3d::new(Vec3::Y, Vec2::splat(size / 2.0)).into(),
                Collider::cuboid(*size, 0.02, *size),
                Vec3::new(size / 2.0, 0.01, size / 2.0),
            ),
        };
        half = h;
        collider = Some(col);
        mesh_handle = Some(world.resource_mut::<Assets<Mesh>>().add(mesh));
    }
    if let Some(p) = def.physics.as_ref().and_then(|p| p.box_size) {
        half = Vec3::from(p) / 2.0;
        collider = Some(Collider::cuboid(p[0], p[1], p[2]));
    }

    if let Some(mesh) = mesh_handle {
        let m = def.material.clone().unwrap_or(MaterialDef {
            color: [1.0; 3],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0; 3],
            texture: None,
        });
        let tex = m.texture.as_ref().map(|t| world.resource::<AssetServer>().load::<Image>(format!("assets/{t}")));
        let mat = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial {
            base_color: rgb(m.color),
            base_color_texture: tex,
            metallic: m.metallic,
            perceptual_roughness: m.roughness,
            emissive: LinearRgba::rgb(m.emissive[0], m.emissive[1], m.emissive[2]),
            ..default()
        });
        world.entity_mut(id).insert((Mesh3d(mesh), MeshMaterial3d(mat)));
    }

    if let Some(path) = &def.model {
        let handle: Handle<WorldAsset> =
            world.resource::<AssetServer>().load(GltfAssetLabel::Scene(0).from_asset(format!("assets/{path}")));
        world.entity_mut(id).insert(WorldAssetRoot(handle));
    }

    match &def.light {
        Some(LightDef::Directional { illuminance, shadows }) => {
            world.entity_mut(id).insert((
                DirectionalLight { illuminance: *illuminance, shadow_maps_enabled: *shadows, ..default() },
                LightBase(*illuminance),
            ));
        }
        Some(LightDef::Point { intensity, shadows }) => {
            world.entity_mut(id).insert((
                PointLight { intensity: *intensity, shadow_maps_enabled: *shadows, range: 40.0, ..default() },
                LightBase(*intensity),
            ));
        }
        Some(LightDef::Spot { intensity, angle, shadows }) => {
            world.entity_mut(id).insert((
                SpotLight { intensity: *intensity, outer_angle: *angle, shadow_maps_enabled: *shadows, range: 60.0, ..default() },
                LightBase(*intensity),
            ));
        }
        None => {}
    }

    if let Some(cam) = &def.camera {
        world.entity_mut(id).insert((SceneCamera, Camera3d::default(), Projection::Perspective(PerspectiveProjection {
            fov: cam.fov.to_radians(),
            ..default()
        })));
    }

    world.entity_mut(id).insert(Bounds(half));

    if let Some(p) = &def.physics {
        let mut e = world.entity_mut(id);
        if let Some(c) = collider {
            e.insert(c);
        }
        e.insert((
            match p.body.as_str() {
                "dynamic" => RigidBody::Dynamic,
                "kinematic" => RigidBody::Kinematic,
                _ => RigidBody::Static,
            },
            Restitution::new(p.bounce),
            Friction::new(p.friction),
        ));
    }

    if let Some(path) = &def.script {
        if let Some(script) = load_script(game, path) {
            let timers = vec![0.0; script.hats.len()];
            world.entity_mut(id).insert(Scripts { script, timers, is_clone, ..default() });
        }
    }
    id
}

#[derive(Component)]
pub struct HudText;
