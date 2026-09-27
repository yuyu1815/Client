use std::path::Path;
use std::time::Instant;

fn write_result_json(path: &Path, value: &impl serde::Serialize) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

use crate::renderer::RenderTimings;

const DURATION_SECS: f32 = 10.0;
const WARMUP_FRAMES: u32 = 30;
const SPIKE_THRESHOLD_MS: f32 = 8.0;

/// A UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) for stamping benchmark results that
/// get reported back.
fn iso8601_utc_now() -> String {
    let fmt = time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    time::OffsetDateTime::now_utc()
        .format(&fmt)
        .unwrap_or_default()
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct FrameSample {
    pub frame_ms: f32,
    pub fence_ms: f32,
    #[serde(default)]
    pub acquire_ms: f32,
    #[serde(default)]
    pub present_ms: f32,
    #[serde(default)]
    pub render_prepare_ms: f32,
    #[serde(default)]
    pub render_setup_ms: f32,
    #[serde(default)]
    pub submit_ms: f32,
    pub cull_ms: f32,
    /// CPU time spent recording render commands; excludes GPU execution.
    pub draw_ms: f32,
    #[serde(default)]
    pub chunk_draw_ms: f32,
    #[serde(default)]
    pub entity_draw_ms: f32,
    #[serde(default)]
    pub block_entity_draw_ms: f32,
    #[serde(default)]
    pub be_model_ms: f32,
    #[serde(default)]
    pub be_sign_text_ms: f32,
    #[serde(default)]
    pub be_model_draws: u32,
    #[serde(default)]
    pub be_sign_vertices: u32,
    #[serde(default)]
    pub item_entity_draw_ms: f32,
    #[serde(default)]
    pub environment_draw_ms: f32,
    #[serde(default)]
    pub hud_draw_ms: f32,
    #[serde(default)]
    pub cpu_update_ms: f32,
    #[serde(default)]
    pub fixed_tick_ms: f32,
    #[serde(default)]
    pub fixed_tick_count: u32,
    #[serde(default)]
    pub be_extract_ms: f32,
    #[serde(default)]
    pub render_wall_ms: f32,
    #[serde(default)]
    pub net_decode_ms: f32,
    #[serde(default)]
    pub visibility_ms: f32,
    #[serde(default)]
    pub rescan_ms: f32,
    #[serde(default)]
    pub mesh_drain_ms: f32,
    #[serde(default)]
    pub upload_ms: f32,
    /// Frame-time residual, including work and waits not covered by the other
    /// phases.
    #[serde(alias = "frame_wait_ms", default)]
    pub unaccounted_ms: f32,
    #[serde(default)]
    pub effective_fps_limit: Option<u32>,
    #[serde(default)]
    pub window_occluded: bool,
    #[serde(default)]
    pub vsync: bool,
    pub chunk_count: u32,
    pub entity_count: u32,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SpikeSample {
    pub frame_index: u32,
    pub frame_ms: f32,
    pub fence_ms: f32,
    #[serde(default)]
    pub acquire_ms: f32,
    #[serde(default)]
    pub present_ms: f32,
    #[serde(default)]
    pub render_prepare_ms: f32,
    #[serde(default)]
    pub render_setup_ms: f32,
    #[serde(default)]
    pub submit_ms: f32,
    pub cull_ms: f32,
    /// CPU time spent recording render commands; excludes GPU execution.
    pub draw_ms: f32,
    #[serde(default)]
    pub chunk_draw_ms: f32,
    #[serde(default)]
    pub entity_draw_ms: f32,
    #[serde(default)]
    pub block_entity_draw_ms: f32,
    #[serde(default)]
    pub be_model_ms: f32,
    #[serde(default)]
    pub be_sign_text_ms: f32,
    #[serde(default)]
    pub be_model_draws: u32,
    #[serde(default)]
    pub be_sign_vertices: u32,
    #[serde(default)]
    pub item_entity_draw_ms: f32,
    #[serde(default)]
    pub environment_draw_ms: f32,
    #[serde(default)]
    pub hud_draw_ms: f32,
    #[serde(default)]
    pub cpu_update_ms: f32,
    #[serde(default)]
    pub fixed_tick_ms: f32,
    #[serde(default)]
    pub fixed_tick_count: u32,
    #[serde(default)]
    pub be_extract_ms: f32,
    #[serde(default)]
    pub render_wall_ms: f32,
    #[serde(default)]
    pub net_decode_ms: f32,
    #[serde(default)]
    pub visibility_ms: f32,
    #[serde(default)]
    pub rescan_ms: f32,
    #[serde(default)]
    pub mesh_drain_ms: f32,
    #[serde(default)]
    pub upload_ms: f32,
    /// Frame-time residual, including work and waits not covered by the other
    /// phases.
    #[serde(alias = "frame_wait_ms", default)]
    pub unaccounted_ms: f32,
    #[serde(default)]
    pub effective_fps_limit: Option<u32>,
    #[serde(default)]
    pub window_occluded: bool,
    #[serde(default)]
    pub vsync: bool,
    pub chunk_count: u32,
    pub entity_count: u32,
}

pub struct Benchmark {
    start: Instant,
    samples: Vec<FrameSample>,
    spikes: Vec<SpikeSample>,
    warmup_remaining: u32,
    gpu_name: String,
    resolution: (u32, u32),
    render_distance: u32,
}

#[derive(serde::Serialize)]
pub struct BenchmarkResult {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub gpu: String,
    pub resolution: [u32; 2],
    pub render_distance: u32,
    pub timestamp: String,
    pub total_frames: u32,
    pub duration_secs: f32,
    pub avg_fps: f32,
    pub min_fps: f32,
    pub max_fps: f32,
    pub avg_frame_ms: f32,
    pub p1_frame_ms: f32,
    pub p99_frame_ms: f32,
    pub avg_fence_ms: f32,
    pub avg_cull_ms: f32,
    pub avg_draw_ms: f32,
    pub peak_chunk_count: u32,
    pub peak_entity_count: u32,
    pub spike_count: u32,
    pub spikes: Vec<SpikeSample>,
}

impl Benchmark {
    pub fn new(gpu_name: &str, width: u32, height: u32, render_distance: u32) -> Self {
        Self {
            start: Instant::now(),
            samples: Vec::with_capacity(6000),
            spikes: Vec::new(),
            warmup_remaining: WARMUP_FRAMES,
            gpu_name: gpu_name.to_owned(),
            resolution: (width, height),
            render_distance,
        }
    }

    pub fn record_frame(
        &mut self,
        frame_ms: f32,
        timings: &RenderTimings,
        cpu_update_ms: f32,
        render_wall_ms: f32,
        phases: UpdatePhases,
        effective_fps_limit: Option<u32>,
        window_occluded: bool,
        vsync: bool,
        chunk_count: u32,
        entity_count: u32,
    ) -> bool {
        if self.warmup_remaining > 0 {
            self.warmup_remaining -= 1;
            if self.warmup_remaining == 0 {
                self.start = Instant::now();
            }
            return false;
        }

        let sample = FrameSample {
            frame_ms,
            fence_ms: timings.fence_ms,
            acquire_ms: timings.acquire_ms,
            present_ms: timings.present_ms,
            render_prepare_ms: timings.render_prepare_ms,
            render_setup_ms: timings.render_setup_ms,
            submit_ms: timings.submit_ms,
            cull_ms: timings.cull_ms,
            draw_ms: timings.draw_ms,
            chunk_draw_ms: timings.chunk_draw_ms,
            entity_draw_ms: timings.entity_draw_ms,
            block_entity_draw_ms: timings.block_entity_draw_ms,
            be_model_ms: timings.be_model_ms,
            be_sign_text_ms: timings.be_sign_text_ms,
            be_model_draws: timings.be_model_draws,
            be_sign_vertices: timings.be_sign_vertices,
            item_entity_draw_ms: timings.item_entity_draw_ms,
            environment_draw_ms: timings.environment_draw_ms,
            hud_draw_ms: timings.hud_draw_ms,
            cpu_update_ms,
            fixed_tick_ms: phases.fixed_tick_ms,
            fixed_tick_count: phases.fixed_tick_count,
            be_extract_ms: phases.be_extract_ms,
            render_wall_ms,
            net_decode_ms: phases.net_decode_ms,
            visibility_ms: phases.visibility_ms,
            rescan_ms: phases.rescan_ms,
            mesh_drain_ms: phases.mesh_drain_ms,
            upload_ms: phases.upload_ms,
            unaccounted_ms: frame_ms - cpu_update_ms - render_wall_ms,
            effective_fps_limit,
            window_occluded,
            vsync,
            chunk_count,
            entity_count,
        };

        if frame_ms > SPIKE_THRESHOLD_MS {
            self.spikes.push(SpikeSample {
                frame_index: self.samples.len() as u32,
                frame_ms: sample.frame_ms,
                fence_ms: sample.fence_ms,
                acquire_ms: sample.acquire_ms,
                present_ms: sample.present_ms,
                render_prepare_ms: sample.render_prepare_ms,
                render_setup_ms: sample.render_setup_ms,
                submit_ms: sample.submit_ms,
                cull_ms: sample.cull_ms,
                draw_ms: sample.draw_ms,
                chunk_draw_ms: sample.chunk_draw_ms,
                entity_draw_ms: sample.entity_draw_ms,
                block_entity_draw_ms: sample.block_entity_draw_ms,
                be_model_ms: sample.be_model_ms,
                be_sign_text_ms: sample.be_sign_text_ms,
                be_model_draws: sample.be_model_draws,
                be_sign_vertices: sample.be_sign_vertices,
                item_entity_draw_ms: sample.item_entity_draw_ms,
                environment_draw_ms: sample.environment_draw_ms,
                hud_draw_ms: sample.hud_draw_ms,
                cpu_update_ms: sample.cpu_update_ms,
                fixed_tick_ms: sample.fixed_tick_ms,
                fixed_tick_count: sample.fixed_tick_count,
                be_extract_ms: sample.be_extract_ms,
                render_wall_ms: sample.render_wall_ms,
                net_decode_ms: sample.net_decode_ms,
                visibility_ms: sample.visibility_ms,
                rescan_ms: sample.rescan_ms,
                mesh_drain_ms: sample.mesh_drain_ms,
                upload_ms: sample.upload_ms,
                unaccounted_ms: sample.unaccounted_ms,
                effective_fps_limit: sample.effective_fps_limit,
                window_occluded: sample.window_occluded,
                vsync: sample.vsync,
                chunk_count: sample.chunk_count,
                entity_count: sample.entity_count,
            });
        }

        self.samples.push(sample);
        self.start.elapsed().as_secs_f32() >= DURATION_SECS
    }

    pub fn finish(self, game_dir: &Path) -> BenchmarkResult {
        let count = self.samples.len().max(1);
        let mut frame_times: Vec<f32> = self.samples.iter().map(|s| s.frame_ms).collect();
        frame_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let sum: f32 = frame_times.iter().sum();
        let avg_ms = sum / count as f32;
        let p1_idx = ((count as f32 * 0.99) as usize).min(count - 1);
        let p99_idx = (count as f32 * 0.01) as usize;

        let fence_sum: f32 = self.samples.iter().map(|s| s.fence_ms).sum();
        let cull_sum: f32 = self.samples.iter().map(|s| s.cull_ms).sum();
        let draw_sum: f32 = self.samples.iter().map(|s| s.draw_ms).sum();
        let peak_chunks = self
            .samples
            .iter()
            .map(|s| s.chunk_count)
            .max()
            .unwrap_or(0);
        let peak_entities = self
            .samples
            .iter()
            .map(|s| s.entity_count)
            .max()
            .unwrap_or(0);

        let now = iso8601_utc_now();

        let result = BenchmarkResult {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            gpu: self.gpu_name,
            resolution: [self.resolution.0, self.resolution.1],
            render_distance: self.render_distance,
            timestamp: now,
            total_frames: count as u32,
            duration_secs: DURATION_SECS,
            avg_fps: 1000.0 / avg_ms,
            min_fps: 1000.0 / frame_times[p1_idx],
            max_fps: 1000.0 / frame_times[p99_idx].max(0.001),
            avg_frame_ms: avg_ms,
            p1_frame_ms: frame_times[p1_idx],
            p99_frame_ms: frame_times[p99_idx],
            avg_fence_ms: fence_sum / count as f32,
            avg_cull_ms: cull_sum / count as f32,
            avg_draw_ms: draw_sum / count as f32,
            peak_chunk_count: peak_chunks,
            peak_entity_count: peak_entities,
            spike_count: self.spikes.len() as u32,
            spikes: self.spikes,
        };

        let path = game_dir.join("benchmark.json");
        match write_result_json(&path, &result) {
            Ok(()) => tracing::info!("Benchmark saved to {}", path.display()),
            Err(error) => {
                tracing::error!("Failed to save benchmark to {}: {error}", path.display())
            }
        }

        result
    }

    pub fn progress(&self) -> f32 {
        if self.warmup_remaining > 0 {
            return 0.0;
        }
        (self.start.elapsed().as_secs_f32() / DURATION_SECS).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_json_write_reports_success_and_io_failure() {
        let dir = std::env::temp_dir().join(format!("pomme-benchmark-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("benchmark.json");
        write_result_json(&path, &serde_json::json!({"avg_fps": 120.0})).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap()["avg_fps"],
            120.0
        );
        assert!(
            write_result_json(&dir.join("missing/benchmark.json"), &serde_json::json!({})).is_err()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn frame_sample_timing_and_legacy_defaults() {
        let legacy: FrameSample = serde_json::from_str(
            r#"{"frame_ms":93.53,"fence_ms":0.008,"cull_ms":0.0,"draw_ms":0.0,"chunk_count":1,"entity_count":2}"#,
        )
        .unwrap();
        assert_eq!(legacy.chunk_draw_ms, 0.0);
        assert_eq!(legacy.acquire_ms, 0.0);
        assert_eq!(legacy.present_ms, 0.0);
        assert_eq!(legacy.fixed_tick_ms, 0.0);
        assert_eq!(legacy.fixed_tick_count, 0);
        assert_eq!(legacy.be_extract_ms, 0.0);
        assert_eq!(legacy.entity_draw_ms, 0.0);
        assert_eq!(legacy.block_entity_draw_ms, 0.0);
        assert_eq!(legacy.be_model_ms, 0.0);
        assert_eq!(legacy.be_sign_text_ms, 0.0);
        assert_eq!(legacy.be_model_draws, 0);
        assert_eq!(legacy.be_sign_vertices, 0);
        assert_eq!(legacy.item_entity_draw_ms, 0.0);
        assert_eq!(legacy.environment_draw_ms, 0.0);
        assert_eq!(legacy.hud_draw_ms, 0.0);
        assert_eq!(legacy.cpu_update_ms, 0.0);
        assert_eq!(legacy.net_decode_ms, 0.0);
        assert_eq!(legacy.visibility_ms, 0.0);
        assert_eq!(legacy.rescan_ms, 0.0);
        assert_eq!(legacy.mesh_drain_ms, 0.0);
        assert_eq!(legacy.upload_ms, 0.0);
        assert_eq!(legacy.unaccounted_ms, 0.0);
        assert_eq!(legacy.effective_fps_limit, None);

        let legacy_spike: SpikeSample = serde_json::from_str(
            r#"{"frame_index":0,"frame_ms":93.53,"fence_ms":0.008,"cull_ms":0.0,"draw_ms":0.0,"chunk_count":1,"entity_count":2}"#,
        )
        .unwrap();
        assert_eq!(legacy_spike.acquire_ms, 0.0);
        assert_eq!(legacy_spike.present_ms, 0.0);
        assert_eq!(legacy_spike.fixed_tick_ms, 0.0);
        assert_eq!(legacy_spike.fixed_tick_count, 0);
        assert_eq!(legacy_spike.be_extract_ms, 0.0);
        assert_eq!(legacy_spike.be_model_ms, 0.0);
        assert_eq!(legacy_spike.be_sign_text_ms, 0.0);
        assert_eq!(legacy_spike.be_model_draws, 0);
        assert_eq!(legacy_spike.be_sign_vertices, 0);
        assert_eq!(legacy_spike.net_decode_ms, 0.0);
        assert_eq!(legacy_spike.visibility_ms, 0.0);
        assert_eq!(legacy_spike.rescan_ms, 0.0);
        assert_eq!(legacy_spike.mesh_drain_ms, 0.0);
        assert_eq!(legacy_spike.upload_ms, 0.0);

        let old_wait_key: FrameSample = serde_json::from_str(
            r#"{"frame_ms":93.53,"fence_ms":0.008,"cull_ms":0.0,"draw_ms":0.0,"frame_wait_ms":12.5,"chunk_count":1,"entity_count":2}"#,
        )
        .unwrap();
        assert_eq!(old_wait_key.unaccounted_ms, 12.5);

        let mut bench = Benchmark::new("test", 1280, 720, 8);
        let timings = RenderTimings {
            acquire_ms: 0.3,
            present_ms: 0.4,
            render_prepare_ms: 0.5,
            render_setup_ms: 0.6,
            submit_ms: 0.7,
            draw_ms: 3.0,
            be_model_ms: 1.25,
            be_sign_text_ms: 0.75,
            be_model_draws: 4,
            be_sign_vertices: 36,
            ..Default::default()
        };
        let phases = UpdatePhases {
            fixed_tick_ms: 1.2,
            fixed_tick_count: 2,
            be_extract_ms: 1.3,
            net_decode_ms: 1.0,
            visibility_ms: 2.0,
            rescan_ms: 3.0,
            mesh_drain_ms: 4.0,
            upload_ms: 5.0,
            ..Default::default()
        };
        for _ in 0..WARMUP_FRAMES {
            assert!(!bench.record_frame(
                50.0,
                &timings,
                20.0,
                25.0,
                phases,
                Some(60),
                false,
                true,
                1,
                2
            ));
        }
        assert!(!bench.record_frame(
            100.0,
            &timings,
            30.0,
            40.0,
            phases,
            Some(60),
            false,
            true,
            1,
            2
        ));
        let sample = &bench.samples[0];
        assert_eq!(sample.cpu_update_ms, 30.0);
        assert_eq!(sample.fixed_tick_ms, 1.2);
        assert_eq!(sample.fixed_tick_count, 2);
        assert_eq!(sample.be_extract_ms, 1.3);
        assert_eq!(sample.acquire_ms, 0.3);
        assert_eq!(sample.present_ms, 0.4);
        assert_eq!(sample.render_prepare_ms, 0.5);
        assert_eq!(sample.render_setup_ms, 0.6);
        assert_eq!(sample.submit_ms, 0.7);
        assert_eq!(sample.render_wall_ms, 40.0);
        assert_eq!(sample.be_model_ms, 1.25);
        assert_eq!(sample.be_sign_text_ms, 0.75);
        assert_eq!(sample.be_model_draws, 4);
        assert_eq!(sample.be_sign_vertices, 36);
        assert_eq!(sample.net_decode_ms, 1.0);
        assert_eq!(sample.visibility_ms, 2.0);
        assert_eq!(sample.rescan_ms, 3.0);
        assert_eq!(sample.mesh_drain_ms, 4.0);
        assert_eq!(sample.upload_ms, 5.0);
        assert_eq!(sample.unaccounted_ms, 30.0);
        assert_eq!(bench.spikes[0].be_model_ms, 1.25);
        assert_eq!(bench.spikes[0].be_sign_text_ms, 0.75);
        assert_eq!(bench.spikes[0].be_model_draws, 4);
        assert_eq!(bench.spikes[0].be_sign_vertices, 36);
        assert_eq!(bench.spikes[0].net_decode_ms, 1.0);
        assert_eq!(bench.spikes[0].visibility_ms, 2.0);
        assert_eq!(bench.spikes[0].rescan_ms, 3.0);
        assert_eq!(bench.spikes[0].mesh_drain_ms, 4.0);
        assert_eq!(bench.spikes[0].upload_ms, 5.0);
        assert_eq!(bench.spikes[0].unaccounted_ms, 30.0);
        let json = serde_json::to_value(sample).unwrap();
        assert_eq!(json["unaccounted_ms"], 30.0);
        assert_eq!(json["net_decode_ms"], 1.0);
        assert_eq!(json["visibility_ms"], 2.0);
        assert_eq!(json["rescan_ms"], 3.0);
        assert_eq!(json["mesh_drain_ms"], 4.0);
        assert_eq!(json["upload_ms"], 5.0);
        assert!(json.get("frame_wait_ms").is_none());
        assert_eq!(sample.effective_fps_limit, Some(60));
        assert!(sample.vsync);
    }
}

/// Lowest render distance to drop to during the chunk-load reset phase.
pub const CHUNK_LOAD_MIN_RD: u32 = 2;
/// Minimum time to hold the minimum render distance before the timed load can
/// start, so the server has a chance to begin unloading the far chunks.
const CHUNK_RESET_MIN_SECS: f32 = 0.75;
/// The reset is done once the loaded-chunk count has stopped dropping for this
/// long — i.e. the server has finished unloading — regardless of latency.
const CHUNK_RESET_STABLE_SECS: f32 = 0.5;
/// Loading is done once the loaded-chunk count holds steady for this long —
/// long enough to ride out the server's inter-batch gaps at high render
/// distances, so a mid-stream pause isn't mistaken for completion.
const CHUNK_LOAD_STALL_SECS: f32 = 8.0;
/// ...or as soon as this fraction of the target radius's columns have loaded.
const CHUNK_LOAD_COMPLETE_FRAC: f32 = 0.98;
/// Safety cap so a stalled/capped load can't run forever.
const CHUNK_TIMEOUT_SECS: f32 = 90.0;
/// First run(s) are discarded as warmup (cold disk/network caches).
pub const CHUNK_LOAD_WARMUP_RUNS: u32 = 1;
/// Runs that actually count toward the averaged result.
pub const CHUNK_LOAD_MEASURED_RUNS: u32 = 3;
const CHUNK_LOAD_TOTAL_RUNS: u32 = CHUNK_LOAD_WARMUP_RUNS + CHUNK_LOAD_MEASURED_RUNS;
const MEASUREMENT_NOTE: &str =
    "frame_ms measured with entities, weather, and HUD hidden (top-down benchmark view)";

/// Debug builds run unoptimized, so their timings are far slower and not
/// comparable to release — results record and surface which one produced them.
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

pub fn build_profile() -> &'static str {
    if is_debug_build() { "debug" } else { "release" }
}

/// Columns in a fully-loaded square of the given radius: (2r+1)².
fn expected_columns(rd: u32) -> u32 {
    let d = 2 * rd + 1;
    d * d
}

/// Infer the loaded radius from a (roughly square) loaded area: count ≈
/// (2r+1)². Servers often don't advertise their view distance (proxies, dynamic
/// VD), so this is what actually loaded — the honest number when the target is
/// unreachable.
fn radius_from_chunk_count(count: u32) -> u32 {
    if count == 0 {
        return 0;
    }
    (((count as f32).sqrt() - 1.0) / 2.0).round().max(0.0) as u32
}

/// `update_game`'s CPU phase timings — the per-frame work not covered by the
/// render timings. Set each frame and folded into [`FrameBreakdown`].
/// `update_ms` is the whole-`update_game` wall time (including the render
/// call); if it is far below `total_ms`, the hitch is outside `update_game`
/// (framerate limiter / OS scheduling / inter-frame gap) rather than in any CPU
/// phase.
#[derive(Clone, Copy, Default, serde::Serialize)]
pub struct UpdatePhases {
    pub update_ms: f32,
    pub cpu_update_ms: f32,
    pub fixed_tick_ms: f32,
    pub fixed_tick_count: u32,
    pub be_extract_ms: f32,
    pub render_wall_ms: f32,
    pub net_decode_ms: f32,
    pub visibility_ms: f32,
    pub rescan_ms: f32,
    pub mesh_drain_ms: f32,
    pub upload_ms: f32,
}

/// Phase split of a run's single worst frame, to localize a hitch. `total_ms`
/// is the wall-clock frame (`raw_dt`); `render_ms` the `render_frame` portion
/// (which includes `fence_ms`, the GPU-bound wait); the `update` phases cover
/// the rest. All sub-timings reflect the same prior frame `raw_dt` measures, so
/// the split lines up; whatever `total_ms` exceeds the parts is time spent
/// outside `update_game` (limiter / OS scheduling / inter-frame gap).
#[derive(Clone, Default, serde::Serialize)]
pub struct FrameBreakdown {
    pub total_ms: f32,
    pub render_ms: f32,
    pub fence_ms: f32,
    pub acquire_ms: f32,
    pub cull_ms: f32,
    pub present_ms: f32,
    #[serde(flatten)]
    pub update: UpdatePhases,
}

/// One reset→load cycle's measurements.
#[derive(Clone, serde::Serialize)]
pub struct ChunkLoadRun {
    pub chunk_count: u32,
    pub load_secs: f32,
    pub chunks_per_sec: f32,
    pub time_to_first_secs: f32,
    pub avg_frame_ms: f32,
    pub worst_frame_ms: f32,
    pub mesh_total_secs: f32,
    pub mesh_avg_ms: f32,
    pub queue_avg_ms: f32,
    pub worst_frame_breakdown: FrameBreakdown,
}

#[derive(Clone, serde::Serialize)]
pub struct ChunkLoadResult {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub gpu: String,
    pub vulkan: String,
    pub cpu_threads: u32,
    pub resolution: [u32; 2],
    pub timestamp: String,
    /// Where the benchmark was taken — results vary a lot by terrain, so this
    /// is the context that makes two pastes comparable (or not).
    pub player_pos: [f64; 3],
    pub target_rd: u32,
    /// Server-advertised cap, if it sent one (else equals `target_rd`).
    pub effective_rd: u32,
    /// Radius actually loaded, inferred from `chunk_count` — the real distance
    /// when the server caps or never advertises its view distance.
    pub achieved_rd: u32,
    /// Number of measured (non-warmup) runs the scalar fields below average
    /// over.
    pub runs: u32,
    pub warmup_runs: u32,
    pub chunk_count: u32,
    /// Wall-clock from raising the render distance to the last chunk landing.
    pub load_secs: f32,
    pub chunks_per_sec: f32,
    /// Time from the raise to the first new chunk landing — server/network
    /// response latency before throughput kicks in.
    pub time_to_first_secs: f32,
    /// Average and worst frame time observed while loading — the hitching you
    /// feel as chunks mesh and upload.
    pub avg_frame_ms: f32,
    pub worst_frame_ms: f32,
    /// Summed worker meshing wall time across the run — how much of the load
    /// was actually spent meshing (divide by worker threads for the
    /// wall-clock lower bound).
    pub mesh_total_secs: f32,
    /// Per-job averages: meshing wall time, and time waiting in the mesh
    /// queue before a worker picked the job up (queue-bound vs mesh-bound).
    pub mesh_avg_ms: f32,
    pub queue_avg_ms: f32,
    pub runs_detail: Vec<ChunkLoadRun>,
    /// Phase split of the worst frame across the measured runs — what the spike
    /// was actually spent on.
    pub worst_frame_breakdown: FrameBreakdown,
    /// "debug" or "release" — see [`build_profile`].
    pub profile: String,
    pub measurement_note: String,
}

impl ChunkLoadResult {
    pub fn save(&self, game_dir: &Path) {
        let path = game_dir.join("chunk_load.json");
        match write_result_json(&path, self) {
            Ok(()) => tracing::info!("Chunk load result saved to {}", path.display()),
            Err(error) => tracing::error!(
                "Failed to save chunk load result to {}: {error}",
                path.display()
            ),
        }
    }
}

enum ChunkPhase {
    Reset,
    Load,
}

/// What the per-frame driver should do with the render distance this frame.
pub enum ChunkLoadStep {
    /// Nothing to apply; keep waiting/measuring.
    Wait,
    /// Apply this render distance and sync it to the server — the timed load
    /// starts now.
    Load(u32),
    /// Loading finished; the driver should restore the original render
    /// distance.
    Done(Box<ChunkLoadResult>),
}

/// Measures how long it takes to load every chunk in a chosen render-distance
/// radius. First drops to [`CHUNK_LOAD_MIN_RD`] so the server unloads the far
/// chunks, then raises to the target and times the fresh load until the
/// loaded-chunk count stops rising.
pub struct ChunkLoadBench {
    phase: ChunkPhase,
    target_rd: u32,
    effective_rd: u32,
    original_rd: u32,
    gpu_name: String,
    vulkan: String,
    resolution: (u32, u32),
    player_pos: [f64; 3],
    reset_start: Instant,
    start: Instant,
    last_count: u32,
    last_change: Instant,
    /// Loaded count when the timed load began (the reset baseline).
    baseline_count: u32,
    /// When the first chunk past the baseline landed.
    first_load_at: Option<Instant>,
    frame_ms_sum: f32,
    frame_ms_max: f32,
    /// Phase split of the current run's worst frame so far.
    worst_breakdown: FrameBreakdown,
    frame_samples: u32,
    mesh_ms_sum: f32,
    queue_ms_sum: f32,
    mesh_jobs: u32,
    /// How many reset→load cycles have finished (warmup + measured).
    runs_done: u32,
    completed: Vec<ChunkLoadRun>,
}

impl ChunkLoadBench {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_rd: u32,
        original_rd: u32,
        server_rd: u32,
        gpu_name: &str,
        vulkan: &str,
        width: u32,
        height: u32,
        player_pos: [f64; 3],
    ) -> Self {
        let effective_rd = if server_rd > 0 {
            target_rd.min(server_rd)
        } else {
            target_rd
        };
        let now = Instant::now();
        Self {
            phase: ChunkPhase::Reset,
            target_rd,
            effective_rd,
            original_rd,
            gpu_name: gpu_name.to_owned(),
            vulkan: vulkan.to_owned(),
            resolution: (width, height),
            player_pos,
            reset_start: now,
            start: now,
            last_count: 0,
            last_change: now,
            baseline_count: 0,
            first_load_at: None,
            frame_ms_sum: 0.0,
            frame_ms_max: 0.0,
            worst_breakdown: FrameBreakdown::default(),
            frame_samples: 0,
            mesh_ms_sum: 0.0,
            queue_ms_sum: 0.0,
            mesh_jobs: 0,
            runs_done: 0,
            completed: Vec::new(),
        }
    }

    /// Record one drained mesh job's worker timing; only counted while the
    /// timed load phase is running.
    pub fn record_mesh(&mut self, queue_ms: f32, mesh_ms: f32) {
        if matches!(self.phase, ChunkPhase::Load) {
            self.queue_ms_sum += queue_ms;
            self.mesh_ms_sum += mesh_ms;
            self.mesh_jobs += 1;
        }
    }

    pub fn update(
        &mut self,
        loaded_count: u32,
        frame_ms: f32,
        timings: &RenderTimings,
        phases: UpdatePhases,
    ) -> ChunkLoadStep {
        match self.phase {
            ChunkPhase::Reset => {
                // Wait for the unload to settle (count stops dropping) so the
                // timed load always starts from a clean low baseline, even on a
                // laggy connection.
                if loaded_count != self.last_count {
                    self.last_count = loaded_count;
                    self.last_change = Instant::now();
                }
                let min_elapsed = self.reset_start.elapsed().as_secs_f32() >= CHUNK_RESET_MIN_SECS;
                let settled = self.last_change.elapsed().as_secs_f32() >= CHUNK_RESET_STABLE_SECS;
                if min_elapsed && settled {
                    let now = Instant::now();
                    self.phase = ChunkPhase::Load;
                    self.start = now;
                    self.last_change = now;
                    self.last_count = loaded_count;
                    self.baseline_count = loaded_count;
                    ChunkLoadStep::Load(self.target_rd)
                } else {
                    ChunkLoadStep::Wait
                }
            }
            ChunkPhase::Load => {
                self.frame_ms_sum += frame_ms;
                if frame_ms > self.frame_ms_max {
                    self.frame_ms_max = frame_ms;
                    self.worst_breakdown = FrameBreakdown {
                        total_ms: frame_ms,
                        render_ms: timings.frame_ms,
                        fence_ms: timings.fence_ms,
                        acquire_ms: timings.acquire_ms,
                        cull_ms: timings.cull_ms,
                        present_ms: timings.present_ms,
                        update: phases,
                    };
                }
                self.frame_samples += 1;

                if loaded_count != self.last_count {
                    self.last_count = loaded_count;
                    self.last_change = Instant::now();
                }
                if self.first_load_at.is_none() && loaded_count > self.baseline_count {
                    self.first_load_at = Some(Instant::now());
                }

                // Done when nearly the whole radius has loaded, or the stream has
                // genuinely stalled (a capped/slow server), or the safety timeout.
                let near_complete = loaded_count as f32
                    >= expected_columns(self.target_rd) as f32 * CHUNK_LOAD_COMPLETE_FRAC;
                let stalled = loaded_count > 0
                    && self.last_change.elapsed().as_secs_f32() >= CHUNK_LOAD_STALL_SECS;
                let timeout = self.start.elapsed().as_secs_f32() >= CHUNK_TIMEOUT_SECS;
                if near_complete || stalled || timeout {
                    let load_secs = self
                        .last_change
                        .saturating_duration_since(self.start)
                        .as_secs_f32();
                    let chunks_per_sec = if load_secs > 0.0 {
                        loaded_count as f32 / load_secs
                    } else {
                        0.0
                    };
                    let time_to_first_secs = self
                        .first_load_at
                        .map(|t| t.saturating_duration_since(self.start).as_secs_f32())
                        .unwrap_or(0.0);
                    let avg_frame_ms = if self.frame_samples > 0 {
                        self.frame_ms_sum / self.frame_samples as f32
                    } else {
                        0.0
                    };
                    let jobs = self.mesh_jobs.max(1) as f32;
                    self.completed.push(ChunkLoadRun {
                        chunk_count: loaded_count,
                        load_secs,
                        chunks_per_sec,
                        time_to_first_secs,
                        avg_frame_ms,
                        worst_frame_ms: self.frame_ms_max,
                        mesh_total_secs: self.mesh_ms_sum / 1000.0,
                        mesh_avg_ms: self.mesh_ms_sum / jobs,
                        queue_avg_ms: self.queue_ms_sum / jobs,
                        worst_frame_breakdown: self.worst_breakdown.clone(),
                    });
                    self.runs_done += 1;

                    if self.runs_done >= CHUNK_LOAD_TOTAL_RUNS {
                        return ChunkLoadStep::Done(Box::new(self.aggregate()));
                    }

                    // Next cycle: drop back to the minimum RD and re-enter the reset
                    // phase so the server unloads before the next timed load.
                    let now = Instant::now();
                    self.phase = ChunkPhase::Reset;
                    self.reset_start = now;
                    self.last_change = now;
                    self.last_count = loaded_count;
                    self.first_load_at = None;
                    self.frame_ms_sum = 0.0;
                    self.frame_ms_max = 0.0;
                    self.worst_breakdown = FrameBreakdown::default();
                    self.frame_samples = 0;
                    self.mesh_ms_sum = 0.0;
                    self.queue_ms_sum = 0.0;
                    self.mesh_jobs = 0;
                    ChunkLoadStep::Load(CHUNK_LOAD_MIN_RD)
                } else {
                    ChunkLoadStep::Wait
                }
            }
        }
    }

    /// Average the measured (non-warmup) runs into the shareable result.
    fn aggregate(&self) -> ChunkLoadResult {
        let measured = &self.completed[CHUNK_LOAD_WARMUP_RUNS as usize..];
        let n = measured.len().max(1) as f32;
        let avg = |sel: fn(&ChunkLoadRun) -> f32| measured.iter().map(sel).sum::<f32>() / n;
        let chunk_count =
            (measured.iter().map(|r| r.chunk_count as f32).sum::<f32>() / n).round() as u32;
        ChunkLoadResult {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            gpu: self.gpu_name.clone(),
            vulkan: self.vulkan.clone(),
            cpu_threads: std::thread::available_parallelism()
                .map(|n| n.get() as u32)
                .unwrap_or(0),
            resolution: [self.resolution.0, self.resolution.1],
            timestamp: iso8601_utc_now(),
            player_pos: self.player_pos,
            target_rd: self.target_rd,
            effective_rd: self.effective_rd,
            achieved_rd: radius_from_chunk_count(chunk_count),
            runs: measured.len() as u32,
            warmup_runs: CHUNK_LOAD_WARMUP_RUNS,
            chunk_count,
            load_secs: avg(|r| r.load_secs),
            chunks_per_sec: avg(|r| r.chunks_per_sec),
            time_to_first_secs: avg(|r| r.time_to_first_secs),
            avg_frame_ms: avg(|r| r.avg_frame_ms),
            mesh_total_secs: avg(|r| r.mesh_total_secs),
            mesh_avg_ms: avg(|r| r.mesh_avg_ms),
            queue_avg_ms: avg(|r| r.queue_avg_ms),
            worst_frame_ms: measured
                .iter()
                .map(|r| r.worst_frame_ms)
                .fold(0.0, f32::max),
            worst_frame_breakdown: measured
                .iter()
                .max_by(|a, b| {
                    a.worst_frame_ms
                        .partial_cmp(&b.worst_frame_ms)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|r| r.worst_frame_breakdown.clone())
                .unwrap_or_default(),
            runs_detail: measured.to_vec(),
            profile: build_profile().to_owned(),
            measurement_note: MEASUREMENT_NOTE.to_owned(),
        }
    }

    pub fn original_rd(&self) -> u32 {
        self.original_rd
    }

    pub fn target_rd(&self) -> u32 {
        self.target_rd
    }

    pub fn effective_rd(&self) -> u32 {
        self.effective_rd
    }

    /// 1-based index of the run currently in progress (warmup runs included).
    pub fn current_run(&self) -> u32 {
        (self.runs_done + 1).min(CHUNK_LOAD_TOTAL_RUNS)
    }

    pub fn total_runs(&self) -> u32 {
        CHUNK_LOAD_TOTAL_RUNS
    }

    pub fn loaded(&self) -> u32 {
        self.last_count
    }

    pub fn resetting(&self) -> bool {
        matches!(self.phase, ChunkPhase::Reset)
    }
}

const PASTE_URL: &str = "https://paste.marshall.dev/documents";

/// Progress of an in-flight (or finished) benchmark-result upload, shared
/// between the render thread and the spawned upload task.
#[derive(Clone)]
pub enum UploadStatus {
    Uploading,
    Done { url: String, copied: bool },
    Failed(String),
}

pub type UploadHandle = std::sync::Arc<std::sync::Mutex<UploadStatus>>;

#[derive(serde::Deserialize)]
struct DocResponse {
    key: String,
}

/// POST the result JSON to paste.marshall.dev and return the shareable link.
async fn post_paste(json: String) -> Result<String, String> {
    let resp = reqwest::Client::new()
        .post(PASTE_URL)
        .header(reqwest::header::CONTENT_TYPE, "text/plain")
        .body(json)
        .send()
        .await
        .map_err(|e| format!("Upload failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Upload failed: HTTP {}", resp.status()));
    }
    let doc: DocResponse = resp
        .json()
        .await
        .map_err(|e| format!("Upload response parse failed: {e}"))?;
    Ok(format!("https://paste.marshall.dev/{}", doc.key))
}

/// Spawn a background upload of `json` and copy the resulting link to the
/// clipboard. Returns a handle the UI polls for status.
pub fn upload_result(rt: &tokio::runtime::Runtime, json: String) -> UploadHandle {
    let handle: UploadHandle = std::sync::Arc::new(std::sync::Mutex::new(UploadStatus::Uploading));
    let out = std::sync::Arc::clone(&handle);
    rt.spawn(async move {
        let status = match post_paste(json).await {
            Ok(url) => {
                let copied = crate::ui::common::set_clipboard(&url);
                UploadStatus::Done { url, copied }
            }
            Err(e) => UploadStatus::Failed(e),
        };
        *out.lock().unwrap() = status;
    });
    handle
}
