use crate::physics::block_shape::{self, LocalBox};
use crate::world::block::{block_id, has_collision, is_air};
use crate::world::chunk::ChunkStore;

const FULL_BLOCK: LocalBox = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];

#[derive(Clone, Copy, Debug)]
pub(crate) struct ShadowPiece {
    pub relative: [f32; 3],
    pub bounds: LocalBox,
    pub alpha: f32,
    pub brightness: u8,
    pub power_at_depth: f32,
    pub uv: [f32; 4],
}

/// Java 26.2 `EntityRenderer.extractShadow` surface/light/visibility contract,
/// limited to known full-collision cubes. Missing chunks fail closed.
pub(crate) fn item_shadow_pieces(
    chunks: &ChunkStore,
    entity: [f64; 3],
    camera: [f64; 3],
    radius: f32,
    strength: f32,
    ambient_light: f32,
    shadows_enabled: bool,
    visible: bool,
) -> Vec<ShadowPiece> {
    if !shadows_enabled || !visible || radius <= 0.0 {
        return Vec::new();
    }
    let radius = radius.min(32.0);
    let distance_sq = (0..3)
        .map(|i| (camera[i] - entity[i]).powi(2))
        .sum::<f64>();
    let Some(power) = shadow_power(distance_sq, strength) else {
        return Vec::new();
    };

    let depth = (power / 0.5 - 1.0).min(radius);
    let x0 = (entity[0] - radius as f64).floor() as i32;
    let x1 = (entity[0] + radius as f64).floor() as i32;
    let z0 = (entity[2] - radius as f64).floor() as i32;
    let z1 = (entity[2] + radius as f64).floor() as i32;
    let y0 = (entity[1] - depth as f64).floor() as i32;
    let y1 = entity[1].floor() as i32;
    let mut pieces = Vec::new();

    for z in z0..=z1 {
        for x in x0..=x1 {
            for y in y0..=y1 {
                let Some(below) = loaded_full_surface(chunks, x, y - 1, z) else {
                    continue;
                };
                let Some(brightness) = loaded_max_brightness(chunks, x, y, z) else {
                    continue;
                };
                let power_at_depth = power - (entity[1] - y as f64) as f32 * 0.5;
                let Some(alpha) = shadow_alpha(power_at_depth, brightness, ambient_light) else {
                    continue;
                };
                let relative = [
                    x as f32 - entity[0] as f32,
                    y as f32 - entity[1] as f32,
                    z as f32 - entity[2] as f32,
                ];
                let [min_x, min_y, min_z, max_x, _, max_z] = below;
                let u0 = -(relative[0] + min_x as f32) / (2.0 * radius) + 0.5;
                let u1 = -(relative[0] + max_x as f32) / (2.0 * radius) + 0.5;
                let v0 = -(relative[2] + min_z as f32) / (2.0 * radius) + 0.5;
                let v1 = -(relative[2] + max_z as f32) / (2.0 * radius) + 0.5;
                pieces.push(ShadowPiece {
                    relative: [relative[0], relative[1], relative[2]],
                    bounds: [min_x, min_y, min_z, max_x, below[4], max_z],
                    alpha,
                    brightness,
                    power_at_depth,
                    uv: [u0, v0, u1, v1],
                });
            }
        }
    }
    pieces
}

fn loaded_full_surface(chunks: &ChunkStore, x: i32, y: i32, z: i32) -> Option<LocalBox> {
    // Block data does not encode RenderShape; reject empty/partial surfaces, but an invisible full-collision block remains unclassifiable.
    let chunk_pos = azalea_core::position::ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
    chunks.get_chunk(&chunk_pos)?;
    let state = chunks.get_block_state(x, y, z);
    full_surface_shape(state)
}

fn full_surface_shape(state: azalea_block::BlockState) -> Option<LocalBox> {
    let name = block_id(state);
    let invisible_render_shape =
        crate::world::block_entity::is_invisible_block(name) && name != "heavy_core";
    if is_air(state)
        || invisible_render_shape
        || !has_collision(state)
        || block_shape::partial_shape(state).is_some()
    {
        return None;
    }
    Some(FULL_BLOCK)
}

fn loaded_max_brightness(chunks: &ChunkStore, x: i32, y: i32, z: i32) -> Option<u8> {
    // skyDarken is not tracked; the accepted daytime fixture's sky level is 15, so Java's subtraction is zero here.
    let chunk_pos = azalea_core::position::ChunkPos::new(x.div_euclid(16), z.div_euclid(16));
    chunks.light_data.get(&(chunk_pos.x, chunk_pos.z))?;
    Some(chunks.get_sky_light(x, y, z).max(chunks.get_block_light(x, y, z)))
}

fn shadow_power(distance_sq: f64, strength: f32) -> Option<f32> {
    let power = (1.0 - distance_sq / 256.0) as f32 * strength;
    (power > 0.0).then_some(power)
}

fn lightmap_brightness(level: u8, ambient_light: f32) -> f32 {
    let value = level as f32 / 15.0;
    let curved = value / (4.0 - 3.0 * value);
    curved + (1.0 - curved) * ambient_light
}

fn shadow_alpha(power_at_depth: f32, brightness: u8, ambient_light: f32) -> Option<f32> {
    (brightness > 3).then(|| {
        (power_at_depth * 0.5 * lightmap_brightness(brightness, ambient_light)).clamp(0.0, 1.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadow_contract_table_rejects_missing_surface_and_matches_alpha_gates() {
        crate::world::block::init("26.2");
        let stone = crate::world::block::first_state_of("stone").unwrap();
        let air = crate::world::block::first_state_of("air").unwrap();
        let slab = crate::world::block::first_state_of("stone_slab").unwrap();
        let barrier = crate::world::block::first_state_of("barrier").unwrap();
        let distance_sq = (65.62_f64 - 64.0).powi(2) + 2.0_f64.powi(2);
        let cases = [
            ("unloaded", None, None, true, true, distance_sq, None),
            ("empty surface", Some(air), Some(15), true, true, distance_sq, None),
            ("invisible RenderShape", Some(barrier), Some(15), true, true, distance_sq, None),
            ("partial shape", Some(slab), Some(15), true, true, distance_sq, None),
            ("light threshold", Some(stone), Some(3), true, true, distance_sq, None),
            ("distance fade", Some(stone), Some(15), true, true, 256.0, None),
            ("option disabled", Some(stone), Some(15), false, true, distance_sq, None),
            ("invisible", Some(stone), Some(15), true, false, distance_sq, None),
            ("stone at full light", Some(stone), Some(15), true, true, distance_sq, Some(0.36529627)),
        ];
        for (name, state, light, enabled, visible, distance, expected) in cases {
            let actual = state
                .and_then(full_surface_shape)
                .zip(light)
                .filter(|_| enabled && visible)
                .and_then(|(_, level)| shadow_power(distance, 0.75).and_then(|power| shadow_alpha(power, level, 0.0)));
            assert_eq!(actual.is_some(), expected.is_some(), "{name}");
            if let (Some(actual), Some(expected)) = (actual, expected) {
                assert!((actual - expected).abs() < 1e-6, "{name}: {actual}");
            }
        }
    }
}
