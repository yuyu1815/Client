// The frame camera UBO (set 0, binding 0), shared by the fogged passes; a
// single declaration so the std140 layout can't drift between them. Shaders
// that don't fog declare a shorter prefix of the same buffer. Lane meanings
// live on the Rust struct (camera::CameraUniform).
layout(set = 0, binding = 0) uniform CameraUniform {
    mat4 view_proj;
    vec4 camera_pos;
    vec4 fog_color;
    ivec4 camera_block;
    vec4 fog_env;
    // x: sky darken (0..15), y: ambient light, z: native values supported.
    vec4 terrain_light_environment;
    // Java 26.2 lightmap LUT: block index = x, sky index = y, RGBA8 values.
    vec4 particle_lightmap[256];
};
