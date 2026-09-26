#version 450
layout(set = 1, binding = 0) uniform sampler2DArray gray_atlas;
layout(set = 1, binding = 1) uniform sampler2DArray color_atlas;
layout(location = 0) in vec3 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in int v_colored;
layout(location = 0) out vec4 out_color;
void main() {
    // Solid backgrounds must never sample/discard against an arbitrary atlas texel.
    if (v_colored < 0) {
        out_color = vec4(v_color.rgb * v_color.a, v_color.a);
        return;
    }
    vec4 tex = v_colored != 0 ? texture(color_atlas, v_uv) : vec4(1.0, 1.0, 1.0, texture(gray_atlas, v_uv).r);
    if (tex.a < 0.1) discard;
    // Coverage AND entity/span opacity premultiply RGB for ONE / ONE_MINUS_SRC_ALPHA.
    float alpha = tex.a * v_color.a;
    out_color = vec4(tex.rgb * v_color.rgb * alpha, alpha);
}
