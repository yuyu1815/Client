#version 450

layout(set = 0, binding = 0) uniform Globals {
    float screen_w;
    float screen_h;
};

layout(set = 1, binding = 0) uniform sampler2D font_tex;
layout(set = 1, binding = 1) uniform sampler2D sprite_tex;
layout(set = 1, binding = 2) uniform sampler2D item_tex;
layout(set = 1, binding = 3) uniform sampler2DArray mc_font_tex;
layout(set = 1, binding = 4) uniform sampler2D blur_tex;
layout(set = 1, binding = 5) uniform sampler2D favicon_tex;
layout(set = 1, binding = 6) uniform sampler2D overlay_tex;
layout(set = 1, binding = 7) uniform sampler2D underwater_tex;
layout(set = 1, binding = 8) uniform sampler2DArray mc_font_color_tex;

layout(location = 0) in vec2 v_uv;
layout(location = 1) in vec4 v_color;
layout(location = 2) in float v_mode;
layout(location = 3) in vec2 v_rect_size;
layout(location = 4) in float v_corner_radius;

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

float sdf_rounded_rect(vec2 p, vec2 half_size, float radius) {
    vec2 q = abs(p) - half_size + vec2(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2(0.0))) - radius;
}

void main() {
    if (v_mode > 9.5) {
        // Plain premultiplied fill; color pre-converted CPU-side (sleep fade).
        out_color = v_color;
        return;
    }

    if (v_mode > 8.5) {
        // Underwater tint: standard alpha blend over the scene.
        vec4 tex = texture(underwater_tex, v_uv);
        out_color = vec4(tex.rgb * v_color.rgb * tex.a * v_color.a, tex.a * v_color.a);
        return;
    }

    if (v_mode > 7.5) {
        // Vignette. Vanilla blends (ZERO, ONE_MINUS_SRC_COLOR): each channel of
        // dst is scaled by 1 - src, where src = grayscale texture * brightness,
        // in the gamma-space GL framebuffer. Emitting (0, 0, 0, a) through the
        // premultiplied One / OneMinusSrcAlpha blend scales dst by 1 - a, and
        // converting the gamma-space factor to linear keeps the darkening
        // identical on the sRGB target. (dst alpha is scaled too, unlike
        // vanilla; the swapchain composite ignores alpha.)
        // R8G8B8A8_SRGB sampling hardware-decodes the native encoded PNG.
        // Vanilla multiplies the encoded sample by the encoded vertex tint, so
        // recover that value with the exact inverse transfer before multiplying.
        float tex_gamma = linear_to_srgb_exact(texture(overlay_tex, v_uv).r);
        float src = tex_gamma * v_color.r;
        out_color = vec4(0.0, 0.0, 0.0, 1.0 - pow(1.0 - src, 2.2));
        return;
    }

    if (v_mode > 6.5) {
        // Camera overlay (pumpkin blur): standard alpha blend.
        vec4 tex = texture(overlay_tex, v_uv);
        out_color = vec4(tex.rgb * v_color.rgb * tex.a * v_color.a, tex.a * v_color.a);
        return;
    }

    if (v_mode > 5.5) {
        // The tint is gamma-space like glyph colors; linearize it the same way.
        vec4 tex = texture(favicon_tex, v_uv);
        vec3 tint = pow(v_color.rgb, vec3(2.2));
        out_color = vec4(tex.rgb * tint * tex.a * v_color.a, tex.a * v_color.a);
        return;
    }

    if (v_mode > 4.5) {
        vec2 local = (v_uv - 0.5) * v_rect_size;
        vec2 half_s = v_rect_size * 0.5;
        float d = sdf_rounded_rect(local, half_s, v_corner_radius);
        float alpha = 1.0 - smoothstep(-1.0, 1.0, d);

        vec2 screen_uv = gl_FragCoord.xy / vec2(screen_w, screen_h);
        vec4 blurred = texture(blur_tex, screen_uv);

        vec3 tinted = blurred.rgb * v_color.rgb;
        float a = v_color.a * alpha;

        float border_band = smoothstep(-2.0, 0.0, d) * alpha;
        vec4 border_color = vec4(1.0, 1.0, 1.0, 0.06);

        vec4 col = vec4(tinted * a, a);
        col = col + border_color * border_band * (1.0 - col.a);

        out_color = col;
        return;
    }

    if (v_mode > 3.5) {
        vec3 linear_color = pow(v_color.rgb, vec3(2.2));
        // Colored glyphs are mode 4.25, gray 4.0.
        if (v_mode > 4.125) {
            vec4 tex = texture(mc_font_color_tex, vec3(v_uv, v_rect_size.x));
            out_color = vec4(tex.rgb * linear_color * tex.a * v_color.a, tex.a * v_color.a);
        } else {
            float coverage = texture(mc_font_tex, vec3(v_uv, v_rect_size.x)).r;
            out_color = vec4(linear_color * coverage * v_color.a, coverage * v_color.a);
        }
        return;
    }

    if (v_mode > 2.5) {
        vec4 tex = texture(item_tex, v_uv);
        vec3 linear = vec3(
            srgb_to_linear_exact(tex.r),
            srgb_to_linear_exact(tex.g),
            srgb_to_linear_exact(tex.b)
        );
        out_color = vec4(linear * v_color.rgb * v_color.a, tex.a * v_color.a);
        return;
    }

    if (v_mode > 1.5) {
        vec4 tex = texture(sprite_tex, v_uv);
        out_color = vec4(tex.rgb * v_color.rgb * tex.a * v_color.a, tex.a * v_color.a);
        return;
    }

    if (v_mode > 0.5) {
        vec4 tex = texture(font_tex, v_uv);
        out_color = vec4(v_color.rgb * tex.a, v_color.a * tex.a);
        return;
    }

    vec2 local = (v_uv - 0.5) * v_rect_size;
    vec2 half_s = v_rect_size * 0.5;
    float d = sdf_rounded_rect(local, half_s, v_corner_radius);

    float alpha = 1.0 - smoothstep(-1.0, 1.0, d);

    float border_band = smoothstep(-2.0, 0.0, d) * alpha;
    vec4 border_color = vec4(0.12, 0.12, 0.12, 0.12);

    vec4 premul = vec4(v_color.rgb * v_color.a, v_color.a);
    vec4 col = premul * alpha;
    col = col + border_color * border_band * (1.0 - col.a);

    out_color = col;
}
