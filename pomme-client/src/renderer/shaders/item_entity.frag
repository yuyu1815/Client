#version 450

#include "fog.glsl"

layout(set = 1, binding = 0) uniform sampler2D atlas_texture;

layout(push_constant) uniform PushConstants {
    layout(offset = 64) float world_light;
};

layout(location = 0) in vec2 v_tex_coords;
layout(location = 1) in float v_light;
layout(location = 2) in vec3 v_tint;
layout(location = 3) in float v_fog;
layout(location = 4) in vec3 v_fog_color;

layout(location = 0) out vec4 out_color;

float srgb_to_linear_exact(float value) {
    return value <= 0.04045
        ? value / 12.92
        : pow((value + 0.055) / 1.055, 2.4);
}

float linear_to_srgb_exact(float value) {
    return value <= 0.0031308
        ? value * 12.92
        : 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

void main() {
    vec4 color = texture(atlas_texture, v_tex_coords);
    // Vanilla ITEM_CUTOUT and ITEM_TRANSLUCENT both use ALPHA_CUTOUT=0.1.
    if (color.a < 0.1) discard;

    // The item atlas is SRGB, so the hardware sample is linear. Vanilla's
    // GUI item atlas and main target are UNORM and multiply encoded bytes.
    // Restore that encoded value for the tint/light operation, then return
    // linear data for Rust's SRGB swapchain (or the UNORM GUI bake atlas).
    vec3 encoded = vec3(
        linear_to_srgb_exact(color.r),
        linear_to_srgb_exact(color.g),
        linear_to_srgb_exact(color.b)
    );
    encoded *= v_tint * (world_light * v_light);
    encoded = apply_fog(encoded, v_fog, v_fog_color);
    vec3 linear = vec3(
        srgb_to_linear_exact(encoded.r),
        srgb_to_linear_exact(encoded.g),
        srgb_to_linear_exact(encoded.b)
    );
    out_color = vec4(linear, color.a);
}
