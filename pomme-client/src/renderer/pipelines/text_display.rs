//! CPU world-space TextDisplay geometry; block_entity owns its GPU resources.
//! Renderer extracts once per world frame using the current camera and font
//! atlas.
use glam::{DMat4, DQuat, DVec3, Vec3};

use super::block_entity::sign_text::{SignVertex, sign_glyph_quad};
use super::menu_overlay::split_text_display_lines;
use crate::entity::{EntityStore, VehicleState};
use crate::ui::font::{GlyphInfo, GlyphMap};
use crate::ui::text::TextSpan;

/// Exactly the sign shader's position / UV + atlas layer / RGBA / colored
/// layout.
pub(crate) type TextDisplayVertex = SignVertex;

/// Solid geometry is deliberately separate: it must not sample the font atlas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextDisplayBackgroundVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextDisplayDraw {
    pub entity_id: i32,
    /// Camera-anchor-relative world positions, not screen-space rectangles.
    pub vertices: Vec<TextDisplayVertex>,
    pub shadow_vertices: Vec<TextDisplayVertex>,
    pub background_vertices: Vec<TextDisplayBackgroundVertex>,
    /// Selects depth-tested or depth-test-disabled (both depth-write-disabled).
    pub see_through: bool,
    pub shadow: bool,
}

impl TextDisplayDraw {
    /// One ordered triangle stream: solid background, shadows, then glyphs.
    /// `colored = -1` is the TextDisplay-only solid mode; sign glyphs stay 0/1.
    pub(crate) fn gpu_vertices(&self) -> Vec<SignVertex> {
        self.background_vertices
            .iter()
            .map(|vertex| SignVertex {
                position: vertex.position,
                uv_layer: [0.0; 3],
                color: vertex.color,
                colored: -1.0,
            })
            .chain(self.shadow_vertices.iter().copied())
            .chain(self.vertices.iter().copied())
            .collect()
    }
}

pub(crate) struct TextDisplayFlags;
impl TextDisplayFlags {
    pub const SHADOW: u8 = 0x01;
    pub const SEE_THROUGH: u8 = 0x02;
    pub const DEFAULT_BACKGROUND: u8 = 0x04;
    pub const ALIGN_LEFT: u8 = 0x08;
    pub const ALIGN_RIGHT: u8 = 0x10;
}

/// Maps vanilla alignment flag bits to left (0), center (1), or right (2).
pub fn text_display_alignment(flags: u8) -> u8 {
    if flags & TextDisplayFlags::ALIGN_LEFT != 0 {
        0
    } else if flags & TextDisplayFlags::ALIGN_RIGHT != 0 {
        2
    } else {
        1
    }
}

fn quat(q: [f32; 4]) -> Option<DQuat> {
    let q = DQuat::from_xyzw(q[0] as f64, q[1] as f64, q[2] as f64, q[3] as f64);
    (q.is_finite() && q.length_squared() > 0.0).then(|| q.normalize())
}

fn billboard(mode: u8, yaw: f32, pitch: f32, cy: f32, cp: f32) -> Option<DQuat> {
    if ![yaw, pitch, cy, cp].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (y, p) = match mode {
        0 => (-f64::from(yaw), f64::from(pitch)),
        1 => (180.0 - f64::from(cy), f64::from(pitch)),
        2 => (-f64::from(yaw), -f64::from(cp)),
        3 => (180.0 - f64::from(cy), -f64::from(cp)),
        _ => return None,
    };
    Some(DQuat::from_rotation_y(y.to_radians()) * DQuat::from_rotation_x(p.to_radians()))
}

fn text_matrix(e: &VehicleState, anchor: DVec3, cy: f32, cp: f32) -> Option<DMat4> {
    // Subtract in f64 before any f32 conversion (large multiplayer coordinates).
    let p = *e.position - anchor;
    let t = Vec3::from_array(e.text_display_translation).as_dvec3();
    let s = Vec3::from_array(e.text_display_scale).as_dvec3();
    if !anchor.is_finite() || !p.is_finite() || !t.is_finite() || !s.is_finite() {
        return None;
    }
    let look = e.look_dir.as_ref()?;
    let matrix = DMat4::from_translation(p)
        * DMat4::from_quat(billboard(
            e.text_display_billboard,
            look.y_rot_deg(),
            look.x_rot_deg(),
            cy,
            cp,
        )?)
        * DMat4::from_translation(t)
        * DMat4::from_quat(quat(e.text_display_left_rotation)?)
        * DMat4::from_scale(s)
        * DMat4::from_quat(quat(e.text_display_right_rotation)?)
        * DMat4::from_rotation_y(std::f64::consts::PI)
        * DMat4::from_scale(DVec3::splat(-0.025));
    matrix.is_finite().then_some(matrix)
}

fn world_position(matrix: DMat4, position: [f32; 3]) -> Option<[f32; 3]> {
    let position = matrix
        .transform_point3(Vec3::from_array(position).as_dvec3())
        .as_vec3();
    position.is_finite().then(|| position.to_array())
}

fn advance(span: &TextSpan, ch: char, glyph: &GlyphInfo) -> f32 {
    // Match split_text_display_lines even for currently unsupported inline sprites.
    if ch == '\u{fffc}' && span.inline_object.is_some() {
        8.0 + if span.bold { 1.0 } else { 0.0 }
    } else {
        glyph.advance + if span.bold { glyph.bold_offset } else { 0.0 }
    }
}

fn push_glyph(
    vertices: &mut Vec<TextDisplayVertex>,
    matrix: DMat4,
    glyph: &GlyphInfo,
    origin: [f32; 3],
    span: &TextSpan,
    color: [f32; 4],
) -> Option<()> {
    if !color.iter().all(|v| v.is_finite()) {
        return None;
    }
    if glyph.pixel_w == 0 || glyph.pixel_h == 0 || color[3] <= 0.0 {
        return Some(());
    }
    for copy in 0..=usize::from(span.bold) {
        for mut vertex in sign_glyph_quad(glyph, origin[0], origin[1], color) {
            if span.italic {
                vertex.position[0] += 1.0 - 0.25 * (vertex.position[1] - origin[1]);
            }
            vertex.position[0] += copy as f32 * glyph.bold_offset;
            vertex.position[2] = origin[2];
            vertex.position = world_position(matrix, vertex.position)?;
            vertices.push(vertex);
        }
    }
    Some(())
}

fn text_display_background_color(e: &VehicleState) -> [f32; 4] {
    let argb = if e.text_display_flags & TextDisplayFlags::DEFAULT_BACKGROUND != 0 {
        0x4000_0000
    } else {
        e.text_display_background
    };
    [
        ((argb >> 16) & 0xff) as f32 / 255.0,
        ((argb >> 8) & 0xff) as f32 / 255.0,
        (argb & 0xff) as f32 / 255.0,
        (argb >> 24) as f32 / 255.0 * (f32::from(e.text_display_opacity) / 255.0),
    ]
}

/// Build real atlas-backed triangles. Camera angles are render-view degrees,
/// including third-person view changes, NOT the local player's body rotation.
/// FIXED orientation comes from each entity's own spawn/update look direction.
/// Invalid transforms discard the entire entity, never a partial triangle.
pub(crate) fn extract_text_displays(
    store: &EntityStore,
    camera_anchor: DVec3,
    camera_yaw: f32,
    camera_pitch: f32,
    font: &GlyphMap,
) -> Vec<TextDisplayDraw> {
    store
        .vehicles
        .iter()
        .filter_map(|(&id, entity)| {
            if entity.kind != Some(azalea_registry::builtin::EntityKind::TextDisplay) {
                return None;
            }
            extract_display(id, entity, camera_anchor, camera_yaw, camera_pitch, font)
        })
        .collect()
}

fn extract_display(
    entity_id: i32,
    e: &VehicleState,
    anchor: DVec3,
    cy: f32,
    cp: f32,
    font: &GlyphMap,
) -> Option<TextDisplayDraw> {
    let matrix = text_matrix(e, anchor, cy, cp)?;
    let lines = split_text_display_lines(
        e.text_display_text.as_ref()?,
        e.text_display_line_width as f32,
        font,
    );
    let widths: Vec<f32> = lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| {
                    span.text
                        .chars()
                        .map(|ch| advance(span, ch, font.glyph(ch, span.font.as_deref())))
                        .sum::<f32>()
                })
                .sum()
        })
        .collect();
    if lines.is_empty() || !widths.iter().all(|w| w.is_finite()) {
        return None;
    }
    let width = widths.iter().copied().fold(0.0f32, f32::max);
    let height = lines.len() as f32 * 10.0;
    let mut draw = TextDisplayDraw {
        entity_id,
        vertices: Vec::new(),
        shadow_vertices: Vec::new(),
        background_vertices: Vec::new(),
        see_through: e.text_display_flags & TextDisplayFlags::SEE_THROUGH != 0,
        shadow: e.text_display_flags & TextDisplayFlags::SHADOW != 0,
    };
    let background = text_display_background_color(e);
    if background[3] > 0.0 {
        // One font-pixel horizontal padding, 10px per row. Negative local Z is
        // behind the text after the prescribed Ry(pi) * S(-.025) transform.
        let corners = [
            [-width / 2.0 - 1.0, -height - 1.0, -0.01],
            [-width / 2.0 - 1.0, -1.0, -0.01],
            [width / 2.0 + 1.0, -1.0, -0.01],
            [width / 2.0 + 1.0, -height - 1.0, -0.01],
        ];
        for index in [0, 1, 2, 0, 2, 3] {
            draw.background_vertices.push(TextDisplayBackgroundVertex {
                position: world_position(matrix, corners[index])?,
                color: background,
            });
        }
    }
    let opacity = f32::from(e.text_display_opacity) / 255.0;
    for (row, (line, line_width)) in lines.iter().zip(&widths).enumerate() {
        let mut x = match text_display_alignment(e.text_display_flags) {
            0 => -width / 2.0,
            2 => width / 2.0 - line_width,
            _ => -line_width / 2.0,
        };
        let y = -height + row as f32 * 10.0;
        for span in line {
            let mut color = span.color;
            color[3] *= opacity;
            let shadow_color = match span.shadow_color {
                Some(mut explicit) => {
                    explicit[3] *= color[3];
                    Some(explicit)
                }
                None => draw.shadow.then_some([
                    color[0] * 0.25,
                    color[1] * 0.25,
                    color[2] * 0.25,
                    color[3],
                ]),
            };
            // ponytail: underline/strike, animated obfuscation and inline sprites
            // have no world geometry yet; add it when extending rich-text parity.
            for ch in span.text.chars() {
                let glyph = font.glyph(ch, span.font.as_deref());
                if let Some(shadow_color) = shadow_color {
                    push_glyph(
                        &mut draw.shadow_vertices,
                        matrix,
                        glyph,
                        [x + glyph.shadow_offset, y + glyph.shadow_offset, -0.005],
                        span,
                        shadow_color,
                    )?;
                }
                push_glyph(&mut draw.vertices, matrix, glyph, [x, y, 0.0], span, color)?;
                x += advance(span, ch, glyph);
            }
        }
    }
    Some(draw)
}

#[cfg(test)]
mod tests {
    use azalea_registry::builtin::EntityKind;

    use super::*;
    use crate::entity::components::{LookDirection, Position};

    fn font() -> GlyphMap {
        let a = GlyphInfo {
            atlas_layer: 2,
            colored: false,
            atlas_x: 16,
            atlas_y: 32,
            pixel_w: 5,
            pixel_h: 8,
            draw_w: 5.0,
            draw_h: 8.0,
            left: 1.0,
            top: 2.0,
            advance: 6.0,
            bold_offset: 1.0,
            shadow_offset: 1.0,
        };
        let b = GlyphInfo {
            atlas_layer: 3,
            colored: true,
            atlas_x: 64,
            atlas_y: 96,
            pixel_w: 7,
            draw_w: 7.0,
            advance: 8.0,
            ..a.clone()
        };
        let space = GlyphInfo {
            pixel_w: 0,
            pixel_h: 0,
            advance: 4.0,
            ..a.clone()
        };
        GlyphMap::with_test_glyphs(&[
            ("minecraft:default", 'A', a),
            ("minecraft:default", ' ', space),
            ("test:color", 'B', b),
        ])
    }

    fn store() -> EntityStore {
        let mut store = EntityStore::new();
        store.set_vehicle_spawn_transform(
            7,
            Position::default(),
            DVec3::ZERO,
            LookDirection::new(0.0, 0.0),
        );
        store.set_vehicle_kind(7, EntityKind::TextDisplay);
        let e = store.vehicles.get_mut(&7).unwrap();
        let a = TextSpan::new("A".into(), [1.0, 0.25, 0.0, 0.5]);
        let mut b = TextSpan::new("B".into(), [0.0, 0.5, 1.0, 0.75]);
        b.font = Some("test:color".into());
        e.text_display_text = Some(vec![a, b]);
        e.text_display_opacity = 128;
        store
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
    }

    fn position_close(actual: [f32; 3], expected: DVec3) {
        for (actual, expected) in actual.into_iter().zip(expected.to_array()) {
            close(actual, expected as f32);
        }
    }

    #[test]
    fn world_draw_list_keeps_nameplates_and_uses_current_camera_and_font() {
        use super::super::menu_overlay::MenuElement;
        use crate::player::tab_list::{PlayerInfoActions, PlayerInfoEntry, TabList};
        use crate::renderer::camera::{Camera, CameraMode, CameraUniform};
        use crate::ui::hud::Scoreboard;
        use crate::ui::player_tab::{PlayerNameplates, build_player_nameplates};

        let mut store = store();
        let font = font();
        let mut camera = Camera::new(16.0 / 9.0);
        camera.position = Position::from(DVec3::new(30_000_000.25, 100.5, -30_000_000.75));
        camera.look_dir = LookDirection::new(25.0, -15.0);
        camera.mode = CameraMode::ThirdPersonFront;
        let anchor = camera.anchor();
        let eye = *camera.position + camera.third_person_offset().as_dvec3();
        let (yaw, pitch) = camera.effective_look_deg();
        assert_eq!((yaw, pitch), (205.0, 15.0));
        let e = store.vehicles.get_mut(&7).unwrap();
        e.position = Position::from(eye - camera.look_dir.as_vec().as_dvec3() * 12.0);
        e.text_display_billboard = 3;
        e.text_display_flags = TextDisplayFlags::SHADOW;
        e.text_display_scale = [2.0, 0.5, 3.0];
        e.text_display_translation = [0.125, 0.25, 0.5];
        e.text_display_right_rotation = glam::Quat::from_rotation_y(0.6).to_array();
        let mut through = e.clone();
        through.text_display_flags |= TextDisplayFlags::SEE_THROUGH;
        store.vehicles.insert(8, through);

        let uuid = uuid::Uuid::from_u128(1);
        store.spawn_living(
            9,
            EntityKind::Player,
            Position::from(eye),
            LookDirection::default(),
            0.0,
            Some(uuid),
        );
        let mut tab_list = TabList::new();
        tab_list.apply_update(
            &PlayerInfoActions {
                add_player: true,
                ..Default::default()
            },
            &[PlayerInfoEntry {
                uuid,
                name: "Regular nameplate".into(),
                textures: None,
                game_mode: 0,
                listed: true,
                latency: 0,
                display_name: None,
                list_order: 0,
                show_hat: true,
                chat_session: None,
            }],
        );
        let scoreboard = Scoreboard::default();
        let gui = |store: &EntityStore| {
            let mut overlay = Vec::new();
            build_player_nameplates(
                &mut overlay,
                PlayerNameplates {
                    entity_store: store,
                    tab_list: &tab_list,
                    scoreboard: &scoreboard,
                    local_uuid: uuid::Uuid::nil(),
                    partial_tick: 1.0,
                    screen_height: 1080,
                    fov_degrees: camera.fov_degrees(),
                    camera_pos: eye,
                    project: &|_| Some((400.0, 300.0, 12.0)),
                },
            );
            assert_eq!(
                overlay.len(),
                1,
                "only the ordinary nameplate belongs in GUI"
            );
            let MenuElement::TextSpans {
                x,
                y,
                spans,
                scale,
                centered,
            } = &overlay[0]
            else {
                panic!("TextDisplay must not replace or duplicate a nameplate");
            };
            assert!(*centered);
            (
                *x,
                *y,
                *scale,
                spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            )
        };
        let nameplate = gui(&store);
        assert_eq!(nameplate.3, "Regular nameplate");
        let draws = extract_text_displays(&store, anchor, yaw, pitch, &font);
        assert_eq!(draws.len(), 2);
        for (id, see_through) in [(7, false), (8, true)] {
            let matching: Vec<_> = draws.iter().filter(|d| d.entity_id == id).collect();
            assert_eq!(matching.len(), 1, "one world draw per display");
            let draw = matching[0];
            assert_eq!(draw.see_through, see_through);
            assert!(draw.shadow && !draw.gpu_vertices().is_empty());
        }
        let normal = draws.iter().find(|d| d.entity_id == 7).unwrap();
        // Same position subtraction + UBO projection as sign_text.vert, not
        // the old single screen-anchor depth shared by all quad corners.
        let uniform = CameraUniform::new(&camera, [0.0; 3], 16, false);
        let vp = glam::Mat4::from_cols_array_2d(&uniform.view_projection());
        let camera_pos = Vec3::from_array(uniform.camera_position());
        let depths: Vec<_> = normal.vertices[..6]
            .iter()
            .map(|v| {
                assert!(v.position.iter().all(|p| p.abs() < 32.0));
                let clip = vp * (Vec3::from_array(v.position) - camera_pos).extend(1.0);
                assert!(clip.w > 0.0);
                clip.z / clip.w
            })
            .collect();
        let min = depths.iter().copied().fold(f32::INFINITY, f32::min);
        let max = depths.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!(min > 0.0 && max < 1.0 && max - min > 1e-6);
        let shifted = extract_text_displays(&store, anchor + DVec3::X, yaw, pitch, &font);
        let shifted = shifted.iter().find(|d| d.entity_id == 7).unwrap();
        for (a, b) in normal.vertices.iter().zip(&shifted.vertices) {
            position_close(
                b.position,
                Vec3::from_array(a.position).as_dvec3() - DVec3::X,
            );
        }
        assert_ne!(
            draws,
            extract_text_displays(
                &store,
                anchor,
                camera.look_dir.y_rot_deg(),
                camera.look_dir.x_rot_deg(),
                &font
            )
        );

        // A new pack's map must produce new UVs on the very next extraction.
        let mut glyph = font.glyph('A', None).clone();
        glyph.atlas_x += 32;
        let reloaded = GlyphMap::with_test_glyphs(&[("minecraft:default", 'A', glyph)]);
        let next = extract_text_displays(&store, anchor, yaw, pitch, &reloaded);
        let next = next.iter().find(|d| d.entity_id == 7).unwrap();
        assert_ne!(normal.vertices[0].uv_layer, next.vertices[0].uv_layer);
        store.remove_entity(7);
        store.remove_entity(8);
        assert!(extract_text_displays(&store, anchor, yaw, pitch, &reloaded).is_empty());
        assert_eq!(
            gui(&store),
            nameplate,
            "display removal must not leave a GUI fallback"
        );
    }

    #[test]
    fn world_text_display_wiring_precedes_hand_depth_clear_without_overlay_duplicate() {
        // Pure wiring guard supplements the CPU draw-list test: no Vulkan/window
        // is needed, and reintroducing the old app source must fail this test.
        let app = include_str!("../../app/phases/in_game.rs");
        assert!(!app.contains("build_text_display_overlays("));
        assert!(app.contains("build_player_nameplates("));
        assert!(app.contains("(!benchmark_running).then_some(&game.entity_store)"));
        let renderer = include_str!("../mod.rs");
        assert_eq!(
            renderer
                .matches("extract_text_displays(store, anchor, yaw, pitch, font)")
                .count(),
            1
        );
        let draw = renderer
            .find("self.block_entity_pipeline.draw_text_display(")
            .unwrap();
        let world_depth = renderer.find("self.block_entity_pipeline.draw(").unwrap();
        let clear = renderer.find("cmd.clear_attachments(").unwrap();
        assert!(world_depth < draw && draw < clear);
        assert!(!renderer.contains("draw_occluded_text_displays("));
        assert!(!renderer.contains("draw_from_excluding_occluded_text_displays("));
        assert!(renderer.contains("let (yaw, pitch) = self.camera.effective_look_deg();"));
        assert!(renderer.contains("let Some((font, textures)) = self.menu_pipeline.world_font()"));
    }

    #[test]
    fn gpu_stream_draws_solid_background_before_shadow_and_glyphs() {
        let mut store = store();
        store.vehicles.get_mut(&7).unwrap().text_display_flags = TextDisplayFlags::SHADOW;
        let draw = extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font()).remove(0);
        let stream = draw.gpu_vertices();
        let background_end = draw.background_vertices.len();
        let shadow_end = background_end + draw.shadow_vertices.len();
        assert!(background_end > 0 && shadow_end > background_end);
        for (packed, background) in stream[..background_end]
            .iter()
            .zip(&draw.background_vertices)
        {
            assert_eq!(packed.position, background.position);
            assert_eq!(packed.color, background.color);
            assert_eq!(packed.colored, -1.0);
        }
        assert_eq!(stream[background_end..shadow_end], draw.shadow_vertices);
        assert_eq!(stream[shadow_end..], draw.vertices);
        assert_eq!(stream.len() % 6, 0);
        assert!(
            stream[background_end..]
                .iter()
                .all(|v| v.colored == 0.0 || v.colored == 1.0)
        );
        let empty = TextDisplayDraw {
            vertices: vec![],
            shadow_vertices: vec![],
            background_vertices: vec![],
            ..draw
        };
        assert!(empty.gpu_vertices().is_empty());
    }

    #[test]
    fn styled_spans_use_real_atlas_triangles_and_separate_effects() {
        let mut store = store();
        let font = font();
        store.vehicles.get_mut(&7).unwrap().text_display_flags =
            TextDisplayFlags::SHADOW | TextDisplayFlags::SEE_THROUGH;
        let draws = extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font);
        assert_eq!(draws.len(), 1);
        let draw = &draws[0];
        assert_eq!(draw.entity_id, 7);
        assert_eq!(draw.vertices.len(), 12);
        assert_eq!(draw.shadow_vertices.len(), 12);
        assert_eq!(draw.background_vertices.len(), 6);
        assert!(draw.see_through && draw.shadow);
        for (vertices, rgb, alpha, layer, colored) in [
            (&draw.vertices[..6], [1.0, 0.25, 0.0], 0.5, 2.0, 0.0),
            (&draw.vertices[6..], [0.0, 0.5, 1.0], 0.75, 3.0, 1.0),
        ] {
            for vertex in vertices {
                assert_eq!(&vertex.color[..3], &rgb);
                close(vertex.color[3], alpha * 128.0 / 255.0);
                assert_eq!(vertex.uv_layer[2], layer);
                assert_eq!(vertex.colored, colored);
            }
            assert_ne!(vertices[0].uv_layer, vertices[2].uv_layer);
            assert_eq!(vertices[0], vertices[3]);
            assert_eq!(vertices[2], vertices[4]);
        }
        assert_eq!(
            draw.vertices[0].uv_layer,
            [16.0 / 2048.0, 32.0 / 2048.0, 2.0]
        );
        assert_eq!(
            draw.vertices[8].uv_layer,
            [71.0 / 2048.0, 104.0 / 2048.0, 3.0]
        );
        position_close(draw.vertices[0].position, DVec3::new(-0.15, 0.2, 0.0));
        position_close(draw.vertices[6].position, DVec3::new(0.0, 0.2, 0.0));
        close(draw.shadow_vertices[0].color[0], 0.25);
        close(draw.shadow_vertices[0].color[3], draw.vertices[0].color[3]);
        assert!(draw.background_vertices[0].position[2] < draw.shadow_vertices[0].position[2]);
        assert!(draw.shadow_vertices[0].position[2] < draw.vertices[0].position[2]);
        close(
            draw.background_vertices[0].color[3],
            (64.0 / 255.0) * (128.0 / 255.0),
        );
        let edge_a = Vec3::from_array(draw.vertices[1].position)
            - Vec3::from_array(draw.vertices[0].position);
        let edge_b = Vec3::from_array(draw.vertices[2].position)
            - Vec3::from_array(draw.vertices[0].position);
        assert!(edge_a.cross(edge_b).length() > 0.0);
        assert_eq!(std::mem::size_of::<TextDisplayVertex>(), 44);
    }

    #[test]
    fn wrapped_rows_keep_styles_and_align_within_max_content_width() {
        let font = font();
        let mut store = store();
        // Width 8 forces A (6px) and B (8px) onto distinct rows.
        store.vehicles.get_mut(&7).unwrap().text_display_line_width = 8;
        for (flags, a_x) in [(0x08, -3.0), (0, -2.0), (0x10, -1.0), (0x18, -3.0)] {
            store.vehicles.get_mut(&7).unwrap().text_display_flags = flags;
            let draws = extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font);
            let draw = &draws[0];
            assert_eq!(draw.vertices.len(), 12);
            close(draw.vertices[0].position[0], a_x * 0.025);
            close(draw.vertices[6].position[0], -3.0 * 0.025);
            close(
                draw.vertices[0].position[1] - draw.vertices[6].position[1],
                10.0 * 0.025,
            );
            close(draw.background_vertices[0].position[0], -5.0 * 0.025);
            close(draw.background_vertices[2].position[0], 5.0 * 0.025);
            close(
                draw.background_vertices[0].position[1] - draw.background_vertices[1].position[1],
                20.0 * 0.025,
            );
            assert_eq!(draw.vertices[6].uv_layer[2], 3.0);
        }
        assert_eq!(text_display_alignment(0x18), 0);
    }

    #[test]
    fn background_alpha_default_and_transparent_text() {
        let mut store = store();
        let font = font();
        for (argb, flags, opacity, expected) in [
            (0x0012_3456, 0, 255, None),
            (0x8012_3456, 0, 0, None),
            (
                0x8012_3456,
                0,
                128,
                Some([
                    18.0 / 255.0,
                    52.0 / 255.0,
                    86.0 / 255.0,
                    (128.0 / 255.0) * (128.0 / 255.0),
                ]),
            ),
            (
                0xffff_ffff,
                TextDisplayFlags::DEFAULT_BACKGROUND,
                255,
                Some([0.0, 0.0, 0.0, 64.0 / 255.0]),
            ),
        ] {
            let e = store.vehicles.get_mut(&7).unwrap();
            e.text_display_background = argb;
            e.text_display_flags = flags;
            e.text_display_opacity = opacity;
            let draws = extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font);
            let draw = &draws[0];
            match expected {
                Some(color) => {
                    assert_eq!(draw.background_vertices.len(), 6);
                    assert_eq!(draw.background_vertices[0].color, color);
                }
                None => assert!(draw.background_vertices.is_empty()),
            }
            if opacity == 0 {
                assert!(draw.vertices.is_empty());
            }
            assert!(draw.shadow_vertices.is_empty());
            assert!(!draw.see_through);
        }
    }

    #[test]
    fn entity_rotation_billboard_and_nonuniform_quaternions_transform_every_corner() {
        let mut store = store();
        let font = font();
        let left = DQuat::from_rotation_z(0.4);
        let right = DQuat::from_rotation_x(-0.6);
        let scale = DVec3::new(2.0, 0.5, 3.0);
        let translation = DVec3::new(1.0, 2.0, 3.0);
        let (cy, cp) = (35.0, -20.0);
        let e = store.vehicles.get_mut(&7).unwrap();
        e.look_dir = Some(LookDirection::new(60.0, 25.0));
        e.text_display_translation = translation.as_vec3().to_array();
        e.text_display_scale = scale.as_vec3().to_array();
        e.text_display_left_rotation = left.to_array().map(|v| v as f32);
        e.text_display_right_rotation = right.to_array().map(|v| v as f32);
        let local = sign_glyph_quad(font.glyph('A', None), -7.0, -10.0, [1.0; 4]);
        for (mode, yaw, pitch) in [
            (0, -60.0f64, 25.0f64),
            (1, 145.0, 25.0),
            (2, -60.0, 20.0),
            (3, 145.0, 20.0),
        ] {
            store.vehicles.get_mut(&7).unwrap().text_display_billboard = mode;
            let draws = extract_text_displays(&store, DVec3::ZERO, cy, cp, &font);
            let vertices = &draws[0].vertices[..6];
            let facing = DQuat::from_rotation_y(yaw.to_radians())
                * DQuat::from_rotation_x(pitch.to_radians());
            for (actual, pixel) in vertices.iter().zip(local) {
                // Independent point-by-point application, NOT text_matrix.
                let point = Vec3::from_array(pixel.position).as_dvec3() * -0.025;
                let point = DQuat::from_rotation_y(std::f64::consts::PI) * point;
                let point = left * (scale * (right * point));
                position_close(actual.position, facing * (translation + point));
            }
            let min = vertices
                .iter()
                .map(|v| v.position[2])
                .fold(f32::INFINITY, f32::min);
            let max = vertices
                .iter()
                .map(|v| v.position[2])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(
                max - min > 0.001,
                "corners must not share a fake screen depth"
            );
        }
        store.vehicles.get_mut(&7).unwrap().text_display_billboard = 0;
        let original = extract_text_displays(&store, DVec3::ZERO, cy, cp, &font);
        assert_eq!(
            original,
            extract_text_displays(&store, DVec3::ZERO, -90.0, 80.0, &font)
        );
        let mut other = store.vehicles[&7].clone();
        other.look_dir = Some(LookDirection::new(-45.0, -10.0));
        store.vehicles.insert(8, other);
        let draws = extract_text_displays(&store, DVec3::ZERO, cy, cp, &font);
        assert_ne!(draws[0].vertices, draws[1].vertices);
        store.vehicles.get_mut(&7).unwrap().text_display_billboard = 3;
        store.vehicles.get_mut(&8).unwrap().text_display_billboard = 3;
        let draws = extract_text_displays(&store, DVec3::ZERO, cy, cp, &font);
        assert_eq!(draws[0].vertices, draws[1].vertices);
    }

    #[test]
    fn large_positions_rebase_before_f32_and_invalid_inputs_are_rejected() {
        let font = font();
        let mut store = store();
        let anchor = DVec3::new(30_000_000.0, 100.0, -30_000_000.0);
        store.vehicles.get_mut(&7).unwrap().position =
            Position::from(anchor + DVec3::new(0.125, 0.25, 0.5));
        let draws = extract_text_displays(&store, anchor, 0.0, 0.0, &font);
        position_close(draws[0].vertices[0].position, DVec3::new(-0.025, 0.45, 0.5));
        assert!(extract_text_displays(&store, DVec3::splat(f64::NAN), 0.0, 0.0, &font).is_empty());
        assert!(extract_text_displays(&store, anchor, f32::NAN, 0.0, &font).is_empty());
        let valid = store.vehicles[&7].clone();
        for invalid in 0..9 {
            let mut e = valid.clone();
            match invalid {
                0 => e.text_display_translation[0] = f32::NAN,
                1 => e.text_display_scale[1] = f32::INFINITY,
                2 => e.text_display_left_rotation = [0.0; 4],
                3 => e.text_display_right_rotation[2] = f32::NAN,
                4 => e.position = Position::from(DVec3::splat(f64::NAN)),
                5 => e.position = Position::from(DVec3::splat(f64::MAX)),
                6 => e.text_display_billboard = 255,
                7 => e.look_dir = None,
                _ => e.text_display_text.as_mut().unwrap()[0].color[0] = f32::NAN,
            }
            store.vehicles.insert(7, e);
            assert!(
                extract_text_displays(&store, anchor, 0.0, 0.0, &font).is_empty(),
                "invalid case {invalid}"
            );
        }
    }

    #[test]
    fn whitespace_bold_italic_and_entity_filtering() {
        let mut store = store();
        let font = font();
        let e = store.vehicles.get_mut(&7).unwrap();
        let span = &mut e.text_display_text.as_mut().unwrap()[0];
        span.text = " A".into();
        span.bold = true;
        span.italic = true;
        span.shadow_color = Some([0.2, 0.3, 0.4, 0.5]);
        let draws = extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font);
        let draw = &draws[0];
        assert_eq!(draw.vertices.len(), 18); // A doubled, B once, space no triangles.
        assert_eq!(draw.shadow_vertices.len(), 12); // Explicit style shadow without flag.
        close(
            draw.vertices[6].position[0] - draw.vertices[0].position[0],
            0.025,
        );
        close(
            draw.vertices[0].position[0] - draw.vertices[1].position[0],
            0.05,
        );
        close(draw.shadow_vertices[0].color[3], 0.5 * 0.5 * 128.0 / 255.0);
        for kind in [None, Some(EntityKind::Horse), Some(EntityKind::ItemDisplay)] {
            store.vehicles.get_mut(&7).unwrap().kind = kind;
            assert!(extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font).is_empty());
        }
        let e = store.vehicles.get_mut(&7).unwrap();
        e.kind = Some(EntityKind::TextDisplay);
        e.text_display_text = None;
        assert!(extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font).is_empty());
        store.vehicles.get_mut(&7).unwrap().text_display_text = Some(Vec::new());
        assert!(extract_text_displays(&store, DVec3::ZERO, 0.0, 0.0, &font).is_empty());
    }
}
