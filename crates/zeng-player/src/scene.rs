use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub version: String,
    pub main_scene: String,
    #[serde(default = "default_quality")]
    pub quality: String,
}

fn default_quality() -> String {
    "medium".into()
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Scene {
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub entities: Vec<EntityDef>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Environment {
    #[serde(default = "sky_default")]
    pub sky: [f32; 3],
    #[serde(default = "ambient_default")]
    pub ambient: f32,
    #[serde(default = "gravity_default")]
    pub gravity: f32,
    pub fog: Option<FogDef>,
    #[serde(default)]
    pub bloom: bool,
}

impl Default for Environment {
    fn default() -> Self {
        Environment { sky: sky_default(), ambient: ambient_default(), gravity: gravity_default(), fog: None, bloom: false }
    }
}

fn sky_default() -> [f32; 3] {
    [0.16, 0.17, 0.19]
}
fn ambient_default() -> f32 {
    300.0
}
fn gravity_default() -> f32 {
    -9.81
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FogDef {
    pub color: [f32; 3],
    pub start: f32,
    pub end: f32,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct TransformDef {
    #[serde(default)]
    pub position: [f32; 3],
    /// Degrees, applied as yaw (Y), pitch (X), roll (Z).
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default = "one3")]
    pub scale: [f32; 3],
    pub look_at: Option<[f32; 3]>,
}

impl Default for TransformDef {
    fn default() -> Self {
        TransformDef { position: [0.0; 3], rotation: [0.0; 3], scale: [1.0; 3], look_at: None }
    }
}

fn one3() -> [f32; 3] {
    [1.0; 3]
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ShapeDef {
    Cube { size: f32 },
    Sphere { radius: f32 },
    Cylinder { radius: f32, height: f32 },
    Capsule { radius: f32, length: f32 },
    Plane { size: f32 },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MaterialDef {
    #[serde(default = "white")]
    pub color: [f32; 3],
    #[serde(default)]
    pub metallic: f32,
    #[serde(default = "half")]
    pub roughness: f32,
    #[serde(default)]
    pub emissive: [f32; 3],
    pub texture: Option<String>,
}

fn white() -> [f32; 3] {
    [1.0; 3]
}
fn half() -> f32 {
    0.5
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LightDef {
    Directional { illuminance: f32, #[serde(default)] shadows: bool },
    Point { intensity: f32, #[serde(default)] shadows: bool },
    Spot { intensity: f32, #[serde(default = "spot_angle")] angle: f32, #[serde(default)] shadows: bool },
}

fn spot_angle() -> f32 {
    0.8
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CameraDef {
    #[serde(default = "fov_default")]
    pub fov: f32,
}

fn fov_default() -> f32 {
    60.0
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PhysicsDef {
    /// "dynamic", "static" or "kinematic"
    pub body: String,
    #[serde(default = "half")]
    pub bounce: f32,
    #[serde(default = "half")]
    pub friction: f32,
    /// Collider box size for models (width, height, depth); shapes derive their own.
    pub box_size: Option<[f32; 3]>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct EntityDef {
    pub name: String,
    #[serde(default)]
    pub transform: TransformDef,
    pub shape: Option<ShapeDef>,
    pub material: Option<MaterialDef>,
    pub model: Option<String>,
    pub light: Option<LightDef>,
    pub camera: Option<CameraDef>,
    pub physics: Option<PhysicsDef>,
    pub script: Option<String>,
}

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn load(project_dir: &Path) -> Result<(Project, Scene), String> {
    let project: Project =
        serde_json::from_str(&read(&project_dir.join("project.json"))?).map_err(|e| format!("project.json: {e}"))?;
    let scene_path = project_dir.join(&project.main_scene);
    let scene: Scene =
        serde_json::from_str(&read(&scene_path)?).map_err(|e| format!("{}: {e}", scene_path.display()))?;
    Ok((project, scene))
}
