# ZengEngine

Make 3D games by snapping blocks together, like Scratch, on your own computer.

- **Blocks like Scratch.** Events, motion, looks, sound, physics, sensing, variables, lists and your own functions. No typing code.
- **Real 3D.** Lit scenes with shadows, metal and glow, 3D models (glTF/GLB), physics and sound, rendered with Bevy on Metal, Vulkan or DirectX.
- **A stage like Scratch.** Press the green flag and your game runs in the top-right corner of the editor. It updates as you change blocks or objects. Or play it in a big window.
- **Free for good.** MIT or Apache-2.0. No accounts, no servers, no fees, no royalties. Games you export belong to you.

## Try it

Download a build for your computer from the Releases page, open **ZengEngine**, pick **Coin Collector**, press **Play**. W A S D moves.

Or build it yourself (needs [Rust](https://rustup.rs)):

```
cargo run --release -p zeng-editor      # the editor
cargo run --release -p zeng-player templates/coin-collector   # just play a game
scripts/package.sh                      # make a distributable in dist/
```

Linux also needs `libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libudev-dev`. Windows needs the Edge WebView2 runtime (already on Windows 11).

## Docs

- [Getting started](docs/getting-started.md)
- [Block reference](docs/blocks.md)
- [Project format](docs/project-format.md)
- [Design notes](docs/specs/2026-10-26-zengengine-design.md)

## What it can and cannot do yet

It makes small and medium 3D games well: platform-style collectors, physics toys, arcade games. It is not a AAA pipeline. Missing today: animation graphs, terrain, networking, VR, mobile export, exporting for another OS than the one you build on (build on each OS, or use the CI workflow).
