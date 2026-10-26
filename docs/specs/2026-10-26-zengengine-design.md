# ZengEngine design

A desktop game engine you program with Scratch-style blocks, but backed by a real 3D renderer, physics and audio. Runs locally, free to use and free to host (no servers, no accounts).

## Goals
- Make real 3D games with good graphics: PBR materials, shadows, glTF models, post-processing, physics, sound.
- Visual block scripting that kids can start with, with a path to depth (functions, lists, custom blocks, and a "view as code" panel).
- One download per OS (macOS, Windows, Linux). Export a finished game as a standalone app.
- Instant feedback: press Play, the game window opens; edits hot-reload.

## Non-goals (for now)
VR, consoles, mobile export, multiplayer networking, terrain sculpting, animation state graphs, a marketplace. Revisit after M5.

## Architecture
Two programs sharing one project format:
1. **Editor** (Rust, `tao` + `wry` like ZengSearch): scene tree, inspector, asset browser, and the block editor built on Google Blockly (the library Scratch's blocks come from; Apache-2.0), all in a webview. Dark theme, same look as the rest of the Zeng apps.
2. **Player / runtime** (Rust + Bevy, MIT/Apache): loads a project folder, renders with wgpu (Metal, Vulkan, DX12), runs scripts, physics (Rapier or Avian), audio, input. The editor launches it as a separate window at first (robust, crashes don't take the editor down); embedding the viewport in the editor window is a later milestone.
3. **Scripting layer:** Blockly workspaces export a small JSON block tree. The runtime interprets it directly (or compiles it to Rhai, a pure-Rust script engine that cross-compiles cleanly). No C toolchain needed on user machines.

## Project format (a folder)
```
MyGame/
  project.json          name, version, settings
  scenes/main.scene.json    entities, transforms, components
  scripts/*.blocks.json     Blockly workspaces (one per script)
  assets/                   .glb/.gltf, textures, audio
```
Plain files, diff-friendly, no database.

## Blocks (first set)
Events (game start, key pressed, collision, timer). Motion (move, rotate, look at, apply force). Looks (color, material, light, camera, show/hide). 3D (spawn from asset, clone, destroy, raycast, distance to). Physics (gravity, bounce, make solid). Sound (play, loop, volume). Control (if, repeat, wait, forever). Variables, lists, functions, messages between objects.

## Rendering target
PBR, directional/point/spot lights, cascaded shadow maps, bloom, tonemapping, SSAO, fog, skybox/HDRI, glTF with animations. Quality presets.

## Milestones (each one verifiable)
- **M0 Spike:** Bevy player opens a window, shows a lit cube, camera orbit, reads a scene file. Proves the toolchain on macOS, and cross-builds for Windows/Linux in CI.
- **M1 Blocks:** Blockly editor in a webview, saves `.blocks.json`; runtime runs "when key pressed, move cube". Play button launches the player.
- **M2 Scene editor:** scene tree, inspector (position/rotation/scale, material, light), import glTF, drag-and-drop assets.
- **M3 Physics and sound:** rigid bodies, collisions as events, audio blocks.
- **M4 Polish:** hot reload, error messages that point at the block, undo/redo, example games.
- **M5 Export and packaging:** standalone app per OS, installers for the editor, docs and templates.

## Risks
- Scope: a full engine is years of work; reusing Bevy and Blockly is what makes M0 to M3 realistic.
- Bevy releases often break its API. Pin one version per milestone.
- GPU drivers differ on Windows/Linux. Only macOS can be tested locally; others via CI and community.
- Scripts run through an interpreter, so very heavy games will be slower than hand-written Rust. Fine for the target audience.
- Verification limits: I cannot test Windows/Linux rendering on this Mac beyond what runs headless.

## Licensing
Blockly (Apache-2.0), Bevy (MIT/Apache-2.0), Rapier (Apache-2.0), Rhai (MIT/Apache-2.0). Keep notices in the distribution.

## Staying free (decisions)
- **License:** the engine and editor are open source under MIT OR Apache-2.0 (same as Bevy). No royalties, no revenue share, no seat fees. Games exported with ZengEngine belong entirely to their makers.
- **No running costs:** everything runs on the user's computer. No accounts, no cloud services, no telemetry server. Nothing to bill or shut down.
- **Free distribution:** GitHub Releases for downloads (free for public repos), GitHub Actions for builds (free for public repos), static docs site on the existing zengbot.com hosting.
- **Free content:** ship starter templates with CC0 assets only (e.g. Kenney, Poly Haven); record every asset's license.
- **Sustainability:** optional donations (GitHub Sponsors) only. No paywalled features, ever, so the "free" promise cannot be broken later.
- **Third-party notices:** keep all dependency licenses in a NOTICE file shipped with every build.
