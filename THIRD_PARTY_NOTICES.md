# Third-party notices

## wgpu-hal

`third_party/wgpu-hal` is [wgpu-hal](https://github.com/gfx-rs/wgpu) 30.0.1 by the gfx-rs
developers, with one upstream fix applied
([gfx-rs/wgpu#10124](https://github.com/gfx-rs/wgpu/pull/10124)). It is licensed under MIT or
Apache-2.0; both licenses are in that folder (`LICENSE.MIT`, `LICENSE.APACHE`).

## Pixelify Sans

`crates/horizon-theme/fonts/PixelifySans-*.ttf` is [Pixelify Sans](https://github.com/eifetx/Pixelify-Sans)
by The Pixelify Sans Project Authors, licensed under the SIL Open Font License 1.1
(`crates/horizon-theme/fonts/OFL-PixelifySans.txt`).

## Pixelarticons

`crates/horizon-theme/icons/pixel/*.svg` are icons from
[Pixelarticons](https://github.com/halfmage/pixelarticons) by Gerrit Halfmann, under the MIT
license (`crates/horizon-theme/icons/pixel/LICENSE-pixelarticons.txt`).

## TouchController

Horizon's native touch controls reimplement the D-pad and joystick movement methods, gesture and split interaction
styles, context-sensitive swim, flight and vehicle controls, and layout options inspired by
[TouchController](https://github.com/TouchController/TouchController), by fifth_light. The upstream project is
licensed under LGPL-3.0-or-later. Horizon's implementation is Rust and does not include TouchController source
code or assets.

## MakeUp - Ultra Fast

The MakeUp-inspired renderer work in `crates/horizon-client/src/world_post.wgsl` is based on the shader
features and rendering flow of [MakeUp - Ultra Fast](https://github.com/javiergcim/MakeUpUltraFast) by
XavierFST (Javier Garduño), version 9.6. Upstream shader code is licensed under LGPL-3.0-or-later; the
license is included at `third_party/makeup-ultrafast/LICENSE`. Horizon's WGSL implementation is rewritten
for wgpu and does not bundle the original GLSL pack. The port is incomplete; new derived pieces must retain
this attribution and comply with the LGPL.
