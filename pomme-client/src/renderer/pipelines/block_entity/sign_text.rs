use azalea_registry::builtin::BlockEntityKind;
use pomme_gpu_allocator::vulkan::Allocation;
use pyronyx::vk;

use super::BlockEntityRenderInfo;
use crate::ui::font::{GLYPH_ATLAS_SIZE, GlyphMap};

pub(super) const MAX_SIGN_VERTICES: usize = 65536;

fn reset_sign_vertices(vertices: &mut Vec<SignVertex>) {
    vertices.clear();
}
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SignVertex {
    pub(crate) position: [f32; 3],
    pub(crate) uv_layer: [f32; 3],
    pub(crate) color: [f32; 4],
    pub(crate) colored: f32,
}

pub(super) fn push_sign_glyph(
    vertices: &mut Vec<SignVertex>,
    matrix: glam::Mat4,
    gi: &crate::ui::font::GlyphInfo,
    x: f32,
    y: f32,
    color: [f32; 3],
) {
    if gi.pixel_w == 0 || gi.pixel_h == 0 || vertices.len() + 6 > MAX_SIGN_VERTICES {
        return;
    }
    let quad = sign_glyph_quad(gi, x, y, [color[0], color[1], color[2], 1.0]);
    let corners = [quad[0], quad[1], quad[2], quad[5]].map(|mut vertex| {
        vertex.position = matrix
            .transform_point3(glam::Vec3::from_array(vertex.position))
            .to_array();
        vertex
    });
    vertices.extend([0, 1, 2, 0, 2, 3].map(|index| corners[index]));
}

/// Font-pixel triangles shared by signs and CPU TextDisplay extraction.
/// Callers skip non-drawing glyphs and apply their own world transform.
pub(crate) fn sign_glyph_quad(
    gi: &crate::ui::font::GlyphInfo,
    x: f32,
    y: f32,
    color: [f32; 4],
) -> [SignVertex; 6] {
    let x0 = x + gi.left;
    let y0 = y + gi.top;
    let u0 = gi.atlas_x as f32 / GLYPH_ATLAS_SIZE as f32;
    let v0 = gi.atlas_y as f32 / GLYPH_ATLAS_SIZE as f32;
    let u1 = (gi.atlas_x + gi.pixel_w) as f32 / GLYPH_ATLAS_SIZE as f32;
    let v1 = (gi.atlas_y + gi.pixel_h) as f32 / GLYPH_ATLAS_SIZE as f32;
    let corners = [
        (x0, y0, u0, v0),
        (x0, y0 + gi.draw_h, u0, v1),
        (x0 + gi.draw_w, y0 + gi.draw_h, u1, v1),
        (x0 + gi.draw_w, y0, u1, v0),
    ];
    [0, 1, 2, 0, 2, 3].map(|index| {
        let (px, py, u, v) = corners[index];
        SignVertex {
            position: [px, py, 0.0],
            uv_layer: [u, v, gi.atlas_layer as f32],
            color,
            colored: if gi.colored { 1.0 } else { 0.0 },
        }
    })
}

pub(super) fn draw_sign_text(
    device: &vk::Device,
    cmd: vk::CommandBuffer,
    anchor: glam::DVec3,
    eye: glam::DVec3,
    items: &[BlockEntityRenderInfo],
    glyphs: &GlyphMap,
    textures: [vk::DescriptorImageInfo; 2],
    camera_set: vk::DescriptorSet,
    pipeline: vk::Pipeline,
    layout: vk::PipelineLayout,
    text_set: vk::DescriptorSet,
    buffer: vk::Buffer,
    allocation: &mut Allocation,
    text_sets_ready: &mut bool,
    vertices: &mut Vec<SignVertex>,
) -> u32 {
    reset_sign_vertices(vertices);
    for info in items.iter().filter(|i| i.kind == BlockEntityKind::Sign) {
        for (front, lines, dye, glowing) in [
            (
                true,
                &info.sign_front,
                info.sign_front_color,
                info.sign_front_glowing,
            ),
            (
                false,
                &info.sign_back,
                info.sign_back_color,
                info.sign_back_glowing,
            ),
        ] {
            let Some(lines) = lines else {
                continue;
            };
            let base = (glam::DVec3::new(info.pos.x as f64, info.pos.y as f64, info.pos.z as f64)
                - anchor)
                .as_vec3();
            // StandingSignRenderer.textTransformation, including wall offset,
            // back-face rotation and the inverted Y of Font coordinates.
            let matrix = glam::Mat4::from_translation(base + glam::Vec3::splat(0.5))
                * glam::Mat4::from_rotation_y((-info.yaw).to_radians())
                * glam::Mat4::from_translation(if info.sign_wall {
                    glam::Vec3::new(0.0, -0.3125, -0.4375)
                } else {
                    glam::Vec3::ZERO
                })
                * glam::Mat4::from_rotation_y(if front { 0.0 } else { std::f32::consts::PI })
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, 1.0 / 3.0, 0.046666667))
                * glam::Mat4::from_scale(glam::Vec3::new(1.0 / 96.0, -1.0 / 96.0, 1.0 / 96.0));
            let black = dye == [29.0 / 255.0, 29.0 / 255.0, 33.0 / 255.0];
            let dark = if black && glowing {
                [0.941, 0.922, 0.922]
            } else {
                dye.map(|c| c * 0.4)
            };
            let color = if glowing {
                dye
            } else {
                dark.map(|c| c * info.sign_light)
            };
            let near = (glam::DVec3::new(
                info.pos.x as f64 + 0.5,
                info.pos.y as f64 + 0.5,
                info.pos.z as f64 + 0.5,
            ) - eye)
                .length_squared()
                < 256.0;
            let outline = glowing && (black || near);
            for (row, line) in lines.iter().enumerate() {
                // Vanilla SignBlockEntity: 90 px line width, 10 px height.
                let chars: Vec<_> = line
                    .chars()
                    .take(256)
                    .scan(0.0f32, |width, ch| {
                        let gi = glyphs.glyph(ch, None);
                        if *width + gi.advance > 90.0 {
                            return None;
                        }
                        let x = *width;
                        *width += gi.advance;
                        Some((x, gi))
                    })
                    .collect();
                let width: f32 = chars.last().map_or(0.0, |(x, gi)| x + gi.advance);
                let y = row as f32 * 10.0 - 20.0;
                if outline {
                    for (x, gi) in &chars {
                        for dy in -1..=1 {
                            for dx in -1..=1 {
                                if dx != 0 || dy != 0 {
                                    push_sign_glyph(
                                        vertices,
                                        matrix,
                                        gi,
                                        *x - width / 2.0 + dx as f32 * 0.5,
                                        y + dy as f32 * 0.5,
                                        dark,
                                    );
                                }
                            }
                        }
                    }
                }
                for (x, gi) in &chars {
                    push_sign_glyph(vertices, matrix, gi, *x - width / 2.0, y, color);
                }
            }
        }
    }
    if vertices.is_empty() {
        return 0;
    }
    let len = vertices.len().min(MAX_SIGN_VERTICES);
    let len = len - len % 6;
    let bytes = bytemuck::cast_slice(&vertices[..len]);
    allocation.mapped_slice_mut().unwrap()[..bytes.len()].copy_from_slice(bytes);
    if !*text_sets_ready {
        let writes = super::world_font_writes(text_set, &textures);
        device.update_descriptor_sets(&writes, &[]);
        *text_sets_ready = true;
    }
    cmd.bind_pipeline(vk::PipelineBindPoint::Graphics, pipeline);
    cmd.bind_descriptor_sets(
        vk::PipelineBindPoint::Graphics,
        layout,
        0,
        &[camera_set, text_set],
        &[],
    );
    cmd.bind_vertex_buffers(0, &[buffer], &[0]);
    cmd.draw(len as u32, 1, 0, 0);
    len as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transformed_sign_glyph_matches_six_vertex_reference_for_both_faces() {
        let glyph = crate::ui::font::GlyphInfo {
            atlas_layer: 2,
            colored: true,
            atlas_x: 8,
            atlas_y: 16,
            pixel_w: 4,
            pixel_h: 7,
            draw_w: 4.0,
            draw_h: 7.0,
            left: 1.0,
            top: 2.0,
            advance: 5.0,
            bold_offset: 1.0,
            shadow_offset: 1.0,
        };
        let matrix = glam::Mat4::from_translation(glam::Vec3::new(2.0, 3.0, 4.0))
            * glam::Mat4::from_rotation_y(0.37);
        for color in [[1.0, 0.5, 0.0], [0.2, 0.4, 0.8]] {
            let reference: Vec<_> =
                sign_glyph_quad(&glyph, 10.0, 20.0, [color[0], color[1], color[2], 1.0])
                    .map(|mut vertex| {
                        vertex.position = matrix
                            .transform_point3(glam::Vec3::from_array(vertex.position))
                            .to_array();
                        vertex
                    })
                    .into();
            let mut actual = Vec::new();
            push_sign_glyph(&mut actual, matrix, &glyph, 10.0, 20.0, color);
            assert_eq!(actual, reference);
        }
    }

    #[test]
    fn reusable_sign_vertices_preserve_order_and_limit() {
        let glyph = crate::ui::font::GlyphInfo {
            atlas_layer: 2,
            colored: true,
            atlas_x: 8,
            atlas_y: 16,
            pixel_w: 4,
            pixel_h: 7,
            draw_w: 4.0,
            draw_h: 7.0,
            left: 1.0,
            top: 2.0,
            advance: 5.0,
            bold_offset: 1.0,
            shadow_offset: 1.0,
        };
        let matrix = glam::Mat4::from_translation(glam::Vec3::new(2.0, 3.0, 4.0));
        let mut vertices = Vec::new();
        for x in [10.0, 20.0] {
            reset_sign_vertices(&mut vertices);
            push_sign_glyph(&mut vertices, matrix, &glyph, x, 30.0, [1.0, 0.5, 0.0]);
            let first = vertices.clone();
            reset_sign_vertices(&mut vertices);
            push_sign_glyph(&mut vertices, matrix, &glyph, x, 30.0, [1.0, 0.5, 0.0]);
            assert_eq!(vertices, first);
            assert_eq!(vertices.len(), 6);
        }

        reset_sign_vertices(&mut vertices);
        for _ in 0..(MAX_SIGN_VERTICES / 6 + 1) {
            push_sign_glyph(&mut vertices, matrix, &glyph, 0.0, 0.0, [1.0; 3]);
        }
        assert_eq!(vertices.len(), MAX_SIGN_VERTICES - MAX_SIGN_VERTICES % 6);
        assert_eq!(vertices.len(), 65532);
    }

    #[test]
    fn glyph_quad_uses_atlas_layer_and_world_matrix() {
        let glyph = crate::ui::font::GlyphInfo {
            atlas_layer: 2,
            colored: false,
            atlas_x: 8,
            atlas_y: 16,
            pixel_w: 4,
            pixel_h: 7,
            draw_w: 4.0,
            draw_h: 7.0,
            left: 1.0,
            top: 2.0,
            advance: 5.0,
            bold_offset: 1.0,
            shadow_offset: 1.0,
        };
        let mut vertices = Vec::new();
        push_sign_glyph(
            &mut vertices,
            glam::Mat4::from_translation(glam::Vec3::new(2.0, 3.0, 4.0)),
            &glyph,
            10.0,
            20.0,
            [1.0, 0.5, 0.0],
        );
        assert_eq!(vertices.len(), 6);
        assert_eq!(vertices[0].position, [13.0, 25.0, 4.0]);
        assert_eq!(vertices[0].uv_layer, [8.0 / 2048.0, 16.0 / 2048.0, 2.0]);
        assert_eq!(vertices[2].position, [17.0, 32.0, 4.0]);
    }
}
