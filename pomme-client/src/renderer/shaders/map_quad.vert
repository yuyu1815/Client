#version 450

#include "camera_ubo.glsl"

layout(push_constant) uniform PushConstants {
    mat4 model;
};

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 0) out vec2 v_tex_coords;

void main() {
    vec3 relative_position = (model * vec4(position, 1.0)).xyz - camera_pos.xyz;
    gl_Position = view_proj * vec4(relative_position, 1.0);
    v_tex_coords = tex_coords;
}
