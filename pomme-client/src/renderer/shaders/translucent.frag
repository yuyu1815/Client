#version 450

#include "fog.glsl"
#include "atlas_sprite.glsl"

layout(set = 1, binding = 0) uniform sampler2D atlas_texture;

layout(location = 0) in vec2 v_sprite_uv;
layout(location = 1) in float v_light;
layout(location = 2) in vec3 v_tint;
layout(location = 3) flat in float v_visibility;
layout(location = 4) in vec3 v_fog_color;
layout(location = 5) in float v_fog;
layout(location = 6) flat in uint v_sprite;

layout(location = 0) out vec4 out_color;

void main() {
    if (terrain_edit_masked(v_visibility)) discard;
    vec4 color = sample_atlas_sprite(atlas_texture, v_sprite_uv, v_sprite);
    vec3 shaded =
        shade_chunk_surface(color.rgb, v_tint, v_light, abs(v_visibility), v_fog_color, v_fog);
    out_color = vec4(shaded, color.a);
}
