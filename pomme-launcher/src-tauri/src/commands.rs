use std::collections::VecDeque;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::Stdio;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_specta::Event;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;

use crate::installations::{Installation, InstallationDraft, InstallationError};
use crate::settings::LauncherSettings;
use crate::{AppState, installations, storage};

#[derive(Deserialize)]
struct MojangPatchNotes {
    entries: Vec<MojangEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MojangEntry {
    title: String,
    version: String,
    date: String,
    short_text: String,
    image: MojangImage,
    #[serde(rename = "type")]
    entry_type: String,
    #[serde(rename = "contentPath")]
    content_path: String,
}

#[derive(Deserialize)]
struct MojangImage {
    url: String,
}

#[derive(Deserialize)]
struct MojangContent {
    body: String,
}

#[derive(Serialize, specta::Type)]
pub struct PatchNote {
    pub title: String,
    pub version: String,
    pub date: String,
    pub summary: String,
    pub image_url: String,
    pub entry_type: String,
    pub content_path: String,
}

const PATCH_NOTES_URL: &str = "https://launchercontent.mojang.com/v2/javaPatchNotes.json";
const IMAGE_BASE: &str = "https://launchercontent.mojang.com";

#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ConsoleMessageEvent {
    Message { val: String },
    Reset,
}

#[tauri::command]
#[specta::specta]
pub async fn get_patch_notes(count: Option<usize>) -> Result<Vec<PatchNote>, String> {
    let limit = count.unwrap_or(20);
    let resp: MojangPatchNotes = reqwest::get(PATCH_NOTES_URL)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let mut entries = resp.entries;
    entries.sort_unstable_by(|a, b| b.date.cmp(&a.date));

    Ok(entries
        .into_iter()
        .take(limit)
        .map(|e| PatchNote {
            title: e.title,
            date: e.date.chars().take(10).collect(),
            summary: e.short_text,
            image_url: format!("{IMAGE_BASE}{}", e.image.url),
            entry_type: e.entry_type,
            content_path: e.content_path,
            version: e.version,
        })
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn get_patch_content(content_path: String) -> Result<String, String> {
    let url = format!("{IMAGE_BASE}/v2/{content_path}");
    let content: MojangContent = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    Ok(content.body)
}

#[derive(Deserialize)]
struct SessionProfile {
    properties: Vec<SessionProperty>,
}

#[derive(Deserialize)]
struct SessionProperty {
    value: String,
}

#[derive(Deserialize)]
struct TexturesPayload {
    textures: Textures,
}

#[derive(Deserialize)]
struct Textures {
    #[serde(rename = "SKIN")]
    skin: Option<SkinTexture>,
}

#[derive(Deserialize)]
struct SkinTexture {
    url: String,
}

#[tauri::command]
#[specta::specta]
pub async fn get_skin_url(uuid: String) -> Result<String, String> {
    let clean_uuid = uuid.replace('-', "");
    let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{clean_uuid}");
    let profile: SessionProfile = reqwest::get(&url)
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let value = &profile.properties.first().ok_or("No properties")?.value;

    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|e| e.to_string())?;
    let payload: TexturesPayload = serde_json::from_slice(&decoded).map_err(|e| e.to_string())?;

    payload
        .textures
        .skin
        .map(|s| s.url)
        .ok_or_else(|| "No skin texture".to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_all_accounts() -> Vec<crate::auth::AuthAccount> {
    crate::auth::get_all_accounts()
}

#[tauri::command]
#[specta::specta]
pub async fn add_account(app: AppHandle) -> Result<crate::auth::AuthAccount, String> {
    crate::auth::oauth_sign_in(app).await
}

#[tauri::command]
#[specta::specta]
pub fn remove_account(uuid: String) {
    crate::auth::remove_account(&uuid);
}

#[derive(Deserialize)]
struct VersionManifest {
    latest: LatestVersions,
    versions: Vec<VersionEntry>,
}

#[derive(Deserialize)]
struct VersionEntry {
    id: String,
    #[serde(rename = "type")]
    version_type: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Serialize)]
pub struct Versions {
    pub latest: LatestVersions,
    pub versions: Vec<GameVersion>,
}

#[derive(Serialize, Clone, specta::Type)]
pub struct GameVersion {
    pub id: String,
    pub version_type: String,
}

#[derive(Clone, Serialize, specta::Type, tauri_specta::Event)]
pub struct GameExitedEvent {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub last_lines: Option<Vec<String>>,
}

static VERSION_CACHE: std::sync::OnceLock<Versions> = std::sync::OnceLock::new();

pub async fn fetch_versions() -> Result<&'static Versions, String> {
    if let Some(cached) = VERSION_CACHE.get() {
        return Ok(cached);
    }

    let manifest: VersionManifest =
        reqwest::get("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;

    let versions: Vec<GameVersion> = manifest
        .versions
        .into_iter()
        .map(|v| GameVersion {
            id: v.id,
            version_type: v.version_type,
        })
        .collect();

    let latest: LatestVersions = manifest.latest;

    Ok(VERSION_CACHE.get_or_init(|| Versions { latest, versions }))
}

#[tauri::command]
#[specta::specta]
pub async fn get_versions(show_snapshots: Option<bool>) -> Result<Vec<GameVersion>, String> {
    let all = fetch_versions().await?;
    let include_snapshots = show_snapshots.unwrap_or(false);
    Ok(all
        .versions
        .iter()
        .filter(|v| include_snapshots || v.version_type == "release")
        .cloned()
        .collect())
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_account(uuid: String) -> Result<crate::auth::AuthAccount, String> {
    crate::auth::try_restore_or_refresh(&uuid)
        .await
        .ok_or_else(|| "Failed to refresh account".to_string())
}

static TOKEN_REFRESH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn fresh_token(uuid: &str) -> Result<String, String> {
    let _guard = TOKEN_REFRESH_LOCK.lock().await;
    crate::auth::try_restore_or_refresh(uuid)
        .await
        .map(|a| a.access_token)
        .ok_or_else(|| "Account is not signed in".to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_friends(
    uuid: String,
) -> Result<crate::friends::FriendsList, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::get_friends(&token).await
}

#[tauri::command]
#[specta::specta]
pub async fn send_friend_request(
    uuid: String,
    name: String,
) -> Result<crate::friends::FriendsList, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::action_by_name(&token, &name, crate::friends::UpdateType::Add).await
}

#[tauri::command]
#[specta::specta]
pub async fn accept_friend_request(
    uuid: String,
    friend_uuid: String,
) -> Result<crate::friends::FriendsList, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::action_by_id(&token, &friend_uuid, crate::friends::UpdateType::Add).await
}

#[tauri::command]
#[specta::specta]
pub async fn remove_friend(
    uuid: String,
    friend_uuid: String,
) -> Result<crate::friends::FriendsList, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::action_by_id(&token, &friend_uuid, crate::friends::UpdateType::Remove).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_presence(
    uuid: String,
    status: String,
    join_info: Option<crate::friends::PresenceJoinInfo>,
) -> Result<Vec<crate::friends::PresenceEntry>, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::update_presence(&token, &status, join_info.as_ref()).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_friend_settings(
    uuid: String,
) -> Result<crate::friends::FriendSettings, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::get_friend_settings(&token).await
}

#[tauri::command]
#[specta::specta]
pub async fn update_friend_settings(
    uuid: String,
    show_in_list: bool,
    accept_invites: bool,
) -> Result<crate::friends::FriendSettings, crate::friends::FriendsApiError> {
    let token = fresh_token(&uuid).await?;
    crate::friends::update_friend_settings(&token, show_in_list, accept_invites).await
}

#[tauri::command]
#[specta::specta]
pub async fn ensure_assets(app: AppHandle, version: String) -> Result<(), String> {
    if crate::downloader::needs_download(&version) {
        crate::downloader::download(&app, &version).await?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_downloaded_versions() -> Vec<String> {
    crate::downloader::get_downloaded_versions().await
}

#[tauri::command]
#[specta::specta]
pub async fn launch_game(
    app: AppHandle,
    install_id: String,
    uuid: Option<String>,
    server_ip: Option<String>,
    override_version: Option<String>,
    debug_enabled: Option<bool>,
) -> Result<String, String> {
    let account = uuid.as_deref().and_then(crate::auth::try_restore);
    let install = installations::registry::find_by_id(&installations::Id::from(install_id))
        .map_err(|e| e.to_string())?;
    let mut child = spawn_game(
        &app,
        install,
        account.as_ref(),
        server_ip.as_deref(),
        override_version,
        debug_enabled.unwrap_or(false),
        false,
    )
    .await?;
    let username = account
        .as_ref()
        .map(|a| a.username.as_str())
        .unwrap_or("Steve");

    let stdout = child.stdout.take().expect("couldn't take stdout");
    let stderr = child.stderr.take().expect("couldn't take stderr");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(bool, String)>();
    let tx2 = tx.clone();

    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = tx.send((false, line));
        }
    });

    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = tx2.send((true, line));
        }
    });

    let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Option<Vec<String>>>();
    let app_handle = app.clone();

    tokio::spawn(async move {
        const MAX_LINES: usize = 50;

        let mut stderr_lines: VecDeque<String> = VecDeque::new();
        let mut tracing_errors: VecDeque<String> = VecDeque::new();

        while let Some((is_stderr, line)) = rx.recv().await {
            let _ = ConsoleMessageEvent::Message { val: line.clone() }.emit(&app_handle);

            let state = app_handle.state::<AppState>();
            let mut logs = state.client_logs.lock().await;
            logs.push_back(line.clone());
            if logs.len() > 10_000 {
                logs.pop_front();
            }

            let target = if is_stderr {
                Some(&mut stderr_lines)
            } else if line.contains(" ERROR ") {
                Some(&mut tracing_errors)
            } else {
                None
            };
            if let Some(buf) = target {
                if buf.len() >= MAX_LINES {
                    buf.pop_front();
                }
                buf.push_back(line.clone());
            }
        }

        let mut combined: Vec<String> = stderr_lines.into();
        combined.extend(tracing_errors);

        let result = if combined.is_empty() {
            None
        } else {
            Some(combined)
        };
        let _ = result_tx.send(result);
    });

    tokio::spawn(async move {
        let status = child
            .wait()
            .await
            .expect("client process encountered an error");

        let last = result_rx.await.unwrap_or(None);

        #[cfg(unix)]
        let signal = status.signal();
        #[cfg(not(unix))]
        let signal: Option<i32> = None;

        let _ = GameExitedEvent {
            code: status.code(),
            signal,
            last_lines: last,
        }
        .emit(&app);
    });

    Ok(format!("Launched as {username}"))
}

// Shared launch path: the opt-in runner must not reimplement token handling or
// client arguments.
pub(crate) async fn spawn_game(
    app: &AppHandle,
    install: Installation,
    account: Option<&crate::auth::AuthAccount>,
    server_ip: Option<&str>,
    override_version: Option<String>,
    debug_enabled: bool,
    auto_fps: bool,
) -> Result<tokio::process::Child, String> {
    // Prefer a local dev build (target/); otherwise download the latest release.
    let exe = match find_client_binary() {
        Ok(local) => local,
        Err(_) => crate::client_updater::ensure_client(app).await?,
    };
    let username = account.map(|a| a.username.as_str()).unwrap_or("Steve");

    let token: String = (0..32)
        .map(|_| format!("{:02x}", rand::random::<u8>()))
        .collect();
    let token_path = std::env::temp_dir().join("pomme_launch_token");
    std::fs::write(&token_path, &token).map_err(|e| e.to_string())?;

    let version = override_version.unwrap_or_else(|| install.version.into());
    let install_path: String = install.directory.into();

    let mut cmd = tokio::process::Command::new(&exe);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if auto_fps {
        cmd.kill_on_drop(true);
    }

    // macOS releases bundle MoltenVK next to the client; point the Vulkan loader
    // at the bundled ICD. Absent for local dev builds, which use system Vulkan.
    #[cfg(target_os = "macos")]
    if let Some(icd) = exe
        .parent()
        .map(|d| d.join("MoltenVK_icd.json"))
        .filter(|p| p.exists())
    {
        cmd.env("VK_ICD_FILENAMES", &icd);
    }

    if debug_enabled {
        cmd.env("RUST_LOG", "debug");
        cmd.env("RUST_BACKTRACE", "full");

        match app.webview_windows().get("console") {
            None => {
                WebviewWindowBuilder::new(app, "console", WebviewUrl::App("console".into()))
                    .title("Pomme Debugger")
                    .decorations(false)
                    .build()
                    .unwrap();
            }
            Some(window) => {
                let _ = ConsoleMessageEvent::Reset.emit(app);
                let _ = window.set_focus();
            }
        }
    }

    cmd.arg("--version")
        .arg(&version)
        .arg("--username")
        .arg(&username)
        .arg("--assets-dir")
        .arg(storage::assets_dir().to_string_lossy().as_ref())
        .arg("--versions-dir")
        .arg(storage::versions_dir().to_string_lossy().as_ref())
        .arg("--launch-token")
        .arg(token_path.to_string_lossy().as_ref())
        .arg("--game-dir")
        .arg(install_path);

    if let Some(acc) = &account {
        cmd.arg("--uuid")
            .arg(&acc.uuid)
            .arg("--access-token")
            .arg(&acc.access_token);
    }
    if let Some(server_ip) = server_ip {
        cmd.arg("--quick-access-multiplayer").arg(server_ip);
    }
    if auto_fps {
        cmd.arg("--auto-fps-benchmark");
    }

    #[cfg(unix)]
    cmd.process_group(0);

    // The client is a console-subsystem binary; without this, Windows pops a
    // terminal window for it alongside the game.
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    cmd.spawn().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_client_logs(state: State<'_, AppState>) -> Result<VecDeque<String>, ()> {
    let logs = state.client_logs.lock().await;
    Ok(logs.clone())
}

fn client_binary_candidates(
    dir: &std::path::Path,
    exe_name: &str,
    debug: bool,
) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if debug {
        // A launcher in target/debug may sit beside a stale client build.
        candidates.extend(
            dir.ancestors()
                .skip(1)
                .take(6)
                .map(|ancestor| ancestor.join("target/dev-fast").join(exe_name)),
        );
    }
    candidates.push(dir.join(exe_name));
    let mut ancestor = dir.to_path_buf();
    for _ in 0..6 {
        if !ancestor.pop() {
            break;
        }

        let profiles: &[&str] = if debug {
            &["debug", "release"]
        } else {
            &["release", "debug"]
        };
        for profile in profiles {
            candidates.push(ancestor.join("target").join(profile).join(exe_name));
        }
    }
    candidates
}

fn find_client_binary() -> Result<std::path::PathBuf, String> {
    #[cfg(target_family = "windows")]
    const EXENAME: &str = "pomme-client.exe";

    #[cfg(target_family = "unix")]
    const EXENAME: &str = "pomme-client";

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        #[cfg(debug_assertions)]
        let debug = true;
        #[cfg(not(debug_assertions))]
        let debug = false;

        if let Some(candidate) = client_binary_candidates(dir, EXENAME, debug)
            .into_iter()
            .find(|candidate| candidate.exists())
        {
            return Ok(candidate);
        }
    }

    Err("Pomme client not found. It will be bundled in future releases.".into())
}

#[cfg(test)]
mod client_binary_search_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::client_binary_candidates;

    fn fixture() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pomme-binary-search-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn debug_prefers_dev_fast_over_stale_beside_launcher() {
        let root = fixture();
        let launcher_dir = root.join("target/debug");
        let beside = launcher_dir.join("pomme-client.exe");
        let dev_fast = root.join("target/dev-fast/pomme-client.exe");
        fs::create_dir_all(&launcher_dir).unwrap();
        fs::create_dir_all(dev_fast.parent().unwrap()).unwrap();
        fs::write(&beside, b"old debug client").unwrap();
        fs::write(&dev_fast, b"new dev-fast client").unwrap();

        let candidates = client_binary_candidates(&launcher_dir, "pomme-client.exe", true);
        assert_eq!(candidates.iter().find(|p| p.exists()), Some(&dev_fast));
        let release = client_binary_candidates(&launcher_dir, "pomme-client.exe", false);
        assert_eq!(release.iter().find(|p| p.exists()), Some(&beside));
        assert!(!release.contains(&dev_fast));

        fs::remove_file(&dev_fast).unwrap();
        assert_eq!(candidates.iter().find(|p| p.exists()), Some(&beside));
        assert_eq!(fs::read(&beside).unwrap(), b"old debug client");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn debug_prefers_dev_fast_then_beside_then_debug() {
        let root = fixture();
        let launcher_dir = root
            .join("launcher")
            .join("src-tauri")
            .join("target")
            .join("debug");
        let dev_fast = root.join("target/dev-fast/pomme-client.exe");
        let debug = root.join("target/debug/pomme-client.exe");
        let release_client = root.join("target/release/pomme-client.exe");
        let beside = launcher_dir.join("pomme-client.exe");
        for path in [&dev_fast, &debug, &release_client, &beside] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, []).unwrap();
        }

        let candidates = client_binary_candidates(&launcher_dir, "pomme-client.exe", true);
        assert_eq!(
            candidates.iter().find(|p| p.exists()).cloned(),
            Some(dev_fast.clone())
        );
        let release = client_binary_candidates(&launcher_dir, "pomme-client.exe", false);
        assert_eq!(release.iter().find(|p| p.exists()), Some(&beside));
        assert!(!release.contains(&dev_fast));

        fs::remove_file(&dev_fast).unwrap();
        assert_eq!(candidates.iter().find(|p| p.exists()), Some(&beside));
        fs::remove_file(&beside).unwrap();
        assert_eq!(candidates.iter().find(|p| p.exists()), Some(&debug));
        assert_eq!(release.iter().find(|p| p.exists()), Some(&release_client));
        fs::remove_file(&debug).unwrap();
        assert_eq!(
            candidates.iter().find(|p| p.exists()),
            Some(&release_client)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[tauri::command]
#[specta::specta]
pub async fn load_launcher_settings() -> LauncherSettings {
    let settings = LauncherSettings::get().await;
    settings.clone()
}

#[tauri::command]
#[specta::specta]
pub async fn set_launcher_language(language: String) -> Result<(), String> {
    LauncherSettings::update(|s| s.language = language).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_keep_launcher_open(keep: bool) -> Result<(), String> {
    LauncherSettings::update(|s| s.keep_launcher_open = keep).await
}

#[tauri::command]
#[specta::specta]
pub async fn set_launch_with_console(launch: bool) -> Result<(), String> {
    LauncherSettings::update(|s| s.launch_with_console = launch).await
}

#[tauri::command]
#[specta::specta]
pub async fn ping_server(address: String) -> crate::ping::ServerStatus {
    crate::ping::ping_server(&address).await
}

#[tauri::command]
#[specta::specta]
pub async fn load_servers() -> Vec<crate::ping::SavedServer> {
    let path = servers_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
#[specta::specta]
pub async fn save_servers(servers: Vec<crate::ping::SavedServer>) -> Result<(), String> {
    let path = servers_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let json = serde_json::to_string_pretty(&servers).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

fn servers_path() -> std::path::PathBuf {
    storage::installations_dir()
        .join("default")
        .join("servers.json")
}

#[tauri::command]
#[specta::specta]
pub async fn load_installations(
    state: State<'_, AppState>,
) -> Result<Vec<Installation>, InstallationError> {
    let _guard = state.installations_lock.lock().await;
    installations::load_installations().await
}

#[tauri::command]
#[specta::specta]
pub async fn create_installation(
    state: State<'_, AppState>,
    payload: InstallationDraft,
) -> Result<Installation, InstallationError> {
    let _guard = state.installations_lock.lock().await;
    installations::create_installation(payload).await
}

#[tauri::command]
#[specta::specta]
pub async fn delete_installation(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), InstallationError> {
    let _guard = state.installations_lock.lock().await;
    installations::delete_installation(id).await
}

#[tauri::command]
#[specta::specta]
pub async fn duplicate_installation(
    state: State<'_, AppState>,
    old_id: String,
    payload: InstallationDraft,
) -> Result<Installation, InstallationError> {
    let _guard = state.installations_lock.lock().await;
    installations::duplicate_installation(old_id, payload).await
}

#[tauri::command]
#[specta::specta]
pub async fn edit_installation(
    state: State<'_, AppState>,
    id: String,
    payload: InstallationDraft,
) -> Result<Installation, InstallationError> {
    let _guard = state.installations_lock.lock().await;
    installations::edit_installation(id, payload).await
}
