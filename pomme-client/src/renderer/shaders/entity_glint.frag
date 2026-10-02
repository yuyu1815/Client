#version 450

layout(set = 1, binding = 0) uniform sampler2D glint_tex;
layout(location = 0) in vec2 v_tex_coords;
layout(location = 0) out vec4 out_color;

void main() {
    out_color = texture(glint_tex, v_tex_coords);
}
