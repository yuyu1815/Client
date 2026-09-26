use std::path::Path;
use std::sync::{Arc, Mutex};

use glam::{Mat4, Vec3};
use pomme_gpu_allocator::vulkan::Allocator;
use pyronyx::vk;

use crate::renderer::camera::CameraUniform;
use crate::renderer::chunk::atlas::TextureAtlas;
use crate::renderer::pipelines::hand;
use crate::renderer::pipelines::item_display::{DisplayResolver, DisplayTransform};
use crate::renderer::pipelines::item_entity::{
    self, ItemEntityPipeline, ItemPipelineShared, push_model_light, push_world_lighting,
};

pub struct HeldItemInfo {
    pub name: String,
    pub light: f32,
    pub has_3d_model: bool,
    pub nether_lighting: bool,
}

/// First-person use-animation state (eat/drink), the inputs to vanilla
/// `ItemInHandRenderer.applyEatTransform`.
#[derive(Clone, Copy)]
pub struct UseAnim {
    /// Vanilla `useItemRemaining - partialTick + 1`.
    pub curr_usage_time: f32,
    pub duration: f32,
    pub left_hand: bool,
    pub bow: bool,
}

pub struct HeldItemPipeline {
    pipeline: vk::Pipeline,
    shared: ItemPipelineShared,
    display: DisplayResolver,
    left_display: DisplayResolver,
    activation: bool,
    last_draw_trace: Option<serde_json::Value>,
}

impl HeldItemPipeline {
    pub fn new(
        device: &vk::Device,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        atlas: &TextureAtlas,
        jar_assets_dir: &Path,
    ) -> Self {
        let shared = ItemPipelineShared::new(device, allocator, atlas, "held_item");
        let pipeline =
            item_entity::create_held_pipeline(device, render_pass, shared.pipeline_layout);
        Self {
            pipeline,
            shared,
            display: DisplayResolver::new(jar_assets_dir, "firstperson_righthand"),
            left_display: DisplayResolver::new(jar_assets_dir, "firstperson_lefthand"),
            activation: false,
            last_draw_trace: None,
        }
    }

    /// Owns an independent activation camera UBO/descriptor pool and atlas set.
    pub fn new_activation(
        device: &vk::Device,
        render_pass: vk::RenderPass,
        allocator: &Arc<Mutex<Allocator>>,
        atlas: &TextureAtlas,
        jar_assets_dir: &Path,
    ) -> Self {
        let shared = ItemPipelineShared::new(device, allocator, atlas, "totem_activation");
        let pipeline =
            item_entity::create_activation_pipeline(device, render_pass, shared.pipeline_layout);
        Self {
            pipeline,
            shared,
            display: DisplayResolver::new(jar_assets_dir, "fixed"),
            left_display: DisplayResolver::new(jar_assets_dir, "fixed"),
            activation: true,
            last_draw_trace: None,
        }
    }

    pub fn rebind_atlas(&self, device: &vk::Device, atlas: &TextureAtlas) {
        self.shared.rebind_atlas(device, atlas);
    }

    pub fn update_display_resources(
        &mut self,
        jar_assets_dir: &Path,
        asset_index: &Option<crate::assets::AssetIndex>,
        pack_dirs: &[std::path::PathBuf],
    ) {
        if self.activation {
            self.display
                .update_resources(jar_assets_dir, asset_index, pack_dirs);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_and_draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        aspect: f32,
        hud_fov: f32,
        swing_progress: f32,
        use_anim: Option<UseAnim>,
        left_hand: bool,
        item: &HeldItemInfo,
        meshes: &ItemEntityPipeline,
        bob: Mat4,
    ) {
        let selected_name = selected_item_model_name(&item.name, use_anim, left_hand);
        let selected_name = if meshes.mesh_handle(selected_name).is_some() {
            selected_name
        } else {
            &item.name
        };
        let Some((buffer, vertex_count)) = meshes.mesh_handle(selected_name) else {
            self.last_draw_trace = Some(serde_json::json!({
                "status": "skipped", "reason": "no_item_mesh", "item": item.name,
                "frameIndex": frame, "vertexCount": 0,
                "provenance": "actual HeldItemPipeline::update_and_draw caller"
            }));
            return;
        };

        let view_projection = hand::projection(aspect, hud_fov) * bob;
        let uniform = CameraUniform::with_view_proj(view_projection);
        self.shared.update_camera(frame, &uniform);

        let display = if left_hand {
            self.left_display
                .resolve(&item.name, default_first_person(item.has_3d_model, true))
        } else {
            self.display
                .resolve(&item.name, default_first_person(item.has_3d_model, false))
        };
        let arm = match use_anim.filter(|anim| anim.left_hand == left_hand) {
            Some(anim) if anim.bow => bow_item_matrix(anim),
            Some(anim) => eat_item_matrix(anim),
            None => first_person_item_matrix(swing_progress, left_hand),
        };
        // build_item_mesh stores vertices centered at the origin; ItemTransform's
        // final -0.5 translation applies to vanilla's uncentered [0, 1] vertices.
        let model = arm * display.to_matrix();

        self.shared.bind(cmd, frame, self.pipeline);
        cmd.bind_vertex_buffers(0, &[buffer], &[0]);
        push_model_light(cmd, self.shared.pipeline_layout, &model, item.light);
        // Held items use the same transformed-normal path as dropped items.
        // Vanilla renders the hand after selecting Lighting.LEVEL; the
        // GUI-only precomputed light byte in the shared mesh is ignored here.
        push_world_lighting(
            cmd,
            self.shared.pipeline_layout,
            &model,
            item.nether_lighting,
        );
        cmd.draw(vertex_count, 1, 0, 0);
        self.last_draw_trace = Some(serde_json::json!({
            "status": "submitted",
            "source": "actual Vulkan HeldItemPipeline::update_and_draw cmd.draw",
            "vertexPayload": std::env::var_os("POMME_HELD_DRAW_PAYLOAD_TRACE")
                .is_some()
                .then(|| meshes.debug_held_draw_payload(&item.name))
                .flatten(),
            "frameIndex": frame,
            "actualDrawCount": 1,
            "actualDrawAt": chrono::Utc::now().to_rfc3339(),
            "item": item.name,
            "vertexCount": vertex_count,
            "light": item.light,
            "lightMode": if item.nether_lighting { "NETHER" } else { "LEVEL" },
            "modelMatrixColumnMajor": model.to_cols_array(),
            "normalMatrixColumnMajor": glam::Mat3::from_mat4(model).inverse().transpose().to_cols_array(),
            "viewProjectionMatrixColumnMajor": view_projection.to_cols_array(),
            "provenance": "CPU actual submitted matrices/pipeline draw; not GPU vertex readback"
        }));
    }

    /// Draws with caller-computed activation camera and animation transform,
    /// composed with the item's resolved FIXED display transform.
    pub fn update_activation_and_draw(
        &mut self,
        cmd: vk::CommandBuffer,
        frame: usize,
        view_projection: Mat4,
        animation_transform: Mat4,
        item: &HeldItemInfo,
        meshes: &ItemEntityPipeline,
    ) {
        assert!(self.activation, "activation draw requires new_activation");
        let Some((buffer, vertex_count)) = meshes.mesh_handle(&item.name) else {
            self.last_draw_trace = Some(serde_json::json!({
                "status": "skipped", "reason": "no_item_mesh", "item": item.name,
                "frameIndex": frame, "vertexCount": 0,
                "provenance": "activation mesh lookup"
            }));
            return;
        };
        self.shared
            .update_camera(frame, &CameraUniform::with_view_proj(view_projection));
        let fixed = self.display.resolve(
            &item.name,
            DisplayTransform {
                rotation: Vec3::ZERO,
                translation: Vec3::ZERO,
                scale: Vec3::ONE,
            },
        );
        let model = animation_transform * fixed.to_matrix();
        self.shared.bind(cmd, frame, self.pipeline);
        cmd.bind_vertex_buffers(0, &[buffer], &[0]);
        push_model_light(cmd, self.shared.pipeline_layout, &model, item.light);
        push_world_lighting(
            cmd,
            self.shared.pipeline_layout,
            &model,
            item.nether_lighting,
        );
        cmd.draw(vertex_count, 1, 0, 0);
        self.last_draw_trace = Some(serde_json::json!({
            "status": "submitted", "source": "actual Vulkan activation draw",
            "frameIndex": frame, "vertexCount": vertex_count,
            "item": item.name, "modelMatrixColumnMajor": model.to_cols_array(),
            "viewProjectionMatrixColumnMajor": view_projection.to_cols_array(),
            "provenance": "CPU submitted matrices/pipeline draw; not GPU readback"
        }));
    }

    pub fn probe_draw_trace(&self) -> Option<serde_json::Value> {
        self.last_draw_trace.clone()
    }

    pub fn clear_probe_trace(&mut self) {
        self.last_draw_trace = None;
    }

    pub fn recreate_pipeline(&mut self, device: &vk::Device, render_pass: vk::RenderPass) {
        device.destroy_pipeline(self.pipeline, None);
        self.pipeline = if self.activation {
            item_entity::create_activation_pipeline(
                device,
                render_pass,
                self.shared.pipeline_layout,
            )
        } else {
            item_entity::create_held_pipeline(device, render_pass, self.shared.pipeline_layout)
        };
    }

    pub fn destroy(&mut self, device: &vk::Device, allocator: &Arc<Mutex<Allocator>>) {
        device.destroy_pipeline(self.pipeline, None);
        self.shared.destroy(device, allocator);
    }
}

/// CPU model selection shared by the draw path and headless interaction tests.
pub(crate) fn selected_item_model_name(
    base_name: &str,
    use_anim: Option<UseAnim>,
    left_hand: bool,
) -> &str {
    use_anim
        .filter(|anim| base_name == "bow" && anim.bow && anim.left_hand == left_hand)
        .as_ref()
        .map(bow_model_name)
        .unwrap_or(base_name)
}

fn bow_model_name(anim: &UseAnim) -> &'static str {
    let elapsed = (anim.duration - anim.curr_usage_time).max(0.0);
    if elapsed >= 18.0 {
        "bow_pulling_2"
    } else if elapsed >= 13.0 {
        "bow_pulling_1"
    } else {
        "bow_pulling_0"
    }
}

// Vanilla ItemInHandRenderer: applyItemArmTransform + swingArm.
fn first_person_item_matrix(swing_progress: f32, left_hand: bool) -> Mat4 {
    let a = swing_progress;
    let sq = a.sqrt();
    let pi = std::f32::consts::PI;

    let hand_sign = if left_hand { -1.0 } else { 1.0 };
    Mat4::from_translation(Vec3::new(hand_sign * 0.56, -0.52, -0.72))
        * Mat4::from_translation(Vec3::new(
            hand_sign * -0.4 * (sq * pi).sin(),
            0.2 * (sq * pi * 2.0).sin(),
            -0.2 * (a * pi).sin(),
        ))
        * Mat4::from_rotation_y((45.0 + (a * a * pi).sin() * -20.0).to_radians())
        * Mat4::from_rotation_z(((sq * pi).sin() * -20.0).to_radians())
        * Mat4::from_rotation_x(((sq * pi).sin() * -80.0).to_radians())
        * Mat4::from_rotation_y((-45.0_f32).to_radians())
}

// Vanilla ItemInHandRenderer BOW branch (26.2): fixed bow pose plus the
// 20-tick quadratic draw power, applied after the ordinary arm transform.
fn bow_item_matrix(anim: UseAnim) -> Mat4 {
    let time_held = anim.duration - anim.curr_usage_time;
    let mut power = time_held / 20.0;
    power = ((power * power + power * 2.0) / 3.0).min(1.0);
    let invert = if anim.left_hand { -1.0 } else { 1.0 };
    first_person_item_matrix(0.0, anim.left_hand)
        * Mat4::from_translation(Vec3::new(invert * -0.2785682, 0.18344387, 0.15731531))
        * Mat4::from_rotation_x((-13.935_f32).to_radians())
        * Mat4::from_rotation_y((invert * 35.3_f32).to_radians())
        * Mat4::from_rotation_z((invert * -9.785_f32).to_radians())
        * Mat4::from_translation(Vec3::new(0.0, 0.0, power * 0.04))
        * Mat4::from_scale(Vec3::new(1.0, 1.0, 1.0 + power * 0.2))
        * Mat4::from_rotation_y((invert * -45.0_f32).to_radians())
}

// Vanilla ItemInHandRenderer: applyEatTransform then applyItemArmTransform
// (EAT/DRINK skip the usual pre-transform; right hand, inverseArmHeight = 0).
fn eat_item_matrix(anim: UseAnim) -> Mat4 {
    let scaled = anim.curr_usage_time / anim.duration;
    let invert = if anim.left_hand { -1.0 } else { 1.0 };
    // The chew bob runs after the first 20% of the eat, oscillating every 4
    // ticks; the jiggle shoves the item into the mouth over the last bite.
    let bob = if scaled < 0.8 {
        ((anim.curr_usage_time / 4.0 * std::f32::consts::PI).cos() * 0.1).abs()
    } else {
        0.0
    };
    let jiggle = 1.0 - (scaled as f64).powf(27.0) as f32;

    Mat4::from_translation(Vec3::new(invert * jiggle * 0.6, bob + jiggle * -0.5, 0.0))
        * Mat4::from_rotation_y((invert * jiggle * 90.0).to_radians())
        * Mat4::from_rotation_x((jiggle * 10.0).to_radians())
        * Mat4::from_rotation_z((invert * jiggle * 30.0).to_radians())
        * Mat4::from_translation(Vec3::new(invert * 0.56, -0.52, -0.72))
}

fn default_first_person(has_3d_model: bool, left_hand: bool) -> DisplayTransform {
    let invert = if left_hand { -1.0 } else { 1.0 };
    if has_3d_model {
        // block/block.json first-person transform, mirrored for the left hand.
        DisplayTransform {
            rotation: Vec3::new(0.0, invert * 45.0, 0.0),
            translation: Vec3::ZERO,
            scale: Vec3::splat(0.40),
        }
    } else {
        // item/generated.json first-person transform, mirrored for the left hand.
        DisplayTransform {
            rotation: Vec3::new(0.0, invert * -90.0, invert * 25.0),
            translation: Vec3::new(invert * 1.13, 3.2, 1.13) / 16.0,
            scale: Vec3::splat(0.68),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bow_mesh_stage_uses_elapsed_ticks() {
        let stage = |ticks: f32| {
            bow_model_name(&UseAnim {
                curr_usage_time: 72_000.0 - ticks,
                duration: 72_000.0,
                left_hand: false,
                bow: true,
            })
        };
        for (ticks, expected) in [
            (0.0, "bow_pulling_0"),
            (12.0, "bow_pulling_0"),
            (13.0, "bow_pulling_1"),
            (17.0, "bow_pulling_1"),
            (18.0, "bow_pulling_2"),
            (20.0, "bow_pulling_2"),
        ] {
            assert_eq!(stage(ticks), expected);
        }
    }

    #[test]
    fn held_item_matrix_places_each_hand_on_its_side_during_idle_and_swing() {
        let idle_right = first_person_item_matrix(0.0, false).w_axis.x;
        let idle_left = first_person_item_matrix(0.0, true).w_axis.x;
        assert!((idle_right - 0.56).abs() < 1e-6);
        assert!((idle_left + 0.56).abs() < 1e-6);

        let swing_right = first_person_item_matrix(0.5, false).w_axis.x;
        let swing_left = first_person_item_matrix(0.5, true).w_axis.x;
        assert!(swing_right > 0.0 && swing_left < 0.0);
        assert!((swing_right + swing_left).abs() < 1e-6);
    }

    #[test]
    fn bow_draw_power_matches_vanilla_twenty_tick_curve_and_hand_sign() {
        let anim = |ticks_held: f32, left_hand| UseAnim {
            curr_usage_time: 72_000.0 - ticks_held,
            duration: 72_000.0,
            left_hand,
            bow: true,
        };
        let idle = bow_item_matrix(anim(0.0, false));
        let early = bow_item_matrix(anim(10.0, false));
        let full = bow_item_matrix(anim(20.0, false));
        let left = bow_item_matrix(anim(20.0, true));
        let base_right = first_person_item_matrix(0.0, false);

        assert!(idle.abs_diff_eq(
            base_right
                * Mat4::from_translation(Vec3::new(-0.2785682, 0.18344387, 0.15731531,))
                * Mat4::from_rotation_x((-13.935_f32).to_radians())
                * Mat4::from_rotation_y(35.3_f32.to_radians())
                * Mat4::from_rotation_z((-9.785_f32).to_radians())
                * Mat4::from_rotation_y((-45.0_f32).to_radians()),
            1e-6
        ));
        // Official 26.2: (0.5^2 + 2 * 0.5) / 3 = 5/12 at 10 ticks.
        // Undo the final -45-degree Y rotation to measure the stretched Z axis.
        let stretch_axis = Mat4::from_rotation_y(45_f32.to_radians()).transform_vector3(Vec3::Z);
        assert!(
            (early.transform_vector3(stretch_axis).length() - (1.0 + (5.0_f32 / 12.0) * 0.2)).abs()
                < 1e-5
        );
        assert!((full.transform_vector3(stretch_axis).length() - 1.2).abs() < 1e-5);
        assert!(!full.abs_diff_eq(base_right, 1e-3));
        assert!((left.w_axis.x + full.w_axis.x).abs() < 1e-6);
    }

    #[test]
    fn held_display_transform_pivots_around_model_center() {
        let arm = first_person_item_matrix(0.0, false);
        let model = arm * default_first_person(true, false).to_matrix();
        let center = model.transform_point3(Vec3::ZERO);
        let arm_origin = arm.transform_point3(Vec3::ZERO);
        assert!(center.abs_diff_eq(arm_origin, 1e-6));
    }
}
