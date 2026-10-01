//! One-shot unattended FPS run. Status deliberately contains no connection or
//! account data.
use std::path::PathBuf;
use std::time::{Duration, Instant, UNIX_EPOCH};

use crate::util::write_atomic;

const JOIN_TIMEOUT: Duration = Duration::from_secs(90);
const STABLE_TIMEOUT: Duration = Duration::from_secs(90);
const STABLE_FOR: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Joining,
    Stabilizing,
    Running,
    Finished,
}

pub(crate) struct AutoFps {
    path: PathBuf,
    run_id: String,
    benchmark_file: String,
    since: Instant,
    unchanged_since: Instant,
    last_chunks: u32,
    stable_frames: u32,
    focused: bool,
    phase: Phase,
    pub failed: bool,
    origin: Option<Scene>,
    previous: Option<Scene>,
}

/// World/player camera, excluding render-only head bob. Small physics jitter is
/// allowed.
#[derive(Clone, Debug)]
pub struct Scene {
    pub dimension: String,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub alive: bool,
    pub camera_ready: bool,
}

impl Scene {
    fn valid(&self) -> bool {
        self.alive
            && !self.dimension.is_empty()
            && self.position.iter().all(|n| n.is_finite())
            && self.yaw.is_finite()
            && self.pitch.is_finite()
    }

    fn close_to(&self, other: &Self) -> bool {
        self.dimension == other.dimension
            && self
                .position
                .iter()
                .zip(other.position)
                .all(|(a, b)| (a - b).abs() <= 2.0)
            && (self.yaw - other.yaw)
                .rem_euclid(360.0)
                .min((other.yaw - self.yaw).rem_euclid(360.0))
                <= 10.0
            && (self.pitch - other.pitch).abs() <= 10.0
    }
}

impl AutoFps {
    pub fn new(game_dir: &std::path::Path, requested_id: Option<&str>) -> Self {
        let now = Instant::now();
        // Random nonce unless launched by the parent with its own unpredictable nonce.
        let run_id = requested_id
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{:032x}", uuid::Uuid::new_v4().as_u128()));
        Self {
            path: game_dir.join(format!("auto-fps-benchmark-status-{run_id}.json")),
            benchmark_file: format!("benchmark-{run_id}.json"),
            run_id,
            since: now,
            unchanged_since: now,
            last_chunks: 0,
            stable_frames: 0,
            focused: false,
            phase: Phase::Joining,
            failed: false,
            origin: None,
            previous: None,
        }
    }

    fn write(
        &self,
        state: &str,
        reason: Option<&str>,
        modified_ms: Option<u128>,
    ) -> std::io::Result<()> {
        let status = serde_json::json!({
            "run_id": self.run_id,
            "state": state,
            "reason": reason,
            "benchmark_file": self.benchmark_file,
            "benchmark_modified_unix_ms": modified_ms,
            "scene_origin": self.origin.as_ref().map(|s| serde_json::json!({
                "dimension": s.dimension, "position": s.position, "yaw": s.yaw, "pitch": s.pitch,
            })),
        });
        write_atomic(
            &self.path,
            &serde_json::to_vec_pretty(&status).map_err(std::io::Error::other)?,
        )
    }

    pub fn start(&mut self) -> std::io::Result<()> {
        match std::fs::remove_file(self.path.with_file_name(&self.benchmark_file)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        self.write("running", None, None)
    }

    pub fn benchmark_path(&self, game_dir: &std::path::Path) -> PathBuf {
        game_dir.join(&self.benchmark_file)
    }

    /// Returns true only when an active measurement was interrupted.
    pub fn focus_changed(&mut self, focused: bool, now: Instant) -> bool {
        if self.phase == Phase::Finished {
            return false;
        }
        if !focused && self.phase == Phase::Running {
            self.fail("focus_lost");
            return true;
        }
        if self.focused != focused {
            self.focused = focused;
            self.unchanged_since = now;
            self.last_chunks = 0;
            self.stable_frames = 0;
        }
        false
    }

    pub fn fail(&mut self, reason: &'static str) {
        if self.phase == Phase::Finished {
            return;
        }
        self.phase = Phase::Finished;
        self.failed = true;
        if let Err(e) = self.write("failed", Some(reason), None) {
            tracing::error!("Failed to save auto FPS status: {e}");
        }
    }

    pub fn joined(&mut self, now: Instant) {
        if self.phase != Phase::Joining {
            return;
        }
        self.phase = Phase::Stabilizing;
        self.since = now;
        self.unchanged_since = now;
    }

    /// Safe to check without a rendered frame: does not advance scene or frame
    /// stability.
    pub fn timeout(&self, now: Instant) -> Option<&'static str> {
        let (limit, reason) = match self.phase {
            Phase::Joining => (JOIN_TIMEOUT, "join_timeout"),
            Phase::Stabilizing => (STABLE_TIMEOUT, "stability_timeout"),
            Phase::Running | Phase::Finished => return None,
        };
        (now.duration_since(self.since) >= limit).then_some(reason)
    }

    /// Call once per rendered game frame. Never start without nonzero,
    /// unchanged GPU-loaded chunks for at least three seconds across
    /// multiple frames.
    pub fn tick(
        &mut self,
        now: Instant,
        chunks: u32,
        scene: Option<Scene>,
    ) -> Result<bool, &'static str> {
        if !self.focused && self.phase == Phase::Stabilizing {
            return self.timeout(now).map_or(Ok(false), Err);
        }
        if self.phase == Phase::Stabilizing || self.phase == Phase::Running {
            let scene = scene.as_ref().ok_or("scene_invalid")?;
            if !scene.alive {
                return Err("player_dead");
            }
            if !scene.valid() || (self.phase == Phase::Running && !scene.camera_ready) {
                return Err("scene_invalid");
            }
            if self.previous.as_ref().is_some_and(|p| !scene.close_to(p))
                || self.origin.as_ref().is_some_and(|p| !scene.close_to(p))
            {
                return Err("scene_changed");
            }
            self.previous = Some(scene.clone());
        }
        let camera_ready = scene.as_ref().is_some_and(|s| s.camera_ready);
        match self.phase {
            Phase::Joining => self.timeout(now).map_or(Ok(false), Err),
            Phase::Stabilizing => {
                if let Some(reason) = self.timeout(now) {
                    return Err(reason);
                }
                if chunks == 0 || chunks != self.last_chunks {
                    self.last_chunks = chunks;
                    self.stable_frames = 1;
                    self.unchanged_since = now;
                } else {
                    self.stable_frames += 1;
                }
                if !camera_ready {
                    self.unchanged_since = now;
                    self.stable_frames = 0;
                }
                if camera_ready
                    && chunks > 0
                    && self.stable_frames >= 2
                    && now.duration_since(self.unchanged_since) >= STABLE_FOR
                {
                    self.origin = scene;
                    self.write("running", None, None)
                        .map_err(|_| "status_save_failed")?;
                    self.phase = Phase::Running;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Phase::Running | Phase::Finished => Ok(false),
        }
    }

    pub fn joining(&self) -> bool {
        self.phase == Phase::Joining
    }
    pub fn running(&self) -> bool {
        self.phase == Phase::Running
    }
    pub fn finished(&self) -> bool {
        self.phase == Phase::Finished
    }

    pub fn succeed(&mut self, game_dir: &std::path::Path) -> std::io::Result<()> {
        if self.phase != Phase::Running {
            return Err(std::io::Error::other("benchmark was not running"));
        }
        let path = self.benchmark_path(game_dir);
        let result: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path)?).map_err(std::io::Error::other)?;
        let valid = result
            .get("auto_fps_run_id")
            .and_then(serde_json::Value::as_str)
            == Some(&self.run_id)
            && result
                .get("total_frames")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|frames| frames > 0)
            && result
                .get("avg_fps")
                .and_then(serde_json::Value::as_f64)
                .is_some_and(|fps| fps.is_finite() && fps > 0.0);
        if !valid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "benchmark result is invalid or belongs to another run",
            ));
        }
        let modified = std::fs::metadata(path)?.modified()?;
        let ms = modified
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_millis();
        self.write("success", None, Some(ms))?;
        self.phase = Phase::Finished;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(ready: bool) -> Scene {
        Scene {
            dimension: "minecraft:overworld".into(),
            position: [0.0, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            alive: true,
            camera_ready: ready,
        }
    }

    #[test]
    fn startup_focus_transient_then_measurement_loss_fails() {
        let dir = crate::test_util::test_temp_dir("auto-focus");
        std::fs::create_dir_all(&dir).unwrap();
        let mut run = AutoFps::new(&dir, None);
        run.start().unwrap();
        let t = Instant::now();
        assert!(!run.focus_changed(false, t)); // Maximizing before the window is ready.
        run.joined(t);
        assert_eq!(
            run.tick(t + Duration::from_secs(1), 1, Some(scene(true))),
            Ok(false)
        );
        assert!(!run.focus_changed(true, t + Duration::from_secs(2)));
        assert_eq!(
            run.tick(t + Duration::from_secs(3), 1, Some(scene(true))),
            Ok(false)
        );
        assert!(!run.focus_changed(false, t + Duration::from_secs(4))); // Reset settle.
        assert!(!run.focus_changed(true, t + Duration::from_secs(5)));
        assert_eq!(
            run.tick(t + Duration::from_secs(6), 1, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(8), 1, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(10), 1, Some(scene(true))),
            Ok(true)
        );
        assert!(run.focus_changed(false, t + Duration::from_secs(11)));
        assert!(!run.focus_changed(true, t + Duration::from_secs(12)));
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&run.path).unwrap()).unwrap();
        assert_eq!(status["state"], "failed");
        assert_eq!(status["reason"], "focus_lost");
        assert!(!run.benchmark_path(&dir).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unfocused_run_times_out() {
        let dir = crate::test_util::test_temp_dir("auto-unfocused");
        let mut joining = AutoFps::new(&dir, None);
        let t = Instant::now();
        assert!(!joining.focus_changed(false, t));
        assert_eq!(joining.tick(t + JOIN_TIMEOUT, 0, None), Err("join_timeout"));
        let mut joined = AutoFps::new(&dir, None);
        joined.joined(t);
        assert_eq!(
            joined.tick(t + STABLE_TIMEOUT, 1, Some(scene(true))),
            Err("stability_timeout")
        );
    }

    #[test]
    fn timeout_without_rendered_frames_fails_status() {
        let dir = crate::test_util::test_temp_dir("auto-no-redraw");
        std::fs::create_dir_all(&dir).unwrap();
        for (joined, limit, reason) in [
            (false, JOIN_TIMEOUT, "join_timeout"),
            (true, STABLE_TIMEOUT, "stability_timeout"),
        ] {
            let mut run = AutoFps::new(&dir, None);
            run.start().unwrap();
            let start = run.since;
            if joined {
                run.joined(start);
            }
            assert_eq!(run.timeout(start + limit - Duration::from_nanos(1)), None);
            assert_eq!(run.timeout(start + limit), Some(reason));
            run.fail(reason);
            assert_eq!(run.timeout(start + limit), None);
            let status: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&run.path).unwrap()).unwrap();
            assert_eq!(status["state"], "failed");
            assert_eq!(status["reason"], reason);
            assert!(!run.benchmark_path(&dir).exists());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_runs_have_distinct_results() {
        let dir = crate::test_util::test_temp_dir("auto-concurrent");
        std::fs::create_dir_all(&dir).unwrap();
        let a = AutoFps::new(&dir, None);
        let b = AutoFps::new(&dir, None);
        assert_ne!(a.run_id, b.run_id);
        assert_ne!(a.benchmark_path(&dir), b.benchmark_path(&dir));
        assert_eq!(a.benchmark_file, format!("benchmark-{}.json", a.run_id));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn camera_section_must_be_ready_for_stable_window() {
        let dir = crate::test_util::test_temp_dir("auto-camera");
        std::fs::create_dir_all(&dir).unwrap();
        let mut run = AutoFps::new(&dir, None);
        let t = Instant::now();
        run.joined(t);
        run.focus_changed(true, t);
        assert_eq!(
            run.tick(t + Duration::from_secs(1), 9, Some(scene(false))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(5), 9, Some(scene(false))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(6), 9, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(10), 9, Some(scene(true))),
            Ok(true)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn death_teleport_and_dimension_fail_measuring() {
        let dir = crate::test_util::test_temp_dir("auto-scene");
        std::fs::create_dir_all(&dir).unwrap();
        let t = Instant::now();
        for (changed, expected) in [
            (
                Scene {
                    alive: false,
                    ..scene(true)
                },
                "player_dead",
            ),
            (
                Scene {
                    position: [100.0, 64.0, 0.0],
                    ..scene(true)
                },
                "scene_changed",
            ),
            (
                Scene {
                    dimension: "minecraft:the_nether".into(),
                    ..scene(true)
                },
                "scene_changed",
            ),
            (
                Scene {
                    camera_ready: false,
                    ..scene(true)
                },
                "scene_invalid",
            ),
        ] {
            let mut run = AutoFps::new(&dir, None);
            run.joined(t);
            run.focus_changed(true, t);
            assert_eq!(
                run.tick(t + Duration::from_secs(1), 5, Some(scene(true))),
                Ok(false)
            );
            assert_eq!(
                run.tick(t + Duration::from_secs(5), 5, Some(scene(true))),
                Ok(true)
            );
            assert_eq!(
                run.tick(t + Duration::from_secs(6), 5, Some(changed)),
                Err(expected)
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn succeed_uses_run_identity_and_valid_result_not_mtime() {
        let dir = crate::test_util::test_temp_dir("auto-result-validation");
        std::fs::create_dir_all(&dir).unwrap();

        let mut current = AutoFps::new(&dir, None);
        current.start().unwrap();
        current.phase = Phase::Running;
        let result = serde_json::json!({
            "auto_fps_run_id": current.run_id,
            "total_frames": 1200,
            "avg_fps": 60.0,
        });
        let path = current.benchmark_path(&dir);
        std::fs::write(&path, result.to_string()).unwrap();
        std::fs::File::open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(UNIX_EPOCH))
            .unwrap();
        current.succeed(&dir).unwrap();

        let mut stale = AutoFps::new(&dir, Some("a0000000000000000000000000000000"));
        std::fs::write(
            stale.benchmark_path(&dir),
            r#"{"total_frames":1200,"avg_fps":60}"#,
        )
        .unwrap();
        stale.start().unwrap();
        assert!(!stale.benchmark_path(&dir).exists());
        stale.phase = Phase::Running;
        assert!(stale.succeed(&dir).is_err());

        let mut invalid = AutoFps::new(&dir, None);
        invalid.start().unwrap();
        invalid.phase = Phase::Running;
        std::fs::write(invalid.benchmark_path(&dir), "{}").unwrap();
        assert_eq!(
            invalid.succeed(&dir).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stable_chunks_timeout_and_one_shot() {
        let dir = crate::test_util::test_temp_dir("auto-fps");
        std::fs::create_dir_all(&dir).unwrap();
        let mut run = AutoFps::new(&dir, None);
        let t = Instant::now();
        assert_eq!(
            run.tick(t + JOIN_TIMEOUT, 1, Some(scene(true))),
            Err("join_timeout")
        );
        run.joined(t);
        run.focus_changed(true, t);
        assert_eq!(
            run.tick(t + Duration::from_secs(1), 0, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(2), 5, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(4), 6, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(6), 6, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(8), 6, Some(scene(true))),
            Ok(true)
        );
        assert_eq!(
            run.tick(t + Duration::from_secs(9), 6, Some(scene(true))),
            Ok(false)
        );
        run.fail("disconnected");
        assert!(run.finished() && run.failed);
        assert_eq!(
            run.tick(t + Duration::from_secs(10), 6, Some(scene(true))),
            Ok(false)
        );
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run.path).unwrap()).unwrap();
        assert_eq!(status["reason"], "disconnected");
        let mut timeout = AutoFps::new(&dir, None);
        timeout.joined(t);
        assert_eq!(
            timeout.tick(t + STABLE_TIMEOUT, 5, Some(scene(false))),
            Err("stability_timeout")
        );
        let mut success = AutoFps::new(&dir, None);
        success.start().unwrap();
        assert!(success.succeed(&dir).is_err());
        success.joined(t);
        success.focus_changed(true, t);
        assert_eq!(
            success.tick(t + Duration::from_secs(1), 5, Some(scene(true))),
            Ok(false)
        );
        assert_eq!(
            success.tick(t + Duration::from_secs(5), 5, Some(scene(true))),
            Ok(true)
        );
        assert!(success.succeed(&dir).is_err()); // No result file yet.
        std::fs::write(
            success.benchmark_path(&dir),
            serde_json::json!({
                "auto_fps_run_id": success.run_id,
                "total_frames": 1200,
                "avg_fps": 60.0,
            })
            .to_string(),
        )
        .unwrap();
        success.succeed(&dir).unwrap();
        assert!(success.succeed(&dir).is_err());
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&success.path).unwrap()).unwrap();
        assert_eq!(status["state"], "success");
        assert!(status["benchmark_modified_unix_ms"].as_u64().is_some());
        assert_ne!(status["run_id"], "");
        assert_eq!(status["benchmark_file"], success.benchmark_file);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
