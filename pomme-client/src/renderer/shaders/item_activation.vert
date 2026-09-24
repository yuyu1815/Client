#version 450

#include "fog.glsl"
#include "camera_ubo.glsl"

// Keep the existing item_entity_world.vert / item_entity.frag push-constant ABI:
// model@0, fragment world_light@64, nether selector@68, normal mat3@80.
// world_light is set to 1.0 for vanilla packed fullbright 15728880; the paired
// item_entity.frag has no overlay texture path, i.e. OverlayTexture.NO_OVERLAY.
layout(push_constant) uniform PushConstants {
    mat4 model;
    layout(offset = 68) float nether_lighting;
    layout(offset = 80) mat3 normal_matrix;
};

layout(location = 0) in vec3 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 2) in vec4 light_tint;
layout(location = 3) in vec4 normal_packed;

layout(location = 0) out vec2 v_tex_coords;
layout(location = 1) out float v_light;
layout(location = 2) out vec3 v_tint;
layout(location = 3) out float v_fog;
layout(location = 4) out vec3 v_fog_color;

// Direct numerical result of 26.2 Lighting.java's item3DPose.transformDirection
// using JOML 1.10.8. Do not replace these with LEVEL/NETHER or (0,1,0) lights.
const vec3 ITEMS_3D_LIGHT_0 = vec3(-0.933439314, -0.262694746, -0.244300187);
const vec3 ITEMS_3D_LIGHT_1 = vec3(-0.103571385, -0.976606905,  0.188446447);

float items_3d_diffuse(vec3 normal) {
    vec2 light = max(vec2(0.0), vec2(
        dot(ITEMS_3D_LIGHT_0, normal),
        dot(ITEMS_3D_LIGHT_1, normal)
    ));
    // 26.2 light.glsl: MINECRAFT_LIGHT_POWER=.6, AMBIENT_LIGHT=.4.
    return min(1.0, (light.x + light.y) * 0.6 + 0.4);
}

void main() {
    vec4 model_pos = model * vec4(position, 1.0);
    vec3 rel = model_pos.xyz - camera_pos.xyz;
    gl_Position = view_proj * vec4(rel, 1.0);

    vec3 item_normal = normalize(normal_matrix * normal_packed.xyz);
    v_tex_coords = tex_coords;
    v_light = items_3d_diffuse(item_normal);
    v_tint = light_tint.gba;
    v_fog = total_fog_value(rel, fog_env, camera_pos.w, fog_color.w);
    v_fog_color = fog_color.rgb;
}
