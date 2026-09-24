//! CPU math for the 26.2 ScreenEffectRenderer item activation transform.
//! Candidate source only: production integration/compilation is owned elsewhere.
use glam::{Mat4, Vec3};

pub const ACTIVATION_TICKS: u32 = 40;
/// Vanilla ScreenEffectRenderer submits packed light 15728880 (block+sky fullbright).
pub const ACTIVATION_PACKED_LIGHT: u32 = 15_728_880;
/// Pomme's item_entity.frag has no lightmap sample; fullbright maps to multiplier 1.
pub const ACTIVATION_WORLD_LIGHT: f32 = 1.0;
pub const ACTIVATION_NEAR: f32 = 0.05;
pub const ACTIVATION_FAR: f32 = 100.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivationPose {
    /// ScreenEffect pose followed by the ItemDisplayContext::FIXED item transform.
    pub model: Mat4,
    /// Vanilla `(elapsed_tick + partial_tick) / 40` input.
    pub progress: f32,
    pub pi_scale: f32,
}

/// WindowRenderState.width/height are framebuffer pixel dimensions in 26.2;
/// ScreenEffectRenderer consumes only their ratio (not GUI-scaled dimensions).
pub fn activation_aspect(viewport_width_px: u32, viewport_height_px: u32) -> Option<f32> {
    if viewport_width_px == 0 || viewport_height_px == 0 {
        return None;
    }
    Some(viewport_width_px as f32 / viewport_height_px as f32)
}

/// ScreenEffectRenderer.renderItemActivationAnimation, with vanilla PoseStack
/// post-multiply order. `fixed` is DisplayResolver("fixed").to_matrix(); the
/// existing ItemVertex mesh is already centered [-.5,.5], so do not subtract .5.
pub fn activation_pose(
    ticks_remaining: u32,
    partial_tick: f32,
    offset_x: f32,
    offset_y: f32,
    viewport_width_px: u32,
    viewport_height_px: u32,
    fixed: Mat4,
) -> Option<ActivationPose> {
    if !(1..=ACTIVATION_TICKS).contains(&ticks_remaining) {
        return None;
    }
    let aspect = activation_aspect(viewport_width_px, viewport_height_px)?;
    // Do not clamp partial_tick: vanilla uses the caller's value directly.
    let progress = ((ACTIVATION_TICKS - ticks_remaining) as f32 + partial_tick) / 40.0;
    let ts = progress * progress;
    let tc = progress * ts;
    let smooth = 10.25 * tc * ts - 24.95 * ts * ts + 25.5 * tc - 13.8 * ts + 4.0 * progress;
    let pi_scale = smooth * std::f32::consts::PI;
    let sin_pi = pi_scale.sin();
    let wobble = (pi_scale * 2.0).sin().abs();
    let tilt = (6.0 * (progress * 8.0).cos()).to_radians();

    // Java PoseStack calls: translate, scale, mulPose(Y), mulPose(X), mulPose(Z).
    // ItemStackRenderState.LayerRenderState.applyTransform then appends FIXED.
    let animated = Mat4::from_translation(Vec3::new(
        offset_x * 0.3 * aspect * wobble,
        offset_y * 0.3 * wobble,
        -10.0 + 9.0 * sin_pi,
    )) * Mat4::from_scale(Vec3::splat(0.8))
        * Mat4::from_rotation_y((900.0 * sin_pi.abs()).to_radians())
        * Mat4::from_rotation_x(tilt)
        * Mat4::from_rotation_z(tilt);

    Some(ActivationPose {
        model: animated * fixed,
        progress,
        pi_scale,
    })
}

/// JOML Matrix4f.rotateYXZ(y,x,z) right-multiplies Ry * Rx * Rz.
fn rotation_yxz(y: f32, x: f32, z: f32) -> Mat4 {
    Mat4::from_rotation_y(y) * Mat4::from_rotation_x(x) * Mat4::from_rotation_z(z)
}

/// Lighting.Entry.ITEMS_3D directions from 26.2 Lighting.java, including
/// item3DPose.transformDirection(DIFFUSE_LIGHT_0/1).
pub fn items_3d_lights() -> [Vec3; 2] {
    let normalize = |v: Vec3| v.normalize();
    let light0 = normalize(Vec3::new(0.2, 1.0, -0.7));
    let light1 = normalize(Vec3::new(-0.2, 1.0, 0.7));
    let pi = std::f32::consts::PI;
    let item3d_pose = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0))
        * rotation_yxz(1.0821041, 3.2375858, 0.0)
        * rotation_yxz(-pi / 8.0, 3.0 * pi / 4.0, 0.0);
    [
        item3d_pose.transform_vector3(light0).normalize(),
        item3d_pose.transform_vector3(light1).normalize(),
    ]
}

/// Vanilla light.glsl minecraft_mix_light_separate diffuse factor.
pub fn items_3d_diffuse(normal: Vec3) -> f32 {
    let [l0, l1] = items_3d_lights();
    let n = normal.normalize();
    ((l0.dot(n).max(0.0) + l1.dot(n).max(0.0)) * 0.6 + 0.4).min(1.0)
}

/// Apply the Vulkan viewport-Y correction used by Pomme's hand projection.
pub fn flip_projection_y_for_vulkan(mut projection: Mat4) -> Mat4 {
    projection.y_axis.y *= -1.0;
    projection
}

/// Reproduces GameRenderer's projection immediately surrounding screenEffects:
/// setupPerspective(.05, 100, cameraState.hudFov, window.width, window.height).
/// `hud_fov` comes from that camera state; dimensions are window framebuffer pixels.
pub fn activation_projection(
    viewport_width_px: u32,
    viewport_height_px: u32,
    hud_fov: f32,
) -> Option<Mat4> {
    let aspect = activation_aspect(viewport_width_px, viewport_height_px)?;
    let projection = glam::camera::rh::proj::directx::perspective(
        hud_fov,
        aspect,
        ACTIVATION_NEAR,
        ACTIVATION_FAR,
    );
    Some(flip_projection_y_for_vulkan(projection))
}

#[cfg(test)]
mod tests {
    use super::*;
    const EPS: f32 = 2.0e-5;

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < EPS, "{a} != {b}");
    }

    #[test]
    fn java_lighting_pose_matches_joml_1_10_8_reference_vectors() {
        let [a, b] = items_3d_lights();
        // Independent oracle: JOML 1.10.8 transformDirection output obtained
        // with the exact Lighting.java constructor chain and normalized base vectors.
        for (actual, expected) in [
            (a, Vec3::new(-0.933439314, -0.262694746, -0.244300187)),
            (b, Vec3::new(-0.103571385, -0.976606905, 0.188446447)),
        ] {
            close(actual.x, expected.x);
            close(actual.y, expected.y);
            close(actual.z, expected.z);
            close(actual.length(), 1.0);
        }
        close(items_3d_diffuse(Vec3::Y), 0.4);
        assert_eq!(ACTIVATION_PACKED_LIGHT, 15_728_880);
        close(ACTIVATION_WORLD_LIGHT, 1.0);
    }

    #[test]
    fn forty_tick_start_end_fraction_and_restart_offsets() {
        let fixed = Mat4::IDENTITY;
        assert!(activation_pose(0, 0.0, 0.0, 0.0, 1920, 1080, fixed).is_none());
        assert!(activation_pose(41, 0.0, 0.0, 0.0, 1920, 1080, fixed).is_none());
        let start = activation_pose(40, 0.0, 0.0, 0.0, 1920, 1080, fixed).unwrap();
        close(start.progress, 0.0);
        let origin = start.model.transform_point3(Vec3::ZERO);
        close(origin.x, 0.0);
        close(origin.y, 0.0);
        close(origin.z, -10.0);
        // No uncentered-mesh T(-.5) correction: center origin remains screen center.
        let fractional = activation_pose(28, 0.5, 0.0, 0.0, 1920, 1080, fixed).unwrap();
        close(fractional.progress, 0.3125);
        close(fractional.pi_scale, 1.4864371);
        close(fractional.model.w_axis.z, -1.0320052);
        let a = activation_pose(28, 0.5, 0.5, -0.25, 1920, 1080, fixed).unwrap();
        let b = activation_pose(28, 0.5, -0.5, 0.75, 1920, 1080, fixed).unwrap();
        // Separate randomized x/y offsets affect only their matching axes.
        close(a.model.w_axis.x - b.model.w_axis.x, 0.08955685);
        close(a.model.w_axis.y - b.model.w_axis.y, -0.05037573);
        // A replacement activation starts at 40 ticks with its new pair of offsets.
        let restarted = activation_pose(40, 0.5, -0.5, 0.75, 1920, 1080, fixed).unwrap();
        close(restarted.progress, 0.0125);
        assert!(restarted.model.w_axis.x < 0.0 && restarted.model.w_axis.y > 0.0);
    }

    #[test]
    fn fixed_transform_is_appended_after_animation_in_pose_stack_order() {
        let point = Vec3::new(0.2, -0.1, 0.3);
        let fixed = Mat4::from_translation(Vec3::new(0.25, -0.5, 0.75))
            * Mat4::from_rotation_y(0.7)
            * Mat4::from_scale(Vec3::new(1.0, 0.8, 1.2));
        let pose = activation_pose(40, 0.0, 0.0, 0.0, 16, 9, fixed).unwrap();
        let p_fixed = fixed.transform_point3(point);
        let tilt = 6.0_f32.to_radians();
        let manually_ordered = Vec3::new(0.0, 0.0, -10.0)
            + Mat4::from_rotation_x(tilt)
                .transform_vector3(Mat4::from_rotation_z(tilt).transform_vector3(p_fixed) * 0.8);
        let got = pose.model.transform_point3(point);
        assert!(got.abs_diff_eq(manually_ordered, EPS));
    }

    #[test]
    fn framebuffer_pixels_feed_only_aspect_and_projection_flips_vulkan_y() {
        let wide = activation_aspect(1920, 1080).unwrap();
        close(wide, activation_aspect(640, 360).unwrap());
        assert_ne!(wide, activation_aspect(1920, 1440).unwrap());
        assert!(activation_aspect(1, 0).is_none());
        let p = activation_projection(1920, 1080, 70.0_f32.to_radians()).unwrap();
        assert!(p.y_axis.y < 0.0, "Vulkan projection must flip Y");
        let up = p.project_point3(Vec3::new(0.0, 1.0, -10.0));
        let down = p.project_point3(Vec3::new(0.0, -1.0, -10.0));
        assert!(up.y < 0.0 && down.y > 0.0);
        let depth = p.project_point3(Vec3::new(0.0, 0.0, -10.0)).z;
        assert!(
            (0.0..1.0).contains(&depth),
            "-10 camera-space depth must survive DirectX/Vulkan projection"
        );
    }
}
