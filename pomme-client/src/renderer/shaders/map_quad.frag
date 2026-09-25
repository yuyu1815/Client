#version 450

layout(set = 1, binding = 0) uniform sampler2D map_texture;
layout(location = 0) in vec2 v_tex_coords;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(map_texture, v_tex_coords);
}
