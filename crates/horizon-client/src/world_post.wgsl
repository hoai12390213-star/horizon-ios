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
    bloom_strength: f32,
    ao_strength: f32,
    volumetric_light: f32,
    contact_shadows: f32,
    contact_strength: f32,
    motion_blur: f32,
    volumetric_clouds: f32,
    temporal_aa: f32,
    history_valid: f32,
}
@group(0) @binding(3) var<uniform> post_settings: PostSettings;
struct EnvironmentSettings {
    sun_data: vec4<f32>,
    sun_direction: vec4<f32>,
    motion_data: vec4<f32>,
    cloud_origin_time: vec4<f32>,
    view_right: vec4<f32>,
    view_up: vec4<f32>,
    view_back: vec4<f32>,
    cloud_params: vec4<f32>,
    previous_right: vec4<f32>,
    previous_up: vec4<f32>,
    previous_back: vec4<f32>,
    camera_delta: vec4<f32>,
    taa_jitter: vec4<f32>,
}
@group(0) @binding(5) var<uniform> environment_settings: EnvironmentSettings;
@group(0) @binding(6) var taa_history: texture_2d<f32>;
@group(0) @binding(7) var water_mask: texture_2d<f32>;
struct ExposureState {
    value: f32,
}
@group(0) @binding(4) var<storage, read_write> exposure_state: ExposureState;
var<workgroup> exposure_samples: array<f32, 256>;

// MakeUp's temporally adapted auto-exposure, reduced from a 16x16 screen grid.
@compute @workgroup_size(16, 16, 1)
fn cs_exposure(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) lane: u32
) {
    let uv = (vec2<f32>(id.xy) + vec2(0.5)) / 16.0;
    let color = max(textureSampleLevel(world, world_sampler, uv, 0.0).rgb, vec3(0.0001));
    let luminance = dot(color, vec3(0.2126, 0.7152, 0.0722));
    exposure_samples[lane] = log2(max(luminance, 0.0001));
    workgroupBarrier();

    var stride = 128u;
    loop {
        if stride == 0u { break; }
        if lane < stride {
            exposure_samples[lane] += exposure_samples[lane + stride];
        }
        workgroupBarrier();
        stride /= 2u;
    }
    if lane == 0u {
        let exposure_target = clamp(exp2(-2.474 + exposure_samples[0] / -256.0), 0.25, 4.0);
        exposure_state.value = mix(exposure_state.value, exposure_target, 0.06);
    }
}

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
@fragment fn fs_bloom_extract(in: PostOut) -> @location(0) vec4<f32> {
    let color = max(
        textureSampleLevel(world, world_sampler, in.uv, 0.0).rgb * exposure_state.value,
        vec3(0.0)
    );
    let brightness = max(max(color.r, color.g), color.b);
    let contribution = max(brightness - 0.72, 0.0) / max(brightness, 0.001);
    return vec4(color * contribution, 1.0);
}
@fragment fn fs_downsample(in: PostOut) -> @location(0) vec4<f32> {
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
    var glow = vec3(0.0);
    var weight_sum = 0.0;
    let levels = textureNumLevels(world);
    for (var level = 1u; level < levels; level += 1u) {
        let sample_color = textureSampleLevel(world, world_sampler, uv, f32(level)).rgb;
        let weight = 1.0 / pow(f32(level), 0.72);
        glow += sample_color * weight;
        weight_sum += weight;
    }
    return glow / max(weight_sum, 0.001) * 1.6;
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

// Reconstruct a camera-relative point from Horizon's reverse-Z depth.
fn reconstruct_world_position(uv: vec2<f32>, depth: f32) -> vec3<f32> {
    let size = textureDimensions(scene_depth);
    let jitter = environment_settings.taa_jitter.xy;
    let screen = uv * 2.0 - vec2(1.0);
    let view_ndc = vec2(
        screen.x - 2.0 * jitter.x / f32(size.x),
        -screen.y + 2.0 * jitter.y / f32(size.y)
    );
    let tangent = environment_settings.cloud_params.x;
    let aspect = environment_settings.cloud_params.y;
    let ray = normalize(
        environment_settings.view_right.xyz * (view_ndc.x * aspect * tangent)
        + environment_settings.view_up.xyz * (view_ndc.y * tangent)
        - environment_settings.view_back.xyz
    );
    let forward_cosine = max(dot(ray, -environment_settings.view_back.xyz), 0.0001);
    return ray * (0.05 / max(depth, 0.00001) / forward_cosine);
}

// Multi-radius horizon search using reverse-Z depth and reconstructed surface
// normals. It keeps local contact shaping, but off-screen blockers and the
// upstream GTAO horizon integral remain unavailable.
fn makeup_ao(uv: vec2<f32>) -> f32 {
    let size = textureDimensions(scene_depth);
    let maximum = vec2<i32>(size) - vec2(1);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), maximum);
    let center = textureLoad(scene_depth, pixel, 0);
    if center <= 0.00001 {
        return 1.0;
    }
    let center_position = reconstruct_world_position(uv, center);
    let texel = 1.0 / vec2<f32>(size);
    let left_uv = clamp(uv - vec2(texel.x, 0.0), vec2(0.0), vec2(1.0));
    let right_uv = clamp(uv + vec2(texel.x, 0.0), vec2(0.0), vec2(1.0));
    let up_uv = clamp(uv - vec2(0.0, texel.y), vec2(0.0), vec2(1.0));
    let down_uv = clamp(uv + vec2(0.0, texel.y), vec2(0.0), vec2(1.0));
    let left_depth = textureLoad(scene_depth, max(pixel - vec2(1, 0), vec2(0)), 0);
    let right_depth = textureLoad(scene_depth, min(pixel + vec2(1, 0), maximum), 0);
    let up_depth = textureLoad(scene_depth, max(pixel - vec2(0, 1), vec2(0)), 0);
    let down_depth = textureLoad(scene_depth, min(pixel + vec2(0, 1), maximum), 0);
    var left_position = center_position;
    var right_position = center_position;
    var up_position = center_position;
    var down_position = center_position;
    if left_depth > 0.00001 {
        left_position = reconstruct_world_position(left_uv, left_depth);
    }
    if right_depth > 0.00001 {
        right_position = reconstruct_world_position(right_uv, right_depth);
    }
    if up_depth > 0.00001 {
        up_position = reconstruct_world_position(up_uv, up_depth);
    }
    if down_depth > 0.00001 {
        down_position = reconstruct_world_position(down_uv, down_depth);
    }
    let dx = select(right_position - center_position, center_position - left_position,
        abs(left_depth - center) < abs(right_depth - center));
    let dy = select(down_position - center_position, center_position - up_position,
        abs(up_depth - center) < abs(down_depth - center));
    var normal = cross(dx, dy);
    if dot(normal, normal) < 0.000001 {
        return 1.0;
    }
    normal = normalize(normal);
    if dot(normal, -center_position) < 0.0 {
        normal = -normal;
    }
    let offsets = array<vec2<i32>, 8>(
        vec2(-1, -1), vec2(0, -1), vec2(1, -1), vec2(-1, 0),
        vec2(1, 0), vec2(-1, 1), vec2(0, 1), vec2(1, 1)
    );
    let radii = array<i32, 3>(2, 5, 10);
    let center_distance = 0.05 / max(center, 0.00001);
    var occlusion = 0.0;
    for (var i = 0u; i < 8u; i += 1u) {
        var horizon = 0.0;
        for (var j = 0u; j < 3u; j += 1u) {
            let sample_pixel = pixel + offsets[i] * radii[j];
            if any(sample_pixel < vec2(0)) || any(sample_pixel > maximum) {
                continue;
            }
            let neighbor = textureLoad(scene_depth, sample_pixel, 0);
            if neighbor <= 0.00001 {
                continue;
            }
            // Reverse-Z: greater depth is closer. Limit the range so distant
            // background geometry cannot cast a screen-wide dark halo.
            let nearer = max(neighbor - center, 0.0) / max(center, 0.001);
            let sample_distance = 0.05 / max(neighbor, 0.00001);
            let range_fade = 1.0 - smoothstep(0.35, 3.5, abs(sample_distance - center_distance));
            let distance_fade = 1.0 - f32(j) * 0.19;
            let sample_uv = (vec2<f32>(sample_pixel) + vec2(0.5)) / vec2<f32>(size);
            let sample_position = reconstruct_world_position(sample_uv, neighbor);
            let sample_direction = normalize(sample_position - center_position);
            let normal_facing = clamp(dot(normal, sample_direction), 0.0, 1.0);
            horizon = max(horizon, clamp((nearer - 0.025) * 0.7, 0.0, 0.85)
                * range_fade * distance_fade * normal_facing);
        }
        occlusion += horizon;
    }
    return clamp(1.0 - (occlusion / 8.0) * 0.72, 0.68, 1.0);
}

// Screen-space radial scattering from bright, unoccluded pixels toward the sun.
// This approximates MakeUp's volumetric light pass; it does not ray-march a 3D medium.
fn makeup_volumetric_light(uv: vec2<f32>) -> vec3<f32> {
    if post_settings.volumetric_light < 0.5 || environment_settings.sun_data.z < 0.5 {
        return vec3(0.0);
    }
    let sun_uv = environment_settings.sun_data.xy;
    let size = textureDimensions(scene_depth);
    let maximum = vec2<i32>(size) - vec2(1, 1);
    let ray = sun_uv - uv;
    var energy = 0.0;
    for (var i = 0u; i < 16u; i += 1u) {
        let t = (f32(i) + 0.5) / 16.0;
        let sample_uv = clamp(uv + ray * t, vec2(0.0), vec2(1.0));
        let pixel = clamp(vec2<i32>(sample_uv * vec2<f32>(size)), vec2(0), maximum);
        let depth = textureLoad(scene_depth, pixel, 0);
        let sky_visibility = 1.0 - smoothstep(0.0001, 0.012, depth);
        let color = textureSampleLevel(world, world_sampler, sample_uv, 0.0).rgb;
        let brightness = dot(color, vec3(0.2126, 0.7152, 0.0722));
        energy += smoothstep(0.48, 1.0, brightness) * sky_visibility * (1.0 - t * 0.45);
    }
    let distance_from_sun = length(ray);
    let attenuation = clamp(distance_from_sun * 1.8, 0.0, 1.0);
    return vec3(1.0, 0.86, 0.62) * energy * attenuation * 0.006;
}

// Sun-direction screen-space depth marching for local contact shadows.
// Missing off-screen samples mean this remains an approximation of a shadow map.
fn makeup_contact_shadow(uv: vec2<f32>) -> f32 {
    if post_settings.contact_shadows < 0.5 || environment_settings.sun_direction.y < 0.025 {
        return 1.0;
    }
    let size = textureDimensions(scene_depth);
    let maximum = vec2<i32>(size) - vec2(1);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), maximum);
    let center = textureLoad(scene_depth, pixel, 0);
    if center <= 0.00001 {
        return 1.0;
    }
    let origin = reconstruct_world_position(uv, center);
    let sun_direction = normalize(environment_settings.sun_direction.xyz);
    let forward = -environment_settings.view_back.xyz;
    let tangent = environment_settings.cloud_params.x;
    let aspect = environment_settings.cloud_params.y;
    var occlusion = 0.0;
    for (var i = 1u; i <= 10u; i += 1u) {
        let travel = 0.45 + f32(i) * 0.5;
        let point = origin + sun_direction * travel;
        let view_distance = dot(point, forward);
        if view_distance <= 0.05 {
            continue;
        }
        let ndc = vec2(
            dot(point, environment_settings.view_right.xyz) / (view_distance * aspect * tangent),
            dot(point, environment_settings.view_up.xyz) / (view_distance * tangent)
        );
        let sample_uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if any(sample_uv <= vec2(0.002)) || any(sample_uv >= vec2(0.998)) {
            continue;
        }
        let sample_pixel = clamp(vec2<i32>(sample_uv * vec2<f32>(size)), vec2(0), maximum);
        let neighbor = textureLoad(scene_depth, sample_pixel, 0);
        if neighbor <= 0.00001 {
            continue;
        }
        let scene_distance = 0.05 / neighbor;
        let thickness = 0.22 + travel * 0.07;
        let error = abs(scene_distance - view_distance);
        if error < thickness {
            occlusion = 1.0 - smoothstep(0.0, thickness, error);
            break;
        }
    }
    return 1.0 - occlusion * post_settings.contact_strength * 0.72;
}

// Camera rotation and approximate camera translation blur; independent object
// motion requires velocity buffers and is not represented by this pass.
fn makeup_motion_blur_rgb(uv: vec2<f32>, use_fxaa: bool) -> vec3<f32> {
    let center = scene_rgb(uv, use_fxaa);
    if post_settings.motion_blur < 0.5 {
        return center;
    }
    let size = vec2<f32>(textureDimensions(scene_depth));
    let jitter_motion = (environment_settings.taa_jitter.zw - environment_settings.taa_jitter.xy)
        / size;
    let velocity = environment_settings.motion_data.xy - jitter_motion;
    let amount = environment_settings.motion_data.z;
    if amount < 0.03 || length(velocity) < 0.0001 {
        return center;
    }
    let blur = velocity * amount;
    var color = center * 2.0;
    var weight = 2.0;
    for (var i = 1u; i <= 3u; i += 1u) {
        let t = f32(i) / 3.0;
        let w = 1.0 - t * 0.55;
        color += scene_rgb(clamp(uv + blur * t, vec2(0.0), vec2(1.0)), use_fxaa) * w;
        color += scene_rgb(clamp(uv - blur * t, vec2(0.0), vec2(1.0)), use_fxaa) * w;
        weight += 2.0 * w;
    }
    return mix(center, color / weight, amount * 0.75);
}

fn cloud_hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}

fn cloud_noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let blend_curve = f * f * (vec2(3.0) - 2.0 * f);
    let a = cloud_hash(cell);
    let b = cloud_hash(cell + vec2(1.0, 0.0));
    let c = cloud_hash(cell + vec2(0.0, 1.0));
    let d = cloud_hash(cell + vec2(1.0, 1.0));
    return mix(mix(a, b, blend_curve.x), mix(c, d, blend_curve.x), blend_curve.y);
}

fn cloud_density(p: vec2<f32>) -> f32 {
    var point = p;
    var amplitude = 0.55;
    var total = 0.0;
    var weight = 0.0;
    for (var octave = 0u; octave < 4u; octave += 1u) {
        total += cloud_noise(point) * amplitude;
        weight += amplitude;
        point = point * 2.03 + vec2(17.0, 9.0);
        amplitude *= 0.5;
    }
    return total / weight;
}

// A low-step world-anchored cloud volume. Height-shifted noise samples create
// a shallow density layer; upstream's full cloud march still has more detail.
fn makeup_cloud_rgb(uv: vec2<f32>) -> vec4<f32> {
    if post_settings.volumetric_clouds < 0.5 {
        return vec4(0.0);
    }
    let size = textureDimensions(scene_depth);
    let pixel = clamp(
        vec2<i32>(uv * vec2<f32>(size)),
        vec2(0),
        vec2<i32>(size) - vec2(1)
    );
    if textureLoad(scene_depth, pixel, 0) > 0.00001 {
        return vec4(0.0);
    }
    let screen = uv * 2.0 - vec2(1.0);
    let tan_half_fov = environment_settings.cloud_params.x;
    let aspect = environment_settings.cloud_params.y;
    let ray = normalize(
        environment_settings.view_right.xyz * (screen.x * aspect * tan_half_fov)
        + environment_settings.view_up.xyz * ((-screen.y) * tan_half_fov)
        - environment_settings.view_back.xyz
    );
    if ray.y <= 0.025 {
        return vec4(0.0);
    }
    let drift = environment_settings.cloud_origin_time.w * 0.0008;
    let sun_height = environment_settings.cloud_params.w;
    let daylight = smoothstep(-0.18, 0.12, sun_height);
    let sunset = (1.0 - smoothstep(0.12, 0.62, abs(sun_height))) * daylight;
    let day_color = mix(vec3(0.36, 0.43, 0.55), vec3(0.83, 0.87, 0.94), daylight);
    let cloud_color = mix(day_color, vec3(1.0, 0.56, 0.32), sunset * 0.72)
        * (0.22 + daylight * 0.78);
    let camera = vec3(
        environment_settings.cloud_origin_time.x,
        environment_settings.cloud_origin_time.y,
        environment_settings.cloud_origin_time.z
    );
    let bottom = 128.0;
    let top = 256.0;
    let center = 192.0;
    let inverse_ray_y = 1.0 / ray.y;
    let start = camera + ray * ((bottom - camera.y) * inverse_ray_y);
    let end = camera + ray * ((top - camera.y) * inverse_ray_y);
    let increment = (end - start) / 8.0;
    let increment_distance = length(increment);
    let opacity_distance = (top - bottom) * 0.15 * 2.0 * inverse_ray_y;
    var accumulated_opacity = 0.0;
    var cloud_depth = 0.0001;
    var first_contact = true;
    for (var step = 0u; step < 8u; step += 1u) {
        let position = start + increment * (f32(step) + 0.5);
        let noise_position = position.xz * 0.00055 + vec2(drift, drift * 0.37);
        let noise = cloud_density(noise_position);
        let cloud_shape = clamp((noise - 0.49) / 0.51, 0.0, 1.0);
        let surface_bottom = center - cloud_shape * (center - bottom);
        let surface_top = center + cloud_shape * (top - center);
        let thickness = surface_top - surface_bottom;
        var sample_opacity = 0.0;
        if position.y > surface_bottom && position.y < surface_top {
            sample_opacity = min(increment_distance, thickness);
        } else if thickness > 0.0 && step > 0u {
            let edge_distance = min(abs(position.y - surface_bottom), abs(position.y - surface_top));
            let edge_fade_distance = (top - bottom) * 0.075 * 0.3;
            if edge_distance < edge_fade_distance {
                sample_opacity = min(
                    (1.0 - edge_distance / edge_fade_distance) * increment_distance,
                    thickness
                );
            }
        }
        if sample_opacity > 0.0 {
            accumulated_opacity += sample_opacity;
            if first_contact {
                first_contact = false;
                cloud_depth = (surface_top - position.y) / (top - bottom);
            }
        }
    }
    let opacity = clamp(accumulated_opacity / max(opacity_distance, 0.001), 0.0, 1.0)
        * clamp((ray.y - 0.06) * 5.0, 0.0, 1.0);
    let density = clamp(cloud_depth, 0.0001, 1.0);
    let shaded_cloud = mix(cloud_color * 1.05, cloud_color * 0.58, sqrt(density));
    return vec4(shaded_cloud, opacity);
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
    let center_color = makeup_motion_blur_rgb(uv, use_fxaa);
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
        blurred += makeup_motion_blur_rgb(
            clamp(uv + offsets[i] * radius, vec2(0.0), vec2(1.0)),
            use_fxaa
        );
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

// A short screen-space ray march for horizontal water. It reconstructs the
// water point from reverse-Z depth, reflects the view ray around world-up,
// and accepts the first depth crossing that remains on screen.
fn makeup_water_reflection(uv: vec2<f32>) -> vec4<f32> {
    let size = textureDimensions(scene_depth);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), vec2<i32>(size) - vec2(1));
    let water_depth = textureLoad(scene_depth, pixel, 0);
    if water_depth <= 0.00001 {
        return vec4(0.0);
    }
    let jitter = environment_settings.taa_jitter.xy;
    let screen = uv * 2.0 - vec2(1.0);
    let view_ndc = vec2(
        screen.x - 2.0 * jitter.x / f32(size.x),
        -screen.y + 2.0 * jitter.y / f32(size.y)
    );
    let tangent = environment_settings.cloud_params.x;
    let aspect = environment_settings.cloud_params.y;
    let view_ray = normalize(
        environment_settings.view_right.xyz * (view_ndc.x * aspect * tangent)
        + environment_settings.view_up.xyz * (view_ndc.y * tangent)
        - environment_settings.view_back.xyz
    );
    let forward = -environment_settings.view_back.xyz;
    let forward_cosine = max(dot(view_ray, forward), 0.0001);
    let water_distance = 0.05 / water_depth / forward_cosine;
    let origin = view_ray * water_distance;
    let reflected_ray = normalize(reflect(view_ray, vec3(0.0, 1.0, 0.0)));
    var previous_delta = -1.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let travel = 0.8 + f32(i * i) * 0.16;
        let point = origin + reflected_ray * travel;
        let view_distance = dot(point, forward);
        if view_distance <= 0.05 {
            continue;
        }
        let ndc = vec2(
            dot(point, environment_settings.view_right.xyz) / (view_distance * aspect * tangent),
            dot(point, environment_settings.view_up.xyz) / (view_distance * tangent)
        );
        let sample_uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        if any(sample_uv <= vec2(0.002)) || any(sample_uv >= vec2(0.998)) {
            break;
        }
        let sample_pixel = clamp(
            vec2<i32>(sample_uv * vec2<f32>(size)),
            vec2(0),
            vec2<i32>(size) - vec2(1)
        );
        let scene_depth_value = textureLoad(scene_depth, sample_pixel, 0);
        if scene_depth_value <= 0.00001 {
            let sky = textureSampleLevel(world, world_sampler, sample_uv, 0.0).rgb;
            let sky_weight = smoothstep(0.0, 12.0, point.y + 5.0) * 0.68;
            if sky_weight > 0.02 {
                return vec4(sky, sky_weight);
            }
            continue;
        }
        let scene_distance = 0.05 / scene_depth_value;
        let delta = view_distance - scene_distance;
        let thickness = 0.18 + travel * 0.05;
        if delta >= 0.0 && (previous_delta < 0.0 || delta < thickness * 2.0) {
            let confidence = clamp(1.0 - delta / (thickness * 3.0), 0.18, 0.92);
            let reflected = textureSampleLevel(world, world_sampler, sample_uv, 0.0).rgb;
            return vec4(reflected, confidence);
        }
        previous_delta = delta;
    }
    return vec4(0.0);
}

fn post_rgb(uv: vec2<f32>, use_fxaa: bool, use_makeup: bool, use_chroma: bool) -> vec3<f32> {
    var source_uv = uv;
    if use_makeup && environment_settings.cloud_params.z > 0.5 {
        // Underwater ripple/refraction approximation. The scene color target
        // is available after translucent rendering, but not the water surface
        // normal, so use animated screen-space waves and a restrained tint.
        let time = environment_settings.cloud_origin_time.w * 0.001;
        let wave = vec2(
            sin(uv.y * 118.0 + time * 1.7) + sin(uv.y * 47.0 - time * 0.8),
            cos(uv.x * 96.0 - time * 1.2) + sin(uv.x * 53.0 + time)
        );
        source_uv = clamp(uv + wave * 0.0007, vec2(0.0), vec2(1.0));
    }
    var scene = makeup_dof_rgb(source_uv, use_fxaa);
    if use_chroma {
        scene = chromatic_rgb(source_uv, use_fxaa);
    }
    if use_makeup {
        // MakeUp's GTAO and bloom are multi-target effects. Horizon uses
        // normal-aware screen-space occlusion and a multi-level highlight
        // gather, while preserving the upstream sigmoid finishing curve.
        if post_settings.ao_strength > 0.001 {
            scene *= mix(1.0, makeup_ao(source_uv), post_settings.ao_strength);
        }
        scene *= makeup_contact_shadow(source_uv);
        if post_settings.bloom_strength > 0.001 {
            scene += bloom_highlights(source_uv) * post_settings.bloom_strength;
        }
        scene += makeup_volumetric_light(source_uv);
        let cloud = makeup_cloud_rgb(source_uv);
        scene = mix(scene, cloud.rgb, cloud.a);
        if environment_settings.motion_data.w > 0.5 {
            let water = textureSampleLevel(water_mask, world_sampler, uv, 0.0).r;
            if water > 0.5 {
                let time = environment_settings.cloud_origin_time.w * 0.001;
                let ripple = sin(uv.x * 125.0 + time * 1.3) * 0.45;
                let reflection = makeup_water_reflection(clamp(
                    uv + vec2(ripple / f32(textureDimensions(world).x), 0.0),
                    vec2(0.0),
                    vec2(1.0)
                ));
                let fresnel = pow(clamp(abs(uv.y - 0.5) * 2.0, 0.0, 1.0), 1.4);
                let reflectivity = (0.2 + fresnel * 0.3) * reflection.a;
                scene = mix(scene, reflection.rgb * vec3(0.82, 0.94, 1.12), reflectivity);
            }
        }
        if environment_settings.cloud_params.z > 0.5 {
            scene = scene * vec3(0.68, 0.84, 1.08) + vec3(0.006, 0.022, 0.045);
        }
        scene = makeup_tonemap(scene * exposure_state.value);
        // Lightweight dimension grades layered over Horizon's vanilla fog.
        // These do not replace MakeUp's dimension-specific fog profiles.
        if environment_settings.sun_data.w > 0.5 && environment_settings.sun_data.w < 1.5 {
            scene = scene * vec3(1.08, 0.88, 0.76) + vec3(0.018, 0.004, 0.0);
        } else if environment_settings.sun_data.w >= 1.5 {
            scene = scene * vec3(0.86, 0.82, 1.08) + vec3(0.008, 0.004, 0.024);
        }
    }
    if post_settings.color_blind_mode > 0.5 {
        scene = makeup_color_blind(scene);
    }
    return scene;
}

fn taa_color(uv: vec2<f32>, use_fxaa: bool, use_chroma: bool) -> vec4<f32> {
    let current = post_rgb(uv, use_fxaa, true, use_chroma);
    let size = textureDimensions(scene_depth);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), vec2<i32>(size) - vec2(1));
    let current_depth = textureLoad(scene_depth, pixel, 0);
    if post_settings.temporal_aa < 0.5 || post_settings.history_valid < 0.5 {
        return vec4(current, current_depth);
    }
    var history_uv = uv + environment_settings.motion_data.xy;
    var expected_history_depth = current_depth;
    if current_depth > 0.00001 {
        let history_size = vec2<f32>(textureDimensions(taa_history));
        let unjittered_uv = uv - environment_settings.taa_jitter.xy / history_size;
        let screen = unjittered_uv * 2.0 - vec2(1.0);
        let tangent = environment_settings.cloud_params.x;
        let aspect = environment_settings.cloud_params.y;
        let ray = normalize(
            environment_settings.view_right.xyz * (screen.x * aspect * tangent)
            + environment_settings.view_up.xyz * ((-screen.y) * tangent)
            - environment_settings.view_back.xyz
        );
        let view_depth = 0.05 / current_depth;
        let forward_cosine = max(dot(ray, -environment_settings.view_back.xyz), 0.0001);
        let current_relative = ray * (view_depth / forward_cosine);
        let previous_relative = current_relative + environment_settings.camera_delta.xyz;
        let previous_view_depth = dot(previous_relative, -environment_settings.previous_back.xyz);
        if previous_view_depth <= 0.05 {
            return vec4(current, current_depth);
        }
        let previous_ndc_x = dot(previous_relative, environment_settings.previous_right.xyz)
            / (previous_view_depth * aspect * tangent);
        let previous_ndc_y = dot(previous_relative, environment_settings.previous_up.xyz)
            / (previous_view_depth * tangent);
        history_uv = vec2(previous_ndc_x * 0.5 + 0.5, 0.5 - previous_ndc_y * 0.5)
            + environment_settings.taa_jitter.zw / history_size;
        expected_history_depth = 0.05 / previous_view_depth;
    }
    if any(history_uv < vec2(0.0)) || any(history_uv > vec2(1.0)) {
        return vec4(current, current_depth);
    }
    let previous = textureSampleLevel(taa_history, world_sampler, history_uv, 0.0);
    let relative_depth_error = abs(previous.a - expected_history_depth)
        / max(max(previous.a, expected_history_depth), 0.0001);
    let depth_rejection = smoothstep(0.035, 0.16, relative_depth_error);
    let color_error = length(previous.rgb - current) * 0.57735;
    let color_rejection = smoothstep(0.18, 0.55, color_error) * 0.72;
    let rejection = max(depth_rejection, color_rejection);
    let motion_fade = 1.0 - environment_settings.motion_data.z * 0.65;
    let history_weight = 0.88 * (1.0 - rejection) * motion_fade;
    return vec4(mix(current, previous.rgb, history_weight), current_depth);
}

@fragment fn fs_makeup_taa(in: PostOut) -> @location(0) vec4<f32> {
    return taa_color(in.uv, false, false);
}
@fragment fn fs_makeup_fxaa_taa(in: PostOut) -> @location(0) vec4<f32> {
    return taa_color(in.uv, true, false);
}
@fragment fn fs_makeup_chroma_taa(in: PostOut) -> @location(0) vec4<f32> {
    return taa_color(in.uv, false, true);
}
@fragment fn fs_makeup_fxaa_chroma_taa(in: PostOut) -> @location(0) vec4<f32> {
    return taa_color(in.uv, true, true);
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
