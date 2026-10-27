// Horizon's world post pass: the world is copied or processed before the hand
// and HUD. The MakeUp-inspired path is a clean WGSL reimplementation; it does
// not load the upstream GLSL shaderpack.
@group(0) @binding(0) var world: texture_2d<f32>;
@group(0) @binding(1) var world_sampler: sampler;
@group(0) @binding(2) var scene_depth: texture_depth_2d;
struct PostSettings {
    depth_of_field: f32,
    focus_distance: f32,
    color_blind_mode: f32,
    _padding: f32,
}
@group(0) @binding(3) var<uniform> post_settings: PostSettings;

struct PostOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex fn vs_post(@builtin(vertex_index) i: u32) -> PostOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: PostOut;
    out.clip = vec4(p * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2(p.x, 1.0 - p.y);
    return out;
}

@fragment fn fs_copy(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(textureSampleLevel(world, world_sampler, in.uv, 0.0).rgb, 1.0);
}
@fragment fn fs_dof(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, false, false, false), 1.0);
}
@fragment fn fs_fxaa_dof(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, true, false, false), 1.0);
}
@fragment fn fs_color_blind(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, false, false, false), 1.0);
}
@fragment fn fs_fxaa_color_blind(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, true, false, false), 1.0);
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.299, 0.587, 0.114));
}

// FXAA 3.11 (console quality): blend along the local edge direction.
fn fxaa_rgb(uv: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(world));
    let rgb_m = textureSampleLevel(world, world_sampler, uv, 0.0).rgb;
    let nw = luma(textureSampleLevel(world, world_sampler, uv + vec2(-0.5, -0.5) * texel, 0.0).rgb);
    let ne = luma(textureSampleLevel(world, world_sampler, uv + vec2(0.5, -0.5) * texel, 0.0).rgb);
    let sw = luma(textureSampleLevel(world, world_sampler, uv + vec2(-0.5, 0.5) * texel, 0.0).rgb);
    let se = luma(textureSampleLevel(world, world_sampler, uv + vec2(0.5, 0.5) * texel, 0.0).rgb);
    let m = luma(rgb_m);
    let lo = min(m, min(min(nw, ne), min(sw, se)));
    let hi = max(m, max(max(nw, ne), max(sw, se)));
    if hi - lo < max(0.0312, hi * 0.125) {
        return rgb_m;
    }
    var dir = vec2(-((nw + ne) - (sw + se)), (nw + sw) - (ne + se));
    let reduce = max((nw + ne + sw + se) * 0.25 * 0.125, 1.0 / 128.0);
    let scale = 1.0 / (min(abs(dir.x), abs(dir.y)) + reduce);
    dir = clamp(dir * scale, vec2(-8.0), vec2(8.0)) * texel;
    let a = 0.5 * (textureSampleLevel(world, world_sampler, uv + dir * (1.0 / 3.0 - 0.5), 0.0).rgb
        + textureSampleLevel(world, world_sampler, uv + dir * (2.0 / 3.0 - 0.5), 0.0).rgb);
    let b = a * 0.5 + 0.25 * (textureSampleLevel(world, world_sampler, uv - dir * 0.5, 0.0).rgb
        + textureSampleLevel(world, world_sampler, uv + dir * 0.5, 0.0).rgb);
    let lb = luma(b);
    if lb < lo || lb > hi {
        return a;
    }
    return b;
}

@fragment fn fs_fxaa(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(fxaa_rgb(in.uv), 1.0);
}

fn bloom_highlights(uv: vec2<f32>) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(world));
    let radius = texel * 2.0;
    let offsets = array<vec2<f32>, 8>(
        vec2(-1.0, -1.0), vec2(0.0, -1.0), vec2(1.0, -1.0),
        vec2(-1.0, 0.0),                         vec2(1.0, 0.0),
        vec2(-1.0, 1.0),  vec2(0.0, 1.0),  vec2(1.0, 1.0)
    );
    var glow = vec3(0.0);
    for (var i = 0u; i < 8u; i += 1u) {
        let sample_color = textureSampleLevel(world, world_sampler, uv + offsets[i] * radius, 0.0).rgb;
        let brightness = max(max(sample_color.r, sample_color.g), sample_color.b);
        glow += sample_color * max(brightness - 0.78, 0.0);
    }
    return glow * (1.0 / 8.0);
}

// MakeUp Ultra Fast's custom_sigmoid tone map, translated from GLSL to WGSL.
// Upstream: Javier Garduño, LGPL-3.0-or-later (see third_party/makeup-ultrafast/LICENSE).
fn makeup_tonemap(color: vec3<f32>) -> vec3<f32> {
    let scaled = color * 1.4;
    let compressed = scaled / pow(pow(scaled, vec3(2.5)) + vec3(1.0), vec3(0.4));
    return pow(compressed, vec3(1.15));
}

// MakeUp's daltonization correction, translated from color_blindness.glsl.
fn makeup_color_blind(color: vec3<f32>) -> vec3<f32> {
    let lms = vec3(
        17.8824 * color.r + 43.5161 * color.g + 4.11935 * color.b,
        3.45565 * color.r + 27.1554 * color.g + 3.86714 * color.b,
        0.0299566 * color.r + 0.184309 * color.g + 1.46709 * color.b
    );
    var simulated = lms;
    if post_settings.color_blind_mode < 1.5 {
        simulated.x = 2.02344 * lms.y - 2.52581 * lms.z;
    } else if post_settings.color_blind_mode < 2.5 {
        simulated.y = 0.494207 * lms.x + 1.24827 * lms.z;
    } else {
        simulated.z = -0.395913 * lms.x + 0.801109 * lms.y;
    }
    let error = vec3(
        0.0809444479 * simulated.x - 0.130504409 * simulated.y + 0.116721066 * simulated.z,
        -0.0102485335 * simulated.x + 0.0540193266 * simulated.y - 0.113614708 * simulated.z,
        -0.000365296938 * simulated.x - 0.00412161469 * simulated.y + 0.693511405 * simulated.z
    );
    let diff = color - error;
    return color + vec3(0.0, diff.r * 0.7 + diff.g, diff.r * 0.7 + diff.b);
}

// Screen-space contact darkening using Horizon's reverse-Z scene depth.
// This is a low-cost approximation of MakeUp's GTAO, not a full horizon search.
fn makeup_ao(uv: vec2<f32>) -> f32 {
    let size = textureDimensions(scene_depth);
    let maximum = vec2<i32>(size) - vec2(1, 1);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0, 0), maximum);
    let center = textureLoad(scene_depth, pixel, 0);
    if center <= 0.00001 {
        return 1.0;
    }
    let offsets = array<vec2<i32>, 8>(
        vec2(-1, -1), vec2(0, -1), vec2(1, -1),
        vec2(-1, 0),                    vec2(1, 0),
        vec2(-1, 1),  vec2(0, 1),  vec2(1, 1)
    );
    var occlusion = 0.0;
    for (var i = 0u; i < 8u; i += 1u) {
        let neighbor = textureLoad(scene_depth, clamp(pixel + offsets[i] * 2, vec2(0, 0), maximum), 0);
        // Horizon uses reverse-Z: a larger depth value is closer to the eye.
        let relative_closer = (neighbor - center) / max(center, 0.001);
        occlusion += clamp((relative_closer - 0.06) * 0.45, 0.0, 1.0);
    }
    return clamp(1.0 - occlusion * 0.035, 0.72, 1.0);
}

fn scene_rgb(uv: vec2<f32>, use_fxaa: bool) -> vec3<f32> {
    if use_fxaa {
        return fxaa_rgb(uv);
    }
    return textureSampleLevel(world, world_sampler, uv, 0.0).rgb;
}

// MakeUp's depth-of-field blur, adapted to Horizon's reverse-Z depth target.
// It uses a small disk gather and focus distance in world blocks; the full
// upstream implementation also uses larger blur chains and temporal depth.
fn makeup_dof_rgb(uv: vec2<f32>, use_fxaa: bool) -> vec3<f32> {
    let center_color = scene_rgb(uv, use_fxaa);
    if post_settings.depth_of_field < 0.5 {
        return center_color;
    }
    let size = textureDimensions(scene_depth);
    let pixel = clamp(
        vec2<i32>(uv * vec2<f32>(size)),
        vec2(0, 0),
        vec2<i32>(size) - vec2(1, 1)
    );
    let depth = textureLoad(scene_depth, pixel, 0);
    if depth <= 0.00001 {
        return center_color;
    }
    // Horizon uses a reversed perspective projection with a 0.05-block near plane.
    let camera_distance = 0.05 / max(depth, 0.00001);
    let focus = max(post_settings.focus_distance, 2.0);
    let coc = clamp(abs(camera_distance - focus) / (focus + 8.0) * 0.003, 0.0, 0.004);
    if coc < 0.00025 {
        return center_color;
    }
    let texel = 1.0 / vec2<f32>(textureDimensions(world));
    let offsets = array<vec2<f32>, 8>(
        vec2(-0.9239, -0.3827), vec2(-0.7071, 0.7071),
        vec2(-0.3827, -0.9239), vec2(0.0, 1.0),
        vec2(0.3827, -0.9239),  vec2(0.7071, 0.7071),
        vec2(0.9239, -0.3827),  vec2(0.0, -1.0)
    );
    var blurred = vec3(0.0);
    let radius = texel * (coc / 0.004) * 6.0;
    for (var i = 0u; i < 8u; i += 1u) {
        blurred += scene_rgb(clamp(uv + offsets[i] * radius, vec2(0.0), vec2(1.0)), use_fxaa);
    }
    blurred *= 0.125;
    return mix(center_color, blurred, clamp(coc * 220.0, 0.0, 0.72));
}

// MakeUp's optional channel separation, with a small radial offset that grows
// toward the screen edges. The effect remains independently switchable.
fn chromatic_rgb(uv: vec2<f32>, use_fxaa: bool) -> vec3<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(world));
    let radial = uv - vec2(0.5, 0.5);
    let offset = radial * texel * 3.0;
    let red = makeup_dof_rgb(clamp(uv + offset, vec2(0.0), vec2(1.0)), use_fxaa).r;
    let green = makeup_dof_rgb(uv, use_fxaa).g;
    let blue = makeup_dof_rgb(clamp(uv - offset, vec2(0.0), vec2(1.0)), use_fxaa).b;
    return vec3(red, green, blue);
}

fn post_rgb(uv: vec2<f32>, use_fxaa: bool, use_makeup: bool, use_chroma: bool) -> vec3<f32> {
    var scene = makeup_dof_rgb(uv, use_fxaa);
    if use_chroma {
        scene = chromatic_rgb(uv, use_fxaa);
    }
    if use_makeup {
        // MakeUp's GTAO and bloom are multi-target effects. Horizon currently
        // uses a depth-neighbor AO approximation and a single-pass highlight
        // gather, while preserving the upstream sigmoid finishing curve.
        scene *= makeup_ao(uv);
        scene += bloom_highlights(uv) * 0.42;
        scene = makeup_tonemap(scene);
    }
    if post_settings.color_blind_mode > 0.5 {
        scene = makeup_color_blind(scene);
    }
    return scene;
}

@fragment fn fs_makeup(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, false, true, false), 1.0);
}
@fragment fn fs_makeup_fxaa(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, true, true, false), 1.0);
}
@fragment fn fs_makeup_chroma(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, false, true, true), 1.0);
}
@fragment fn fs_makeup_fxaa_chroma(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, true, true, true), 1.0);
}
@fragment fn fs_chroma(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, false, false, true), 1.0);
}
@fragment fn fs_fxaa_chroma(in: PostOut) -> @location(0) vec4<f32> {
    return vec4(post_rgb(in.uv, true, false, true), 1.0);
}
