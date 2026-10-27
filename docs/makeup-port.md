# MakeUp Ultra Fast port

Horizon is reimplementing MakeUp Ultra Fast 9.6's visual effects in Rust and WGSL. It does not execute
OptiFine/Iris GLSL shader packs. Upstream code and the LGPL-3.0-or-later license are credited in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) and `third_party/makeup-ultrafast/LICENSE`.

The implementation is opt-in through **Graphics → Advanced Graphics**. MakeUp Visuals runs after the world
pass and before the hand and HUD, so menus and UI keep their normal colors. Chromatic aberration and depth of
field have separate switches; depth of field also has a focus-distance slider. Color vision correction offers
three daltonization modes and can run without enabling the tone-map effects.

## Effect coverage

| MakeUp feature | Horizon port state | Notes |
|---|---|---|
| Final color sigmoid / tone map | **Ported** | `world_post.wgsl`, translated to WGSL from upstream `custom_sigmoid`. |
| Bloom | **Partial** | First low-cost single-pass highlight gather; upstream uses separate buffers and mip levels. |
| FXAA | **Reused** | Horizon's existing FXAA can run with the MakeUp post pass. |
| Chromatic aberration | **Ported** | Optional radial RGB split with a separate Graphics setting; disabled by default like MakeUp's stock profiles. |
| Ambient occlusion | **Partial** | Uses reverse-Z scene-depth neighbors for contact darkening; upstream GTAO uses horizon sampling and temporal/buffer support. |
| Shadows | Not ported | Needs a light-space depth pass across terrain, entities and block entities. |
| Temporal AA / history | Not ported | Needs jittered projection, motion/reprojection data and persistent history targets. |
| Depth of field | **Partial** | Optional disk gather using reverse-Z depth and a user focus distance; upstream uses more blur/history passes. |
| Color blindness correction | **Ported** | Optional Protanopia, Deuteranopia and Tritanopia daltonization modes using MakeUp's algorithm. |
| Water reflection / refraction | Not ported | Needs scene color/depth access during translucent terrain rendering. |
| Volumetric clouds and light | Not ported | Horizon's current clouds and sky remain in use. |
| Motion blur | Not ported | Needs previous-frame transforms or a temporal velocity buffer. |
| Auto exposure | Not ported | Needs luminance reduction and a temporally adapted exposure value. |
| Color schemes and dimension fog | Not ported | Requires carrying dimension/biome lighting parameters into the post and world passes. |

This is an effect-level checklist, not a claim of percentage parity. The current implementation ports the
sigmoid tone map, optional chromatic aberration and color blindness correction, and approximates bloom, GTAO
and depth of field with low-cost screen sampling. The remaining work is dominated by renderer features and
render targets, not shader syntax translation.
