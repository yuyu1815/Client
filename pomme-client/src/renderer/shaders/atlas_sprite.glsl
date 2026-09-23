// Level-0 sprite rectangles `(x, y, width, height)` in atlas texels, indexed by
// the vertex's sprite id; index 0 is the missing tile.
layout(set = 1, binding = 1) readonly buffer SpriteRects {
    uvec4 sprite_rects[];
};

vec2 atlas_sprite_uv(sampler2D atlas_texture, vec2 sprite_uv, uint sprite, out vec2 atlas_size, out vec2 sprite_size) {
    uvec4 rect = sprite_rects[sprite];
    atlas_size = vec2(textureSize(atlas_texture, 0));
    sprite_size = vec2(rect.zw);
    return (vec2(rect.xy) + fract(sprite_uv) * sprite_size) / atlas_size;
}

vec4 sample_atlas_sprite(sampler2D atlas_texture, vec2 sprite_uv, uint sprite) {
    vec2 atlas_size;
    vec2 sprite_size;
    vec2 atlas_uv = atlas_sprite_uv(atlas_texture, sprite_uv, sprite, atlas_size, sprite_size);
    vec2 atlas_scale = sprite_size / atlas_size;
    return textureGrad(atlas_texture, atlas_uv, dFdx(sprite_uv) * atlas_scale, dFdy(sprite_uv) * atlas_scale);
}

// 26.2 terrain.fsh sampleRGSS/sampleNearest, using the observed UseRgss=1 path.
vec4 sample_atlas_sprite_rgss(sampler2D atlas_texture, vec2 sprite_uv, uint sprite) {
    vec2 atlas_size;
    vec2 sprite_size;
    vec2 uv = atlas_sprite_uv(atlas_texture, sprite_uv, sprite, atlas_size, sprite_size);
    vec2 pixel_size = 1.0 / atlas_size;
    vec2 du = dFdx(uv);
    vec2 dv = dFdy(uv);
    vec2 texel_screen_size = sqrt(du * du + dv * dv);

    vec2 texel_coords = uv / pixel_size;
    vec2 texel_center = round(texel_coords) - 0.5;
    vec2 texel_offset = texel_coords - texel_center;
    texel_offset = clamp((texel_offset - 0.5) * pixel_size / texel_screen_size + 0.5, 0.0, 1.0);
    vec4 nearest = textureGrad(atlas_texture, (texel_center + texel_offset) * pixel_size, du, dv);

    float max_texel_size = max(texel_screen_size.x, texel_screen_size.y);
    float min_pixel_size = min(pixel_size.x, pixel_size.y);
    float blend_factor = smoothstep(min_pixel_size, min_pixel_size * 2.0, max_texel_size);
    float min_derivative = min(length(du), length(dv));
    float max_derivative = max(length(du), length(dv));
    float mip_exact = max(0.0, log2(sqrt(min_derivative * max_derivative) / min_pixel_size));
    float mip_low = floor(mip_exact);
    float mip_high = mip_low + 1.0;
    float mip_blend = fract(mip_exact);
    const vec2 offsets[4] = vec2[](vec2(0.125, 0.375), vec2(-0.125, -0.375), vec2(0.375, -0.125), vec2(-0.375, 0.125));
    vec4 low = vec4(0.0);
    vec4 high = vec4(0.0);
    for (int i = 0; i < 4; ++i) {
        vec2 sample_uv = uv + offsets[i] * pixel_size;
        low += textureLod(atlas_texture, sample_uv, mip_low);
        high += textureLod(atlas_texture, sample_uv, mip_high);
    }
    vec4 rgss = mix(low * 0.25, high * 0.25, mip_blend);
    return mix(nearest, rgss, blend_factor);
}
