# Project format

A game is a plain folder. Everything is text, so it works with git.

```
MyGame/
  project.json                 name, version, main_scene, quality (low | medium | high)
  scenes/main.scene.json       environment and objects
  scripts/<Object>.blocks.json one Blockly workspace per object
  assets/models|textures|sounds
```

## scene.json
```json
{
  "environment": { "sky": [r,g,b], "ambient": 500, "gravity": -9.81, "bloom": true,
                   "fog": { "color": [r,g,b], "start": 15, "end": 60 } },
  "entities": [ {
    "name": "Cube",
    "transform": { "position": [x,y,z], "rotation": [yaw,pitch,roll], "scale": [x,y,z], "look_at": [x,y,z] },
    "shape": { "type": "cube", "size": 1 },
    "material": { "color": [r,g,b], "metallic": 0, "roughness": 0.5, "emissive": [r,g,b], "texture": "textures/x.png" },
    "model": "models/x.glb",
    "light": { "type": "directional|point|spot", "...": "..." },
    "camera": { "fov": 60 },
    "physics": { "body": "dynamic|static|kinematic", "bounce": 0.5, "friction": 0.5, "box_size": [w,h,d] },
    "script": "scripts/Cube.blocks.json"
  } ]
}
```
Colours are 0 to 1. Rotation is degrees. Shapes: `cube{size}`, `sphere{radius}`, `cylinder{radius,height}`, `capsule{radius,length}`, `plane{size}`.

## Running without the editor
`zeng-player <folder> [--quality low|medium|high] [--screenshot out.png]`. The player watches the folder and reloads when files change. It prints `ZENGERR<TAB>blockId<TAB>message` lines on stdout for script problems.
