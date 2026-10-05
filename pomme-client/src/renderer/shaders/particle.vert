#version 450

#include "fog.glsl"

#include "camera_ubo.glsl"

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 uv;
layout(location = 2) in vec4 color;
layout(location = 3) in uint light_uv;

layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out float v_fog;
layout(location = 3) out vec3 v_fog_color;

vec4 sample_particle_lightmap(uint uv) {
    // Equivalent to Java sample_lightmap(Sampler2, UV2): the byte light
    // coordinates map to texel-space by /16, then clamp-to-edge bilinear.
    vec2 p = clamp(vec2(float(uv & 255u), float((uv >> 8u) & 255u)) / 16.0, 0.0, 15.0);
    ivec2 lo = ivec2(floor(p));
    ivec2 hi = min(lo + ivec2(1), ivec2(15));
    vec2 f = fract(p);
    vec4 a = particle_lightmap[lo.y * 16 + lo.x];
    vec4 b = particle_lightmap[lo.y * 16 + hi.x];
    vec4 c = particle_lightmap[hi.y * 16 + lo.x];
    vec4 d = particle_lightmap[hi.y * 16 + hi.x];
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

void main() {
    // Positions are absolute world-space; render camera-relative for precision
    // (matches item_entity.vert).
    vec3 rel = position - camera_pos.xyz;
    gl_Position = view_proj * vec4(rel, 1.0);
    v_uv = uv;
    v_color = vec4(color.rgb * sample_particle_lightmap(light_uv).rgb, color.a);
    v_fog = total_fog_value(rel, fog_env, camera_pos.w, fog_color.w);
    v_fog_color = fog_color.rgb;
}
