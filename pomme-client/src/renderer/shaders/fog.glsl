// Vanilla's two-band fog (core include/fog.glsl): the render-distance band
// rides in the camera UBO's spare .w lanes (cylindrical, the last
// `clamp(blocks/10, 4, 64)` blocks), the RD-independent environmental band in
// `fog_env.xy` (spherical, the ambient haze); whichever is denser wins.
float linear_fog_value(float dist, float fog_start, float fog_end) {
    if (dist <= fog_start) {
        return 0.0;
    }
    if (dist >= fog_end) {
        return 1.0;
    }
    return (dist - fog_start) / (fog_end - fog_start);
}

float total_fog_value(vec3 rel, vec4 fog_env, float rd_start, float rd_end) {
    float spherical = length(rel);
    float cylindrical = max(length(rel.xz), abs(rel.y));
    return max(
        linear_fog_value(spherical, fog_env.x, fog_env.y),
        linear_fog_value(cylindrical, rd_start, rd_end)
    );
}

vec3 apply_fog(vec3 color, float fog, vec3 fog_color) {
    return mix(color, fog_color, clamp(fog, 0.0, 1.0));
}

vec3 terrain_srgb_to_linear(vec3 c) {
    bvec3 low = lessThanEqual(c, vec3(0.04045));
    vec3 a = c / 12.92;
    vec3 b = pow((c + 0.055) / 1.055, vec3(2.4));
    return mix(b, a, low);
}

vec3 terrain_linear_to_srgb(vec3 c) {
    c = max(c, vec3(0.0));
    bvec3 low = lessThanEqual(c, vec3(0.0031308));
    vec3 a = c * 12.92;
    vec3 b = 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055;
    return mix(b, a, low);
}

// Java 26.2 terrain.fsh multiplies encoded UNORM samples/color/light and fog.
// Vulkan's SRGB atlas/target require a bridge around that encoded-space math.
vec3 shade_chunk_surface(
    vec3 tex_rgb,
    vec3 tint,
    float light,
    float visibility,
    vec3 fog_color,
    float fog
) {
    vec3 encoded = terrain_linear_to_srgb(tex_rgb) * tint * light;
    if (visibility < 1.0) {
        encoded = mix(fog_color, encoded, visibility);
    }
    encoded = apply_fog(encoded, fog, fog_color);
    return terrain_srgb_to_linear(encoded);
}
