#version 450
#include "camera_ubo.glsl"
layout(location = 0) in vec3 position;
layout(location = 1) in vec3 uv_layer;
layout(location = 2) in vec4 color;
layout(location = 3) in float colored;
layout(location = 0) out vec3 v_uv;
layout(location = 1) out vec4 v_color;
layout(location = 2) flat out int v_colored;
void main() {
    gl_Position = view_proj * vec4(position - camera_pos.xyz, 1.0);
    v_uv = uv_layer;
    v_color = color;
    v_colored = int(colored);
}
