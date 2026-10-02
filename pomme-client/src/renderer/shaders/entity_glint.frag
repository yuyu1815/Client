#version 450

layout(set = 1, binding = 0) uniform sampler2D glint_tex;
layout(location = 0) in vec2 v_tex_coords;
layout(location = 1) in vec4 v_color_modulator;
layout(location = 2) in float v_fog;
layout(location = 0) out vec4 out_color;

void main() {
    vec4 color = texture(glint_tex, v_tex_coords) * v_color_modulator;
    if (color.a < 0.1) discard;
    // Native Options.glintStrength default / GlobalSettingsUniform.GlintAlpha.
    float fade = (1.0 - clamp(v_fog, 0.0, 1.0)) * 0.75;
    out_color = vec4(color.rgb * fade, color.a);
}
