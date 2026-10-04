#version 450
layout(set = 0, binding = 0) uniform CameraUniform {
    mat4 view_proj;
    vec4 camera_pos;
    vec4 fog_color;
};
layout(location = 0) in vec3 position;
layout(location = 0) out vec4 tex_proj;
layout(location = 1) out float spherical_distance;
layout(location = 2) out float cylindrical_distance;
void main() {
    vec3 view_position = position - camera_pos.xyz;
    gl_Position = view_proj * vec4(view_position, 1.0);
    vec4 projection = gl_Position * 0.5;
    // Camera::view_projection flips clip Y for Vulkan; vanilla's projective
    // UV convention is defined before that Vulkan-only correction.
    projection.y = -projection.y;
    projection.xy = vec2(projection.x + projection.w, projection.y + projection.w);
    projection.zw = gl_Position.zw;
    tex_proj = projection;
    spherical_distance = length(view_position);
    cylindrical_distance = max(length(view_position.xz), abs(view_position.y));
}
