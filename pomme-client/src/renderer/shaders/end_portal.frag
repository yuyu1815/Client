#version 450
layout(set = 0, binding = 0) uniform CameraUniform {
    mat4 view_proj;
    vec4 camera_pos;
    vec4 fog_color;
    ivec4 camera_block;
    vec4 fog_env;
};
layout(set = 1, binding = 0) uniform sampler2D Sampler0;
layout(set = 1, binding = 1) uniform sampler2D Sampler1;
layout(push_constant) uniform EndPortalPush { float GameTime; uint PORTAL_LAYERS; } push_data;
layout(location = 0) in vec4 tex_proj;
layout(location = 1) in float spherical_distance;
layout(location = 2) in float cylindrical_distance;
layout(location = 0) out vec4 frag_color;
const vec3 COLORS[16] = vec3[16](
 vec3(0.022087,0.098399,0.110818), vec3(0.011892,0.095924,0.089485), vec3(0.027636,0.101689,0.100326), vec3(0.046564,0.109883,0.114838),
 vec3(0.064901,0.117696,0.097189), vec3(0.063761,0.086895,0.123646), vec3(0.084817,0.111994,0.166380), vec3(0.097489,0.154120,0.091064),
 vec3(0.106152,0.131144,0.195191), vec3(0.097721,0.110188,0.187229), vec3(0.133516,0.138278,0.148582), vec3(0.070006,0.243332,0.235792),
 vec3(0.196766,0.142899,0.214696), vec3(0.047281,0.315338,0.321970), vec3(0.204675,0.390010,0.302066), vec3(0.080955,0.314821,0.661491));
mat2 mat2_rotate_z(float angle) { return mat2(cos(angle),-sin(angle),sin(angle),cos(angle)); }
mat4 end_portal_layer(float layer) {
    mat4 translate = mat4(1,0,0,17.0/layer, 0,1,0,(2.0+layer/1.5)*(push_data.GameTime*1.5), 0,0,1,0, 0,0,0,1);
    mat2 rotate = mat2_rotate_z(radians((layer*layer*4321.0+layer*9.0)*2.0));
    mat2 scale = mat2((4.5-layer/4.0)*2.0);
    mat4 scale_translate = mat4(0.5,0,0,0.25, 0,0.5,0,0.25, 0,0,1,0, 0,0,0,1);
    return mat4(scale*rotate)*translate*scale_translate;
}
float linear_fog_value(float d,float start,float end) {
    if(d<=start)return 0.0; if(d>=end)return 1.0; return (d-start)/(end-start);
}
float srgb_to_linear(float c) {
    return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4);
}
float linear_to_srgb(float c) {
    c = max(c, 0.0);
    return c <= 0.0031308 ? c * 12.92 : 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}
void main() {
    vec3 color = textureProj(Sampler0,tex_proj).rgb*COLORS[0];
    for(int i=0;i<int(push_data.PORTAL_LAYERS);i++) color += textureProj(Sampler1,tex_proj*end_portal_layer(float(i+1))).rgb*COLORS[i];
    float fog = max(
        linear_fog_value(spherical_distance, fog_env.x, fog_env.y),
        linear_fog_value(cylindrical_distance, camera_pos.w, fog_color.w)
    );
    vec3 encoded_fog = vec3(linear_to_srgb(fog_color.r), linear_to_srgb(fog_color.g), linear_to_srgb(fog_color.b));
    vec3 encoded = mix(color, encoded_fog, clamp(fog, 0.0, 1.0));
    frag_color=vec4(vec3(srgb_to_linear(encoded.r), srgb_to_linear(encoded.g), srgb_to_linear(encoded.b)),1.0);
}
