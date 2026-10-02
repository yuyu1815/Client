#version 450

#include "fog.glsl"

layout(set = 1, binding = 0) uniform sampler2D entity_tex;
layout(location = 0) in vec2 v_tex_coords;
layout(location = 2) in float v_fog;
layout(location = 3) in vec3 v_fog_color;
layout(location = 5) in vec4 v_vertex_color;
layout(location = 0) out vec4 out_color;

// Native EnderDragonRenderer supplies this same constant normal for every beam vertex.
const vec3 BEAM_NORMAL = vec3(0.0, -1.0, 0.0);

void main() {
    vec4 color = texture(entity_tex, v_tex_coords) * v_vertex_color;
    if (color.a < 0.1) discard;
    out_color = vec4(apply_fog(color.rgb, v_fog, v_fog_color), color.a);
}
