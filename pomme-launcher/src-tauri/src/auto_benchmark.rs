//! Explicit, unattended launcher run. Never serialize or print account
//! credentials.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Deserialize;
use tauri::AppHandle;

use crate::{auth, commands, installations};

fn status_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("auto-fps-benchmark-status-{id}.json"))
}
const TIMEOUT: Duration = Duration::from_secs(240);

pub fn select_unique<'a, T>(
    items: &'a [T],
    matches: impl Fn(&T) -> bool,
    error: &'static str,
) -> Result<&'a T, &'static str> {
    let mut found = items.iter().filter(|i| matches(i));
    let one = found.next().ok_or(error)?;
    if found.next().is_some() {
        return Err(error);
    }
    Ok(one)
}

#[derive(Deserialize)]
struct Status {
    run_id: String,
    state: String,
    reason: Option<String>,
    benchmark_file: String,
    benchmark_modified_unix_ms: Option<u64>,
    scene_origin: Option<SceneOrigin>,
}
#[derive(Deserialize)]
struct SceneOrigin {
    dimension: String,
    position: [f64; 3],
    yaw: f32,
    pitch: f32,
}
#[derive(Deserialize)]
struct ResultMetadata {
    auto_fps_run_id: String,
    profile: String,
    version: String,
    os: String,
    arch: String,
    gpu: String,
    resolution: [u32; 2],
    timestamp: String,
    total_frames: u32,
    duration_secs: f32,
    avg_fps: f32,
    avg_frame_ms: f32,
    peak_chunk_count: u32,
    spikes: Vec<serde_json::Value>,
}

// Only return known client reasons, never untrusted status content.
fn choose_install(
    installs: &[installations::Installation],
) -> Result<&installations::Installation, &'static str> {
    let install = select_unique(
        installs,
        |i| {
            !i.is_latest
                && i.directory
                    .as_ref()
                    .file_name()
                    .is_some_and(|n| n == "my-installation")
        },
        "installation_not_unique",
    )?;
    if installs.iter().filter(|i| i.id == install.id).count() != 1 {
        return Err("installation_not_unique");
    }
    Ok(install)
}

fn choose_account<'a>(
    accounts: &'a [auth::AuthAccount],
    id: Option<&str>,
) -> Result<&'a auth::AuthAccount, &'static str> {
    let requested = id
        .map(|s| uuid::Uuid::parse_str(s).map_err(|_| "account_invalid"))
        .transpose()?;
    select_unique(
        accounts,
        |a| {
            uuid::Uuid::parse_str(&a.uuid)
                .ok()
                .is_some_and(|parsed| requested.is_none_or(|id| id == parsed))
        },
        "account_not_unique",
    )
}

fn reason(raw: Option<&str>) -> &'static str {
    match raw {
        Some("join_timeout") => "join_timeout",
        Some("stability_timeout") => "stability_timeout",
        Some("focus_lost") => "focus_lost",
        Some("disconnected") => "disconnected",
        Some("startup_failed") => "startup_failed",
        Some("event_loop_error") => "event_loop_error",
        Some("exited_before_completion") => "exited_before_completion",
        Some("window_occluded") => "window_occluded",
        Some("window_not_visible") => "window_not_visible",
        Some("connection_cancelled") => "connection_cancelled",
        Some("connection_failed") => "connection_failed",
        Some("server_transfer") => "server_transfer",
        Some("another_benchmark_running") => "another_benchmark_running",
        Some("status_save_failed") => "status_save_failed",
        Some("scene_invalid") => "scene_invalid",
        Some("scene_changed") => "scene_changed",
        Some("player_dead") => "player_dead",
        Some("benchmark_save_failed") => "benchmark_save_failed",
        _ => "client_failed",
    }
}

fn inspect_status(dir: &Path, expected_run: &str) -> Result<Option<PathBuf>, &'static str> {
    let bytes = match std::fs::read(status_path(dir, expected_run)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("status_unreadable"),
    };
    let status: Status = serde_json::from_slice(&bytes).map_err(|_| "status_invalid")?;
    if status.run_id != expected_run {
        return Err("status_run_changed");
    }
    if status.benchmark_file != format!("benchmark-{expected_run}.json") {
        return Err("status_invalid");
    }
    match status.state.as_str() {
        "running" => Ok(None),
        "failed" => Err(reason(status.reason.as_deref())),
        "success" => {
            let origin = status.scene_origin.ok_or("result_invalid")?;
            if origin.dimension.is_empty()
                || origin.position.iter().any(|p| !p.is_finite())
                || !origin.yaw.is_finite()
                || !origin.pitch.is_finite()
            {
                return Err("result_invalid");
            }
            let path = dir.join(&status.benchmark_file);
            let modified = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
                .map_err(|_| "result_missing")?
                .as_millis();
            if Some(modified as u64) != status.benchmark_modified_unix_ms {
                return Err("result_mismatch");
            }
            let bytes = std::fs::read(&path).map_err(|_| "result_missing")?;
            let result: ResultMetadata =
                serde_json::from_slice(&bytes).map_err(|_| "result_invalid")?;
            if result.auto_fps_run_id != expected_run
                || !matches!(result.profile.as_str(), "debug" | "release")
                || result.version.is_empty()
                || result.os.is_empty()
                || result.arch.is_empty()
                || result.gpu.is_empty()
                || result.timestamp.is_empty()
                || result.resolution.contains(&0)
                || result.total_frames == 0
                || result.peak_chunk_count == 0
                || !result.duration_secs.is_finite()
                || result.duration_secs < 1.0
                || !result.avg_fps.is_finite()
                || result.avg_fps <= 0.0
                || !result.avg_frame_ms.is_finite()
                || result.avg_frame_ms <= 0.0
                || result.spikes.len() > result.total_frames as usize
            {
                return Err("result_invalid");
            }
            Ok(Some(path))
        }
        _ => Err("status_invalid"),
    }
}

pub async fn run(
    app: AppHandle,
    server: &str,
    account_id: Option<&str>,
) -> Result<PathBuf, &'static str> {
    if server.is_empty() || server.starts_with('-') || server.chars().any(char::is_whitespace) {
        return Err("invalid_server");
    }
    let installs = installations::registry::load().map_err(|_| "installations_unavailable")?;
    let install = choose_install(&installs)?;
    let dir: &Path = install.directory.as_ref();
    if !dir.is_dir() {
        return Err("installation_directory_missing");
    }
    let accounts = auth::get_all_accounts();
    let account = choose_account(&accounts, account_id)?;
    let selected_uuid = uuid::Uuid::parse_str(&account.uuid).map_err(|_| "account_invalid")?;
    let restored = auth::try_restore_or_refresh(&account.uuid)
        .await
        .filter(|a| {
            uuid::Uuid::parse_str(&a.uuid).ok() == Some(selected_uuid) && !a.access_token.is_empty()
        })
        .ok_or("account_unavailable")?;

    let version: String = install.version.clone().into();
    commands::ensure_assets(app.clone(), version)
        .await
        .map_err(|_| "assets_unavailable")?;

    // Exclusive per-installation guard; a crash requires manual lock removal (fail
    // closed).
    let lock_path = dir.join("auto-fps-launcher.lock");
    let lock = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|_| "benchmark_already_running")?;
    drop(lock); // Windows cannot unlink a still-open lock file.
    struct Unlock(PathBuf);
    impl Drop for Unlock {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _unlock = Unlock(lock_path);
    let run_id = format!("{:032x}", rand::random::<u128>());
    match std::fs::remove_file(status_path(dir, &run_id)) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err("status_unavailable"),
    }

    let mut child = commands::spawn_game(
        &app,
        install.clone(),
        Some(&restored),
        Some(server),
        None,
        false,
        Some(&run_id),
    )
    .await
    .map_err(|_| "launch_failed")?;
    // Drain without echoing any client log lines (which may contain identifiers).
    for stream in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>),
    ] {
        if let Some(mut stream) = stream {
            tokio::spawn(async move {
                let _ = tokio::io::copy(&mut stream, &mut tokio::io::sink()).await;
            });
        }
    }
    let deadline = Instant::now() + TIMEOUT;
    let mut result = None;
    loop {
        if Instant::now() >= deadline {
            let _ = child.kill().await;
            return Err("benchmark_timeout");
        }
        match inspect_status(dir, &run_id) {
            Ok(Some(path)) => result = Some(path),
            Ok(None) => (),
            Err(e) => {
                let _ = child.kill().await;
                return Err(e);
            }
        }
        match child.try_wait() {
            Ok(Some(exit)) => {
                // One last read, for the status written immediately before exit.
                let path = inspect_status(dir, &run_id)?.or(result);
                return if exit.success() {
                    path.ok_or("benchmark_incomplete")
                } else {
                    Err("client_exited")
                };
            }
            Ok(None) => (),
            Err(_) => {
                let _ = child.kill().await;
                return Err("client_wait_failed");
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_fails_closed() {
        let install = |directory: &str| {
            installations::Installation::try_from(installations::InstallationDraft {
                name: "Test".into(),
                version: "26.2".into(),
                directory: directory.into(),
                width: 854,
                height: 480,
            })
            .unwrap()
        };
        let target = install("my-installation");
        let other = install("other");
        assert!(choose_install(&[]).is_err());
        assert!(choose_install(&[target.clone(), target.clone()]).is_err());
        assert!(choose_install(&[other.clone()]).is_err());
        assert_eq!(
            choose_install(&[other, target.clone()]).unwrap().id,
            target.id
        );
        let account = |uuid: &str| auth::AuthAccount {
            uuid: uuid.into(),
            username: "test".into(),
            access_token: String::new(),
            expires_at: 0,
        };
        let id1 = "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa";
        let id2 = "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb";
        let accounts = [account(id1), account(id2)];
        assert!(choose_account(&accounts, None).is_err());
        assert!(choose_account(&accounts, Some("invalid")).is_err());
        assert!(choose_account(&[account("bad")], None).is_err());
        assert_eq!(
            choose_account(&accounts, Some(&id2.replace('-', "")))
                .unwrap()
                .uuid,
            id2
        );
        assert!(choose_account(&[account(id1), account(id1)], Some(id1)).is_err());
    }

    #[test]
    fn result_requires_matching_run_and_schema() {
        let dir = std::env::temp_dir().join(format!("pomme-result-{}", rand::random::<u64>()));
        std::fs::create_dir(&dir).unwrap();
        let id = "c".repeat(32);
        let path = dir.join(format!("benchmark-{id}.json"));
        std::fs::write(&path, "{}").unwrap();
        let update_status = || {
            let modified = std::fs::metadata(&path)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis();
            std::fs::write(
            status_path(&dir, &id),
            serde_json::json!({
                "run_id": id, "state": "success", "benchmark_file": format!("benchmark-{id}.json"),
                "benchmark_modified_unix_ms": modified, "scene_origin": {
                    "dimension": "minecraft:overworld", "position": [0.0, 64.0, 0.0],
                    "yaw": 0.0, "pitch": 0.0
                }
            })
            .to_string(),
        ).unwrap();
        };
        update_status();
        assert_eq!(inspect_status(&dir, &id).unwrap_err(), "result_invalid");
        let result = |run: &str| {
            serde_json::json!({
                "auto_fps_run_id": run, "profile": "release", "version": "0.1.0",
                "os": "windows", "arch": "x86_64", "gpu": "test", "resolution": [854,480],
                "timestamp": "2026-01-01T00:00:00Z", "total_frames": 300, "duration_secs": 10.0,
                "avg_fps": 30.0, "avg_frame_ms": 33.3, "peak_chunk_count": 9, "spikes": []
            })
        };
        std::fs::write(&path, result(&"d".repeat(32)).to_string()).unwrap();
        update_status();
        assert_eq!(inspect_status(&dir, &id).unwrap_err(), "result_invalid");
        std::fs::write(&path, result(&id).to_string()).unwrap();
        update_status();
        assert_eq!(inspect_status(&dir, &id).unwrap().unwrap(), path);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stale_status_and_one_shot_failure() {
        let dir = std::env::temp_dir().join(format!(
            "pomme-auto-run-test-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&dir).unwrap();
        let id = "a".repeat(32);
        let write = |state: &str, id: &str| {
            std::fs::write(status_path(&dir, &id), serde_json::json!({
            "run_id": id, "state": state, "reason": "join_timeout",
            "benchmark_file": format!("benchmark-{id}.json"), "benchmark_modified_unix_ms": null,
        }).to_string()).unwrap()
        };
        write("success", &id);
        assert_eq!(inspect_status(&dir, &id).unwrap_err(), "result_invalid");
        write("running", &id);
        assert!(inspect_status(&dir, &id).unwrap().is_none());
        write("failed", &"b".repeat(32)); // Another run's own file cannot affect ours.
        assert!(inspect_status(&dir, &id).unwrap().is_none());
        std::fs::write(status_path(&dir, &id), serde_json::json!({
            "run_id": "b".repeat(32), "state": "running", "benchmark_file": format!("benchmark-{id}.json")
        }).to_string()).unwrap();
        assert_eq!(inspect_status(&dir, &id).unwrap_err(), "status_run_changed");
        write("failed", &id);
        assert_eq!(inspect_status(&dir, &id).unwrap_err(), "join_timeout");
        assert_eq!(reason(Some("private server / account")), "client_failed");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
