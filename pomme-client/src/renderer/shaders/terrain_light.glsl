// Packed terrain byte: low nibble sky, high nibble block. Four bytes are
// the four raw samples belonging to this vertex, not a quad-wide average.
const float TERRAIN_OLD_LIGHT[16] = float[16](
    0.05, 0.067, 0.085, 0.106, 0.129, 0.156, 0.188, 0.227,
    0.272, 0.328, 0.393, 0.472, 0.566, 0.679, 0.815, 1.0
);

float terrain_native_brightness(uint pair, float sky_darken, float ambient) {
    uint sky = pair & 15u;
    uint block = (pair >> 4u) & 15u;
    uint darkened_sky = uint(max(int(sky) - int(clamp(sky_darken, 0.0, 15.0)), 0));
    uint level = max(darkened_sky, block);
    float v = TERRAIN_OLD_LIGHT[level];
    float curved = v / (4.0 - 3.0 * v);
    return curved + ambient * (1.0 - curved);
}

float terrain_vertex_light_ratio(uint samples, vec4 environment) {
    if (environment.z < 0.5) {
        return 1.0;
    }
    float old_mean = 0.0;
    float native_mean = 0.0;
    for (uint i = 0u; i < 4u; ++i) {
        uint pair = (samples >> (i * 8u)) & 255u;
        uint level = max(pair & 15u, (pair >> 4u) & 15u);
        old_mean += TERRAIN_OLD_LIGHT[level];
        native_mean += terrain_native_brightness(pair, environment.x, environment.y);
    }
    old_mean *= 0.25;
    native_mean *= 0.25;
    return old_mean > 0.0 ? native_mean / old_mean : 1.0;
}
