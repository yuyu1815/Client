#version 450

#include "fog.glsl"
#include "camera_ubo.glsl"

layout(push_constant) uniform PushConstants { mat4 model; };
layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 2) in vec4 light_tint;
layout(location = 0) out vec2 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) out float v_fog;
layout(location = 3) out vec3 v_fog_color;

void main() {
    vec3 rel = (model * vec4(position, 1.0)).xyz - camera_pos.xyz;
    // Vanilla VIEW_OFFSET_Z_LAYERING: perspective ModelViewMat scales by 1-1/4096.
    gl_Position = view_proj * vec4(rel * (1.0 - 1.0 / 4096.0), 1.0);
    v_uv = tex_coords;
    v_color = vec4(light_tint.gba, light_tint.a);
    v_fog = total_fog_value(rel, fog_env, camera_pos.w, fog_color.w);
    v_fog_color = fog_color.rgb;
}
