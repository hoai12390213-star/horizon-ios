# MakeUp Ultra Fast port

Horizon is reimplementing MakeUp Ultra Fast 9.6's visual effects in Rust and WGSL. It does not execute
OptiFine/Iris GLSL shader packs. Upstream code and the LGPL-3.0-or-later license are credited in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) and `third_party/makeup-ultrafast/LICENSE`.

The implementation is opt-in through **Shaders → MakeUp - Ultra Fast → Apply**. Applying MakeUp enables its
visual profile: temporal AA, water reflections, 70% bloom, 50% AO, volumetric light and clouds, and
screen-space shadows. Its cloud pass replaces Horizon's vanilla cloud mesh while enabled, since that mesh writes depth
over the screen-space volumetric clouds. **Graphics → Advanced Graphics** lets players tune those effects.
Chromatic aberration and depth of field stay off by default; depth of field has a focus-distance slider. Color
vision correction offers three daltonization modes and can run without enabling the tone-map effects.

## Effect coverage

| MakeUp feature | Horizon port state | Notes |
|---|---|---|
| Final color sigmoid / tone map | **Ported** | `world_post.wgsl`, translated to WGSL from upstream `custom_sigmoid`. |
| Bloom | **Partial** | Bright-pass extraction into a GPU downsample pyramid, then multilevel bloom gather with editable strength; upstream has its own blur chain and composition. |
| FXAA | **Reused** | Horizon's existing FXAA can run with the MakeUp post pass. |
| Chromatic aberration | **Ported** | Optional radial RGB split with a separate Graphics setting; disabled by default like MakeUp's stock profiles. |
| Ambient occlusion | **Partial** | Uses an 8-direction, three-radius reverse-Z search with reconstructed normals and editable strength; still lacks upstream GTAO's full horizon integral and dedicated temporal filtering. |
| Shadows | **Partial** | A longer 12-step sun-direction depth march now reaches about 20 blocks on screen; no light-space shadow map or off-screen/entity casters yet. |
| Temporal AA / history | **Partial** | Jittered projection with double-buffered history, camera translation/rotation reprojection and depth/color rejection; lacks per-object velocity, so independently moving geometry may ghost. |
| Depth of field | **Partial** | Optional disk gather using reverse-Z depth and a user focus distance; upstream uses more blur/history passes. |
| Color blindness correction | **Ported** | Optional Protanopia, Deuteranopia and Tritanopia daltonization modes using MakeUp's algorithm. |
| Water reflection / refraction | **Partial** | A water-only mask limits a 12-step reverse-Z screen-space reflection march to horizontal water faces; the marcher skips water pixels to avoid reflecting the surface into itself. Underwater ripple refraction remains. Off-screen rays and varying surface normals are unavailable. |
| Volumetric light shafts | **Partial** | Screen-space bright-pixel scattering toward the sun, toggleable in Advanced Graphics; not a world-space medium march. |
| Volumetric clouds | **Partial** | World-anchored eight-step density march through a height-varying cloud layer with daylight shading; it uses procedural noise instead of the upstream cloud map and has fewer samples. |
| Motion blur | **Partial** | Optional screen blur from camera rotation and approximate translation; independently moving entities still need velocity buffers. |
| Auto exposure | **Partial** | A 16×16 GPU log-luminance reduction adapts exposure over time; upstream uses its own render targets and adaptation curve. |
| Color schemes and dimension fog | **Partial** | Subtle Nether/End post grades; Horizon keeps its vanilla fog, while MakeUp per-dimension profiles and biome colors are not ported. |

This is an effect-level checklist, not a claim of image parity. The port now covers or approximates most named
post effects, including underwater refraction and water-surface reflections, but several remain screen-space approximations. The largest
remaining gaps are light-space shadow maps, per-object motion vectors, full dimension-specific fog profiles, and biome color schemes.
