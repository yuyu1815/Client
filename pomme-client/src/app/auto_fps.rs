//! One-shot unattended FPS run. Status deliberately contains no connection or
//! account data.
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

pub(super) struct AutoFps {
    path: PathBuf,
    run_id: String,
    started_at: SystemTime,
    since: Instant,
    unchanged_since: Instant,
    last_chunks: u32,
    stable_frames: u32,
    phase: Phase,
    pub failed: bool,
}

impl AutoFps {
    pub fn new(game_dir: &std::path::Path) -> Self {
        let now = Instant::now();
        Self {
            path: game_dir.join("auto-fps-benchmark-status.json"),
            run_id: format!("{:016x}", uuid::Uuid::new_v4().as_u128() as u64),
            started_at: SystemTime::now(),
            since: now,
            unchanged_since: now,
            last_chunks: 0,
            stable_frames: 0,
            phase: Phase::Joining,
            failed: false,
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
            "benchmark_file": "benchmark.json",
            "benchmark_modified_unix_ms": modified_ms,
        });
        write_atomic(
            &self.path,
            &serde_json::to_vec_pretty(&status).map_err(std::io::Error::other)?,
        )
    }

    pub fn start(&mut self) -> std::io::Result<()> {
        self.write("running", None, None)
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

    /// Call once per rendered game frame. Never start without nonzero,
    /// unchanged GPU-loaded chunks for at least three seconds across
    /// multiple frames.
    pub fn tick(&mut self, now: Instant, chunks: u32) -> Result<bool, &'static str> {
        match self.phase {
            Phase::Joining => {
                if now.duration_since(self.since) >= JOIN_TIMEOUT {
                    Err("join_timeout")
                } else {
                    Ok(false)
                }
            }
            Phase::Stabilizing => {
                if now.duration_since(self.since) >= STABLE_TIMEOUT {
                    return Err("stability_timeout");
                }
                if chunks == 0 || chunks != self.last_chunks {
                    self.last_chunks = chunks;
                    self.stable_frames = 1;
                    self.unchanged_since = now;
                } else {
                    self.stable_frames += 1;
                }
                if chunks > 0
                    && self.stable_frames >= 2
                    && now.duration_since(self.unchanged_since) >= STABLE_FOR
                {
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
        let modified = std::fs::metadata(game_dir.join("benchmark.json"))?.modified()?;
        if modified < self.started_at {
            return Err(std::io::Error::other(
                "benchmark file was not updated by this run",
            ));
        }
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

    #[test]
    fn stable_chunks_timeout_and_one_shot() {
        let dir = crate::test_util::test_temp_dir("auto-fps");
        std::fs::create_dir_all(&dir).unwrap();
        let mut run = AutoFps::new(&dir);
        let t = Instant::now();
        assert_eq!(run.tick(t + JOIN_TIMEOUT, 1), Err("join_timeout"));
        run.joined(t);
        assert_eq!(run.tick(t + Duration::from_secs(1), 0), Ok(false));
        assert_eq!(run.tick(t + Duration::from_secs(2), 5), Ok(false));
        assert_eq!(run.tick(t + Duration::from_secs(4), 6), Ok(false));
        assert_eq!(run.tick(t + Duration::from_secs(6), 6), Ok(false));
        assert_eq!(run.tick(t + Duration::from_secs(8), 6), Ok(true));
        assert_eq!(run.tick(t + Duration::from_secs(9), 6), Ok(false));
        run.fail("disconnected");
        assert!(run.finished() && run.failed);
        assert_eq!(run.tick(t + Duration::from_secs(10), 6), Ok(false));
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(run.path).unwrap()).unwrap();
        assert_eq!(status["reason"], "disconnected");
        let mut timeout = AutoFps::new(&dir);
        timeout.joined(t);
        assert_eq!(
            timeout.tick(t + STABLE_TIMEOUT, 5),
            Err("stability_timeout")
        );
        let mut success = AutoFps::new(&dir);
        success.start().unwrap();
        assert!(success.succeed(&dir).is_err());
        success.joined(t);
        assert_eq!(success.tick(t + Duration::from_secs(1), 5), Ok(false));
        assert_eq!(success.tick(t + Duration::from_secs(5), 5), Ok(true));
        assert!(success.succeed(&dir).is_err()); // No result file yet.
        std::fs::write(dir.join("benchmark.json"), "{}").unwrap();
        success.succeed(&dir).unwrap();
        assert!(success.succeed(&dir).is_err());
        let status: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&success.path).unwrap()).unwrap();
        assert_eq!(status["state"], "success");
        assert!(status["benchmark_modified_unix_ms"].as_u64().is_some());
        assert_ne!(status["run_id"], "");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
