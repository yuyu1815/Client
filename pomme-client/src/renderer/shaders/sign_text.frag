#version 450
layout(set = 1, binding = 0) uniform sampler2DArray gray_atlas;
layout(set = 1, binding = 1) uniform sampler2DArray color_atlas;
layout(location = 0) in vec3 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) flat in int v_colored;
layout(location = 0) out vec4 out_color;
void main() {
    vec4 tex = v_colored != 0 ? texture(color_atlas, v_uv) : vec4(1.0, 1.0, 1.0, texture(gray_atlas, v_uv).r);
    if (tex.a < 0.1) discard;
    // Premultiplied alpha: keeps translucent glyph edges clean over the sign.
    out_color = vec4(tex.rgb * v_color.rgb * tex.a, tex.a * v_color.a);
}
