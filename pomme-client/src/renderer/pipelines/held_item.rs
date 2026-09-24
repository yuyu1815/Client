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
}

pub struct HeldItemPipeline {
    pipeline: vk::Pipeline,
    shared: ItemPipelineShared,
    display: DisplayResolver,
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
        item: &HeldItemInfo,
        meshes: &ItemEntityPipeline,
        bob: Mat4,
    ) {
        let Some((buffer, vertex_count)) = meshes.mesh_handle(&item.name) else {
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

        let display = self
            .display
            .resolve(&item.name, default_first_person(item.has_3d_model));
        let arm = match use_anim {
            Some(anim) => eat_item_matrix(anim),
            None => first_person_item_matrix(swing_progress),
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

// Vanilla ItemInHandRenderer: applyItemArmTransform + swingArm (right hand,
// inverseArmHeight = 0).
fn first_person_item_matrix(swing_progress: f32) -> Mat4 {
    let a = swing_progress;
    let sq = a.sqrt();
    let pi = std::f32::consts::PI;

    Mat4::from_translation(Vec3::new(0.56, -0.52, -0.72))
        * Mat4::from_translation(Vec3::new(
            -0.4 * (sq * pi).sin(),
            0.2 * (sq * pi * 2.0).sin(),
            -0.2 * (a * pi).sin(),
        ))
        * Mat4::from_rotation_y((45.0 + (a * a * pi).sin() * -20.0).to_radians())
        * Mat4::from_rotation_z(((sq * pi).sin() * -20.0).to_radians())
        * Mat4::from_rotation_x(((sq * pi).sin() * -80.0).to_radians())
        * Mat4::from_rotation_y((-45.0_f32).to_radians())
}

// Vanilla ItemInHandRenderer: applyEatTransform then applyItemArmTransform
// (EAT/DRINK skip the usual pre-transform; right hand, inverseArmHeight = 0).
fn eat_item_matrix(anim: UseAnim) -> Mat4 {
    let scaled = anim.curr_usage_time / anim.duration;
    // The chew bob runs after the first 20% of the eat, oscillating every 4
    // ticks; the jiggle shoves the item into the mouth over the last bite.
    let bob = if scaled < 0.8 {
        ((anim.curr_usage_time / 4.0 * std::f32::consts::PI).cos() * 0.1).abs()
    } else {
        0.0
    };
    let jiggle = 1.0 - (scaled as f64).powf(27.0) as f32;

    Mat4::from_translation(Vec3::new(jiggle * 0.6, bob + jiggle * -0.5, 0.0))
        * Mat4::from_rotation_y((jiggle * 90.0).to_radians())
        * Mat4::from_rotation_x((jiggle * 10.0).to_radians())
        * Mat4::from_rotation_z((jiggle * 30.0).to_radians())
        * Mat4::from_translation(Vec3::new(0.56, -0.52, -0.72))
}

fn default_first_person(has_3d_model: bool) -> DisplayTransform {
    if has_3d_model {
        // block/block.json firstperson_righthand
        DisplayTransform {
            rotation: Vec3::new(0.0, 45.0, 0.0),
            translation: Vec3::ZERO,
            scale: Vec3::splat(0.40),
        }
    } else {
        // item/generated.json firstperson_righthand
        DisplayTransform {
            rotation: Vec3::new(0.0, -90.0, 25.0),
            translation: Vec3::new(1.13, 3.2, 1.13) / 16.0,
            scale: Vec3::splat(0.68),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_display_transform_pivots_around_model_center() {
        let arm = first_person_item_matrix(0.0);
        let model = arm * default_first_person(true).to_matrix();
        let center = model.transform_point3(Vec3::ZERO);
        let arm_origin = arm.transform_point3(Vec3::ZERO);
        assert!(center.abs_diff_eq(arm_origin, 1e-6));
    }
}
