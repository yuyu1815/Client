//! Explicit, unattended launcher run. Never serialize or print account
//! credentials.
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde::Deserialize;
use tauri::AppHandle;

use crate::{auth, commands, installations};

const STATUS: &str = "auto-fps-benchmark-status.json";
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
    select_unique(
        accounts,
        |a| !a.uuid.is_empty() && id.is_none_or(|id| a.uuid == id),
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
        Some("benchmark_save_failed") => "benchmark_save_failed",
        _ => "client_failed",
    }
}

fn inspect_status(
    dir: &Path,
    expected_run: &mut Option<String>,
    saw_running: &mut bool,
) -> Result<Option<PathBuf>, &'static str> {
    let bytes = match std::fs::read(dir.join(STATUS)) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("status_unreadable"),
    };
    let status: Status = serde_json::from_slice(&bytes).map_err(|_| "status_invalid")?;
    if status.run_id.len() != 32 || !status.run_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("status_invalid");
    }
    if let Some(run) = expected_run {
        if run != &status.run_id {
            return Err("status_run_changed");
        }
    } else if status.state == "running" || status.state == "failed" {
        *expected_run = Some(status.run_id.clone());
    } else {
        // A previous success must never count as this run, even after a rapid exit.
        return Err("status_missing_start");
    }
    if status.benchmark_file != format!("benchmark-{}.json", status.run_id) {
        return Err("status_invalid");
    }
    match status.state.as_str() {
        "running" => {
            *saw_running = true;
            Ok(None)
        }
        "failed" => Err(reason(status.reason.as_deref())),
        "success" if *saw_running => {
            let path = dir.join(&status.benchmark_file);
            let modified = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .and_then(|t| t.duration_since(UNIX_EPOCH).map_err(std::io::Error::other))
                .map_err(|_| "result_missing")?
                .as_millis();
            if Some(modified as u64) != status.benchmark_modified_unix_ms {
                return Err("result_mismatch");
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
    let restored = auth::try_restore_or_refresh(&account.uuid)
        .await
        .filter(|a| a.uuid == account.uuid && !a.access_token.is_empty())
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
    match std::fs::remove_file(dir.join(STATUS)) {
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
        true,
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
    let mut run_id = None;
    let mut saw_running = false;
    let mut result = None;
    loop {
        if Instant::now() >= deadline {
            let _ = child.kill().await;
            return Err("benchmark_timeout");
        }
        match inspect_status(dir, &mut run_id, &mut saw_running) {
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
                let path = inspect_status(dir, &mut run_id, &mut saw_running)?.or(result);
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
        let accounts = [account("id-1"), account("id-2")];
        assert!(choose_account(&accounts, None).is_err());
        assert!(choose_account(&accounts, Some("missing")).is_err());
        assert_eq!(
            choose_account(&accounts, Some("id-2")).unwrap().uuid,
            "id-2"
        );
        assert!(choose_account(&[account("id-1"), account("id-1")], Some("id-1")).is_err());
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
            std::fs::write(dir.join(STATUS), serde_json::json!({
            "run_id": id, "state": state, "reason": "join_timeout",
            "benchmark_file": format!("benchmark-{id}.json"), "benchmark_modified_unix_ms": null,
        }).to_string()).unwrap()
        };
        let (mut run, mut running) = (None, false);
        write("success", &id);
        assert_eq!(
            inspect_status(&dir, &mut run, &mut running).unwrap_err(),
            "status_missing_start"
        );
        write("running", &id);
        assert!(
            inspect_status(&dir, &mut run, &mut running)
                .unwrap()
                .is_none()
        );
        write("success", &"b".repeat(32));
        assert_eq!(
            inspect_status(&dir, &mut run, &mut running).unwrap_err(),
            "status_run_changed"
        );
        write("failed", &id);
        assert_eq!(
            inspect_status(&dir, &mut run, &mut running).unwrap_err(),
            "join_timeout"
        );
        assert_eq!(reason(Some("private server / account")), "client_failed");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
