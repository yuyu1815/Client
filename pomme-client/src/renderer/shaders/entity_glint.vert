#version 450

#include "fog.glsl"
#include "camera_ubo.glsl"

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 2) in vec4 light_tint;
layout(location = 3) in vec4 i_model_0;
layout(location = 4) in vec4 i_model_1;
layout(location = 5) in vec4 i_model_2;
layout(location = 6) in vec4 i_model_3;
layout(location = 7) in vec4 i_tint;
layout(location = 8) in vec4 i_overlay;
layout(location = 9) in vec4 i_uv;

layout(location = 0) out vec2 v_tex_coords;
layout(location = 1) out vec4 v_color_modulator;
layout(location = 2) out float v_fog;

void main() {
    mat4 model = mat4(i_model_0, i_model_1, i_model_2, i_model_3);
    vec3 rel = (model * vec4(position, 1.0)).xyz - camera_pos.xyz;
    gl_Position = view_proj * vec4(rel, 1.0);

    // Vanilla TextureTransform.ENTITY_GLINT_TEXTURING: translate, rotate 10°,
    // then scale by 0.5. Scroll offsets use millis * glintSpeed * 8.
    float c = cos(0.1745329252);
    float s = sin(0.1745329252);
    mat2 rotation = mat2(c, s, -s, c);
    v_tex_coords = rotation * (tex_coords * 0.5) + i_uv.xy;
    v_color_modulator = i_tint;
    v_fog = total_fog_value(rel, fog_env, camera_pos.w, fog_color.w);
}
