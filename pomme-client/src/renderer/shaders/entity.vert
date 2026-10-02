#version 450

#include "fog.glsl"

#include "camera_ubo.glsl"

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 2) in vec4 light_tint;

// Per-instance data (binding 1, vertexInputRate = instance): the 4 model-matrix
// columns, then tint, overlay color, and uv transform (offset xy, scale zw).
layout(location = 3) in vec4 i_model_0;
layout(location = 4) in vec4 i_model_1;
layout(location = 5) in vec4 i_model_2;
layout(location = 6) in vec4 i_model_3;
layout(location = 7) in vec4 i_tint;
layout(location = 8) in vec4 i_overlay;
layout(location = 9) in vec4 i_uv;

layout(location = 0) out vec2 v_tex_coords;
layout(location = 1) out vec4 v_tint;
layout(location = 2) out float v_fog;
layout(location = 3) out vec3 v_fog_color;
layout(location = 4) out vec4 v_overlay;
layout(location = 5) out vec4 v_vertex_color;

void main() {
    mat4 model = mat4(i_model_0, i_model_1, i_model_2, i_model_3);
    vec4 world_pos = model * vec4(position, 1.0);
    vec3 rel = world_pos.xyz - camera_pos.xyz;
    gl_Position = view_proj * vec4(rel, 1.0);
    vec2 uv_scale = mix(vec2(1.0), i_uv.zw, greaterThan(i_uv.zw, vec2(0.0)));
    v_tex_coords = tex_coords * uv_scale + i_uv.xy;
    v_tint = i_tint;
    v_overlay = i_overlay;
    v_fog = total_fog_value(rel, fog_env, camera_pos.w, fog_color.w);
    v_fog_color = fog_color.rgb;
    v_vertex_color = light_tint;
}
