//! Opt-in probe-only render diagnostics. Delete this module and its one probe
//! hook when model/time comparison is no longer needed.

use azalea_block::BlockState;
use serde_json::{Value, json};

use crate::renderer::{Renderer, SkyState};
use crate::world::block::{Fluid, FluidKind, fluid};
use crate::world::chunk::ChunkStore;

fn same(a: Fluid, b: Fluid) -> bool {
    a.kind != FluidKind::Empty && a.kind == b.kind
}

fn render_height(world: &ChunkStore, x: i32, y: i32, z: i32, kind: Fluid) -> f32 {
    let state = world.get_block_state(x, y, z);
    let current = fluid(state);
    if !same(kind, current) {
        return if crate::world::block::is_air(state) {
            0.0
        } else {
            -1.0
        };
    }
    let above = fluid(world.get_block_state(x, y + 1, z));
    if same(kind, above) {
        1.0
    } else {
        current.height()
    }
}

fn add_weighted(weighted: &mut [f32; 2], height: f32) {
    if height >= 0.8 {
        weighted[0] += height * 10.0;
        weighted[1] += 10.0;
    } else if height >= 0.0 {
        weighted[0] += height;
        weighted[1] += 1.0;
    }
}

fn average_height(self_height: f32, a: f32, b: f32, corner: f32) -> f32 {
    if a >= 1.0 || b >= 1.0 {
        return 1.0;
    }
    let mut weighted = [0.0, 0.0];
    if a > 0.0 || b > 0.0 {
        if corner >= 1.0 {
            return 1.0;
        }
        add_weighted(&mut weighted, corner);
    }
    add_weighted(&mut weighted, self_height);
    add_weighted(&mut weighted, a);
    add_weighted(&mut weighted, b);
    weighted[0] / weighted[1]
}

fn flow(world: &ChunkStore, x: i32, y: i32, z: i32, current: Fluid) -> [f32; 2] {
    let mut v = [0.0f32; 2];
    for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
        let state = world.get_block_state(x + dx, y, z + dz);
        let neighbor = fluid(state);
        if !same(current, neighbor) {
            continue;
        }
        let height = neighbor.height();
        if height <= 0.0 {
            continue;
        }
        let delta = current.height() - height;
        v[0] += dx as f32 * delta;
        v[1] += dz as f32 * delta;
    }
    let length = v[0].hypot(v[1]);
    if length > 0.0 {
        [v[0] / length, v[1] / length]
    } else {
        [0.0, 0.0]
    }
}

fn top_uv(flow: [f32; 2]) -> Value {
    if flow == [0.0, 0.0] {
        return json!([[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
    }
    let angle = flow[1].atan2(flow[0]) - std::f32::consts::FRAC_PI_2;
    let s = angle.sin() * 0.25;
    let c = angle.cos() * 0.25;
    json!([
        [0.5 + (-c - s), 0.5 + (-c + s)],
        [0.5 + (-c + s), 0.5 + (c + s)],
        [0.5 + (c + s), 0.5 + (c - s)],
        [0.5 + (c - s), 0.5 + (-c - s)],
    ])
}

fn opaque_light_debug(
    world: &ChunkStore,
    x: i32,
    y: i32,
    z: i32,
    state: BlockState,
) -> Option<Value> {
    (crate::world::block::block_id(state) == "stone").then(|| {
        let positions = [
            (x, y + 1, z),
            (x, y + 1, z - 1),
            (x - 1, y + 1, z),
            (x - 1, y + 1, z - 1),
        ];
        let cells = positions
            .iter()
            .map(|&(px, py, pz)| {
                let sky = world.get_sky_light(px, py, pz);
                let block = world.get_block_light(px, py, pz);
                let level = sky.max(block);
                json!({
                    "x": px, "y": py, "z": pz,
                    "sky": sky, "block": block, "level": level,
                    "table": crate::renderer::chunk::mesher::LIGHT_TABLE[level as usize],
                })
            })
            .collect::<Vec<_>>();
        let light = positions
            .iter()
            .map(|&(px, py, pz)| {
                crate::renderer::chunk::mesher::world_brightness(world, px, py, pz)
            })
            .collect::<Vec<_>>();
        json!({
            "source": "Rust greedy_mesh_section/emit_cube_faces equivalent input",
            "face": "up",
            "cells": cells,
            "vertexLight": light,
            "cardinalShade": 1.0,
            "ao": [1.0, 1.0, 1.0, 1.0],
            "finalLight": light,
            "lightTable": crate::renderer::chunk::mesher::LIGHT_TABLE,
        })
    })
}

fn model_debug(renderer: &Renderer, state: BlockState) -> Value {
    let mut model = renderer.probe_model_debug(state);
    atlas_debug(renderer, &mut model);
    model
}

fn atlas_debug(renderer: &Renderer, model: &mut Value) {
    let Some(faces) = model.get("faceTextures").and_then(Value::as_object) else {
        return;
    };
    let mut regions = serde_json::Map::new();
    for value in faces.values() {
        let Some(name) = value.as_str() else {
            continue;
        };
        let region = renderer.atlas_uv_map().get_region(name);
        regions.insert(
            name.to_owned(),
            json!({
                "sprite": region.sprite,
                "pixelRect": region.pixel_rect,
                "uv": [region.u_min, region.v_min, region.u_max, region.v_max],
                "opaque": region.opaque,
                "translucent": region.translucent,
            }),
        );
    }
    model["atlasRegions"] = Value::Object(regions);
}

fn fluid_debug(world: &ChunkStore, x: i32, y: i32, z: i32, state: BlockState) -> Option<Value> {
    let current = fluid(state);
    (current.kind != FluidKind::Empty).then(|| {
        let above = fluid(world.get_block_state(x, y + 1, z));
        let flow = flow(world, x, y, z, current);
        let self_height = render_height(world, x, y, z, current);
        let north = render_height(world, x, y, z - 1, current);
        let south = render_height(world, x, y, z + 1, current);
        let west = render_height(world, x - 1, y, z, current);
        let east = render_height(world, x + 1, y, z, current);
        let corners = [
            average_height(self_height, north, west, render_height(world, x - 1, y, z - 1, current)),
            average_height(self_height, north, east, render_height(world, x + 1, y, z - 1, current)),
            average_height(self_height, south, west, render_height(world, x - 1, y, z + 1, current)),
            average_height(self_height, south, east, render_height(world, x + 1, y, z + 1, current)),
        ];
        let flowing = flow != [0.0, 0.0];
        json!({
            "kind": format!("{:?}", current.kind),
            "type": if current.kind == FluidKind::Water {
                if current.is_source() { "minecraft:water" } else { "minecraft:flowing_water" }
            } else {
                if current.is_source() { "minecraft:lava" } else { "minecraft:flowing_lava" }
            },
            "amount": current.amount,
            "source": current.is_source(),
            "falling": current.falling,
            "ownHeight": current.height(),
            "getHeight": current.height(),
            "rendererHeight": if same(current, above) { 1.0 } else { current.height() },
            "flow": {"x": flow[0], "z": flow[1]},
            "cornerHeights": {"northWest": corners[0], "northEast": corners[1], "southWest": corners[2], "southEast": corners[3]},
            "selectedSprite": if flowing { if current.kind == FluidKind::Water { "water_flow" } else { "lava_flow" } } else { if current.kind == FluidKind::Water { "water_still" } else { "lava_still" } },
            "topUv": top_uv(flow),
            "pipeline": {"layer": "translucent", "depthTest": true, "depthWrite": false, "blend": "src-alpha,one-minus-src-alpha"},
        })
    })
}

pub(crate) fn snapshot(
    renderer: &Renderer,
    sky: &SkyState,
    dimension: &str,
    render_distance: u32,
    lightmap_brightness: f32,
    world: &ChunkStore,
    samples: &[(i32, i32, i32, BlockState)],
) -> Value {
    json!({
        "schema": 2,
        "clock": {
            "dayTime": sky.day_time,
            "gameTime": sky.game_time,
            "partialTick": sky.clock_partial_tick,
            "rate": sky.clock_rate,
            "renderPartialTick": sky.partial_tick,
            "skyColor": sky.sky_color(),
            "rainLevel": sky.rain(),
            "thunderLevel": sky.thunder(),
        },
        "environment": {
            "dimensionSkybox": "selected_by_dimension_type",
            "clearColor": sky.clear_color_linear(dimension, render_distance),
            "colorEncoding": "linear floats -> B8G8R8A8_SRGB framebuffer encode",
            "renderContract": renderer.probe_render_debug(sky.clear_color_linear(dimension, render_distance)),
            "samplerState": {"atlasFormat": "R8G8B8A8_SRGB", "magFilter": "NEAREST", "minFilter": "NEAREST", "mipmapMode": "LINEAR", "maxLod": 4, "anisotropy": 1.0, "addressMode": "CLAMP_TO_EDGE"},
            "lightmap": {"eyeBrightness": lightmap_brightness, "rawSkyBlockEmission": "world-input.jsonl"},
        },
        "samples": samples.iter().map(|(x, y, z, state)| {
            let fluid_json = fluid_debug(world, *x, *y, *z, *state);
            json!({
                "x": x,
                "y": y,
                "z": z,
                "model": model_debug(renderer, *state),
                "opaqueLight": opaque_light_debug(world, *x, *y, *z, *state),
                "fluid": fluid_json,
            })
        }).collect::<Vec<_>>(),
    })
}
