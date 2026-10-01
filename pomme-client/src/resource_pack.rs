use std::path::{Path, PathBuf};

pub const CURRENT_PACK_FORMAT: u32 = 88;
const MAX_PACK_BYTES: u64 = 250 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ZIP_ENTRIES: usize = 100_000;
const CACHE_VALID_MARKER: &str = ".pomme-valid";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PackCompat {
    Compatible,
    TooOld,
    TooNew,
}

#[derive(Clone)]
pub struct PackInfo {
    pub name: String,
    pub description: String,
    pub compat: PackCompat,
    pub source: PackSource,
    pub enabled: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub enum PackSource {
    Server,
    Local,
}

struct ActivePack {
    id: String,
    source: PackSource,
    dir: PathBuf,
    info: PackInfo,
}

pub struct ResourcePackManager {
    packs_dir: PathBuf,
    server_cache_dir: PathBuf,
    active_packs: Vec<ActivePack>,
    available_local: Vec<PackInfo>,
}

impl ResourcePackManager {
    pub fn new(instance_dir: &Path) -> Self {
        let packs_dir = instance_dir.join("resourcepacks");
        let server_cache_dir = packs_dir.join(".server_cache");
        let _ = std::fs::create_dir_all(&packs_dir);
        let _ = std::fs::create_dir_all(&server_cache_dir);
        let mut mgr = Self {
            packs_dir,
            server_cache_dir,
            active_packs: Vec::new(),
            available_local: Vec::new(),
        };
        mgr.scan_local_packs();
        let selected = std::fs::read(instance_dir.join("options.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|settings| settings.get("resource_packs")?.as_array().cloned())
            .unwrap_or_default();
        for name in selected.iter().filter_map(serde_json::Value::as_str) {
            mgr.enable_local_pack(name);
        }
        mgr
    }

    pub fn resolve_asset(&self, asset_key: &str) -> Option<PathBuf> {
        if !crate::assets::valid_asset_key(asset_key) {
            return None;
        }
        for pack in self.active_packs.iter().rev() {
            let path = pack.dir.join("assets").join(asset_key);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    /// Active pack roots in low-to-high priority order, matching the order in
    /// which stacked resources are registered by Vanilla.
    pub fn active_pack_dirs(&self) -> impl Iterator<Item = &Path> {
        self.active_packs.iter().map(|pack| pack.dir.as_path())
    }

    /// Resolve a resource-pack asset together with the metadata sidecar that
    /// vanilla would expose for that resource. Metadata may come from the same
    /// pack or a higher-priority pack, but never from below the pack that
    /// supplied the resource itself.
    pub fn resolve_asset_with_metadata(
        &self,
        asset_key: &str,
    ) -> Option<(PathBuf, Option<PathBuf>)> {
        let metadata_key = format!("{asset_key}.mcmeta");
        for (source_index, pack) in self.active_packs.iter().enumerate().rev() {
            let path = pack.dir.join("assets").join(asset_key);
            if !path.exists() {
                continue;
            }
            let metadata = self.active_packs[source_index..]
                .iter()
                .rev()
                .map(|candidate| candidate.dir.join("assets").join(&metadata_key))
                .find(|candidate| candidate.exists());
            return Some((path, metadata));
        }
        None
    }

    pub fn download_server_pack(
        server_cache_dir: &Path,
        _id: uuid::Uuid,
        url: &str,
        hash: &str,
    ) -> Result<PathBuf, PackError> {
        use std::io::Read;
        use std::time::Duration;

        validate_hash_format(hash)?;
        let url = reqwest::Url::parse(url).map_err(|e| PackError::Download(e.to_string()))?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(PackError::Download(
                "resource pack URL must be HTTP(S)".into(),
            ));
        }
        // Only a supplied hash identifies a reusable download. UUIDs and URLs
        // can both be reused by the next server for different content.
        if !hash.is_empty() {
            let dir = server_cache_dir.join(hash.to_ascii_lowercase());
            if valid_server_cache(&dir, hash) {
                return Ok(dir);
            }
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| PackError::Download(e.to_string()))?;
        let response = client
            .get(url)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|e| PackError::Download(e.to_string()))?;
        if response
            .content_length()
            .is_some_and(|size| size > MAX_PACK_BYTES)
        {
            return Err(PackError::Download(
                "resource pack exceeds download limit".into(),
            ));
        }
        let mut data = Vec::new();
        response
            .take(MAX_PACK_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| PackError::Download(e.to_string()))?;
        if data.len() as u64 > MAX_PACK_BYTES {
            return Err(PackError::Download(
                "resource pack exceeds download limit".into(),
            ));
        }
        validate_hash(&data, hash)?;
        let content_hash = sha1_smol::Sha1::from(&data).digest().to_string();
        let dir = server_cache_dir.join(&content_hash);
        if valid_server_cache(&dir, &content_hash) {
            return Ok(dir);
        }
        std::fs::create_dir_all(server_cache_dir).map_err(|e| PackError::Extract(e.to_string()))?;
        let staging = server_cache_dir.join(format!(".staging-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&staging).map_err(|e| PackError::Extract(e.to_string()))?;
        scopeguard::defer! { let _ = std::fs::remove_dir_all(&staging); }
        extract_zip(&data, &staging)?;
        let meta_path = staging.join("pack.mcmeta");
        if std::fs::metadata(&meta_path)
            .map_err(|e| PackError::Extract(e.to_string()))?
            .len()
            > 1024 * 1024
        {
            return Err(PackError::Extract("pack metadata exceeds 1 MiB".into()));
        }
        let meta = std::fs::read(meta_path).map_err(|e| PackError::Extract(e.to_string()))?;
        let meta: serde_json::Value =
            serde_json::from_slice(&meta).map_err(|e| PackError::Extract(e.to_string()))?;
        if !meta.get("pack").is_some_and(serde_json::Value::is_object) {
            return Err(PackError::Extract("missing pack metadata".into()));
        }
        std::fs::write(staging.join(CACHE_VALID_MARKER), &content_hash)
            .map_err(|e| PackError::Extract(e.to_string()))?;
        // Never replace a directory: an old result or a failed legacy extract
        // might still be referenced by an active pack. Publish beside it.
        let destination = if dir.exists() {
            server_cache_dir.join(format!("{content_hash}-{}", uuid::Uuid::new_v4()))
        } else {
            dir.clone()
        };
        match std::fs::rename(&staging, &destination) {
            Ok(()) => Ok(destination),
            Err(_) if valid_server_cache(&dir, &content_hash) => Ok(dir),
            Err(e) => Err(PackError::Extract(e.to_string())),
        }
    }

    pub fn apply_server_pack(&mut self, id: uuid::Uuid, hash: &str, dir: PathBuf) {
        let pack_id = id.to_string();
        self.active_packs
            .retain(|p| !(p.id == pack_id && p.source == PackSource::Server));
        let info = parse_pack_meta_dir(&dir, hash);
        self.active_packs.push(ActivePack {
            id: pack_id,
            source: PackSource::Server,
            dir,
            info: PackInfo {
                enabled: true,
                source: PackSource::Server,
                ..info
            },
        });
        tracing::info!("Applied server resource pack {id} (hash: {hash})");
    }

    pub fn remove_server_pack(&mut self, id: &uuid::Uuid) -> bool {
        let id_str = id.to_string();
        let before = self.active_packs.len();
        self.active_packs
            .retain(|p| !(p.id == id_str && p.source == PackSource::Server));
        let removed = self.active_packs.len() < before;
        if removed {
            tracing::info!("Removed server resource pack {id}");
        }
        removed
    }

    pub fn clear_server_packs(&mut self) -> bool {
        let before = self.active_packs.len();
        self.active_packs.retain(|p| p.source != PackSource::Server);
        let removed = self.active_packs.len() != before;
        if removed {
            tracing::info!("Cleared all server resource packs");
        }
        removed
    }

    pub fn scan_local_packs(&mut self) {
        self.available_local = Self::scan_local_packs_at(&self.packs_dir);
        for info in &mut self.available_local {
            info.enabled = self
                .active_packs
                .iter()
                .any(|pack| pack.source == PackSource::Local && pack.id == info.name);
        }
    }

    /// Scans one local resource-pack directory without changing active packs.
    /// The options UI uses this on screen entry in both title and in-game
    /// menus.
    pub fn scan_local_packs_at(packs_dir: &Path) -> Vec<PackInfo> {
        let Ok(entries) = std::fs::read_dir(packs_dir) else {
            return Vec::new();
        };
        let mut packs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name == ".server_cache" || !valid_local_pack_name(&name) {
                continue;
            }
            if path.is_dir() && path.join("pack.mcmeta").exists() {
                let mut info = parse_pack_meta_dir(&path, &name);
                info.source = PackSource::Local;
                packs.push(info);
            } else if path.extension().is_some_and(|extension| extension == "zip")
                && let Some(mut info) = parse_pack_meta_zip(&path, &name)
            {
                info.source = PackSource::Local;
                packs.push(info);
            }
        }
        packs
    }

    pub fn enable_local_pack(&mut self, name: &str) {
        if !valid_local_pack_name(name) {
            return;
        }
        let path = self.packs_dir.join(name);
        if path.is_dir() && path.join("pack.mcmeta").exists() {
            self.active_packs
                .retain(|pack| !(pack.id == name && pack.source == PackSource::Local));
            let info = parse_pack_meta_dir(&path, name);
            self.active_packs.push(ActivePack {
                id: name.to_owned(),
                source: PackSource::Local,
                dir: path,
                info: PackInfo {
                    enabled: true,
                    source: PackSource::Local,
                    ..info
                },
            });
            tracing::info!("Enabled local resource pack: {name}");
        } else if path.extension().is_some_and(|e| e == "zip")
            && let Ok(data) = std::fs::read(&path)
        {
            let extract_dir = self.server_cache_dir.join(format!("_local_{name}"));
            if let Err(e) = extract_zip(&data, &extract_dir) {
                tracing::error!("Failed to extract zip pack {name}: {e}");
                return;
            }
            let info = parse_pack_meta_dir(&extract_dir, name);
            self.active_packs
                .retain(|pack| !(pack.id == name && pack.source == PackSource::Local));
            self.active_packs.push(ActivePack {
                id: name.to_owned(),
                source: PackSource::Local,
                dir: extract_dir,
                info: PackInfo {
                    enabled: true,
                    source: PackSource::Local,
                    ..info
                },
            });
            tracing::info!("Enabled local resource pack: {name}");
        }
        self.scan_local_packs();
    }

    pub fn disable_local_pack(&mut self, name: &str) {
        self.active_packs
            .retain(|p| !(p.id == name && p.source == PackSource::Local));
        tracing::info!("Disabled local resource pack: {name}");
        self.scan_local_packs();
    }

    pub fn active_pack_info(&self) -> Vec<PackInfo> {
        self.active_packs.iter().map(|p| p.info.clone()).collect()
    }

    pub fn available_local_packs(&self) -> &[PackInfo] {
        &self.available_local
    }

    pub fn server_cache_dir(&self) -> &Path {
        &self.server_cache_dir
    }
}

fn valid_local_pack_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

#[derive(Debug)]
pub enum PackError {
    Download(String),
    HashMismatch,
    InvalidHash,
    Extract(String),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Download(e) => write!(f, "download failed: {e}"),
            Self::HashMismatch => write!(f, "SHA-1 hash mismatch"),
            Self::InvalidHash => {
                write!(f, "invalid SHA-1 hash (expected 40 hexadecimal characters)")
            }
            Self::Extract(e) => write!(f, "extraction failed: {e}"),
        }
    }
}

fn valid_server_cache(dir: &Path, hash: &str) -> bool {
    std::fs::read_to_string(dir.join(CACHE_VALID_MARKER))
        .is_ok_and(|marker| marker.eq_ignore_ascii_case(hash))
        && dir.join("pack.mcmeta").is_file()
}

fn validate_hash_format(expected: &str) -> Result<(), PackError> {
    if expected.is_empty()
        || (expected.len() == 40 && expected.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        Ok(())
    } else {
        Err(PackError::InvalidHash)
    }
}

fn validate_hash(data: &[u8], expected: &str) -> Result<(), PackError> {
    validate_hash_format(expected)?;
    if expected.is_empty() {
        return Ok(());
    }
    let actual = sha1_smol::Sha1::from(data).digest().to_string();
    if !actual.eq_ignore_ascii_case(expected) {
        tracing::error!("Hash mismatch: expected {expected}, got {actual}");
        return Err(PackError::HashMismatch);
    }
    Ok(())
}

fn parse_meta_value(v: &serde_json::Value, fallback: &str) -> (String, String, PackCompat) {
    let pack = v.get("pack");

    let description = pack
        .and_then(|p| p.get("description"))
        .and_then(|d| d.as_str())
        .unwrap_or(fallback)
        .to_owned();

    let name = pack
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or(fallback)
        .to_owned();

    let compat = pack
        .map(|p| {
            let (min, max) = parse_format_range(p);
            if CURRENT_PACK_FORMAT < min {
                PackCompat::TooNew
            } else if CURRENT_PACK_FORMAT > max {
                PackCompat::TooOld
            } else {
                PackCompat::Compatible
            }
        })
        .unwrap_or(PackCompat::Compatible);

    (name, description, compat)
}

fn parse_pack_meta_dir(dir: &Path, fallback_name: &str) -> PackInfo {
    let meta_path = dir.join("pack.mcmeta");
    let Some(v) = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return PackInfo {
            name: fallback_name.to_owned(),
            description: fallback_name.to_owned(),
            compat: PackCompat::Compatible,
            source: PackSource::Local,
            enabled: false,
        };
    };

    let (name, description, compat) = parse_meta_value(&v, fallback_name);
    PackInfo {
        name,
        description,
        compat,
        source: PackSource::Local,
        enabled: false,
    }
}

fn parse_pack_meta_zip(path: &Path, fallback_name: &str) -> Option<PackInfo> {
    let data = std::fs::read(path).ok()?;
    let cursor = std::io::Cursor::new(data);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut mcmeta = archive.by_name("pack.mcmeta").ok()?;
    let mut contents = String::new();
    std::io::Read::read_to_string(&mut mcmeta, &mut contents).ok()?;
    let v: serde_json::Value = serde_json::from_str(&contents).ok()?;

    let (name, description, compat) = parse_meta_value(&v, fallback_name);
    Some(PackInfo {
        name,
        description,
        compat,
        source: PackSource::Local,
        enabled: false,
    })
}

fn parse_format_range(pack: &serde_json::Value) -> (u32, u32) {
    if let (Some(min), Some(max)) = (pack.get("min_format"), pack.get("max_format")) {
        let min_v = format_value(min);
        let max_v = format_value(max);
        if min_v > 0 && max_v > 0 {
            return (min_v, max_v);
        }
    }

    if let Some(supported) = pack.get("supported_formats") {
        if let Some(arr) = supported.as_array()
            && arr.len() == 2
        {
            return (
                arr[0].as_u64().unwrap_or(0) as u32,
                arr[1].as_u64().unwrap_or(0) as u32,
            );
        }
        if let Some(obj) = supported.as_object() {
            let min = obj
                .get("min_inclusive")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            let max = obj
                .get("max_inclusive")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            return (min, max);
        }
        if let Some(n) = supported.as_u64() {
            return (n as u32, n as u32);
        }
    }

    if let Some(fmt) = pack.get("pack_format").and_then(|v| v.as_u64()) {
        return (fmt as u32, fmt as u32);
    }

    (0, u32::MAX)
}

fn format_value(v: &serde_json::Value) -> u32 {
    if let Some(n) = v.as_u64() {
        return n as u32;
    }
    if let Some(arr) = v.as_array()
        && let Some(major) = arr.first().and_then(|v| v.as_u64())
    {
        return major as u32;
    }
    0
}

fn extract_zip(data: &[u8], dest: &Path) -> Result<(), PackError> {
    use std::io::Read;

    std::fs::create_dir_all(dest).map_err(|e| PackError::Extract(e.to_string()))?;
    let cursor = std::io::Cursor::new(data);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| PackError::Extract(e.to_string()))?;

    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(PackError::Extract("too many ZIP entries".into()));
    }
    let mut extracted = 0;
    for i in 0..archive.len() {
        let file = archive
            .by_index(i)
            .map_err(|e| PackError::Extract(e.to_string()))?;
        // Check both separators even on Unix; packs may later be used on Windows.
        if file.name().starts_with(['/', '\\'])
            || file.name().contains(':')
            || file.name().split(['/', '\\']).any(|part| {
                let base = part
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                (part != "." && part.ends_with([' ', '.']))
                    || matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                    || (base.len() == 4
                        && (base.starts_with("COM") || base.starts_with("LPT"))
                        && matches!(base.as_bytes()[3], b'1'..=b'9'))
            })
            || file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(PackError::Extract("unsafe ZIP entry".into()));
        }
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| PackError::Extract("unsafe ZIP path".into()))?;
        let out_path = dest.join(enclosed);
        if file.size() > MAX_EXTRACTED_BYTES - extracted {
            return Err(PackError::Extract(
                "resource pack exceeds extraction limit".into(),
            ));
        }
        if file.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| PackError::Extract(e.to_string()))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| PackError::Extract(e.to_string()))?;
            }
            let mut out =
                std::fs::File::create(&out_path).map_err(|e| PackError::Extract(e.to_string()))?;
            extracted += std::io::copy(
                &mut file.take(MAX_EXTRACTED_BYTES - extracted + 1),
                &mut out,
            )
            .map_err(|e| PackError::Extract(e.to_string()))?;
            if extracted > MAX_EXTRACTED_BYTES {
                return Err(PackError::Extract(
                    "resource pack exceeds extraction limit".into(),
                ));
            }
        }
    }

    tracing::info!("Extracted resource pack to {}", dest.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use super::*;

    fn zip_pack(asset: &str, contents: &[u8]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        archive
            .start_file("pack.mcmeta", zip::write::SimpleFileOptions::default())
            .unwrap();
        archive
            .write_all(br#"{"pack":{"pack_format":88,"description":"test"}}"#)
            .unwrap();
        archive
            .start_file(asset, zip::write::SimpleFileOptions::default())
            .unwrap();
        archive.write_all(contents).unwrap();
        archive.finish().unwrap().into_inner()
    }

    // A bounded stdlib HTTP peer; a skipped second download fails instead of
    // leaving listener.accept() hanging forever.
    fn serve(responses: Vec<(u16, Vec<u8>, Option<u64>)>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for (status, bytes, length) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "expected another pack request");
                            std::thread::sleep(Duration::from_millis(1));
                        }
                        Err(e) => panic!("{e}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = [0; 1024];
                stream.read(&mut request).unwrap();
                write!(
                    stream,
                    "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    length.unwrap_or(bytes.len() as u64)
                )
                .unwrap();
                stream.write_all(&bytes).unwrap();
            }
        });
        (format!("http://{address}/"), server)
    }

    #[test]
    fn local_pack_scan_and_saved_selection_restore_order_and_skip_missing_paths() {
        let root = std::env::temp_dir().join(format!("pomme-pack-{}", uuid::Uuid::new_v4()));
        let packs_dir = root.join("resourcepacks");
        for name in ["first", "second"] {
            let pack = packs_dir.join(name);
            std::fs::create_dir_all(&pack).unwrap();
            std::fs::write(
                pack.join("pack.mcmeta"),
                br#"{"pack":{"pack_format":88,"description":"test"}}"#,
            )
            .unwrap();
        }
        std::fs::create_dir_all(packs_dir.join(".server_cache/not-a-pack")).unwrap();
        std::fs::write(
            root.join("options.json"),
            br#"{"resource_packs":["second","missing","../outside","first"]}"#,
        )
        .unwrap();

        let manager = ResourcePackManager::new(&root);
        let active = manager.active_pack_info();
        assert_eq!(
            active
                .iter()
                .map(|pack| pack.name.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert!(
            active
                .iter()
                .all(|pack| pack.source == PackSource::Local && pack.enabled)
        );
        assert_eq!(manager.available_local_packs().len(), 2);
        assert!(!root.join("outside").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_hash_same_uuid_downloads_again_and_applies_actual_returned_path() {
        let root = std::env::temp_dir().join(format!("pomme-pack-{}", uuid::Uuid::new_v4()));
        let mut manager = ResourcePackManager::new(&root);
        let cache = manager.server_cache_dir().to_path_buf();
        let id = uuid::Uuid::new_v4();
        let (url, server) = serve(vec![
            (200, zip_pack("assets/test/payload.txt", b"A"), None),
            (200, zip_pack("assets/test/payload.txt", b"B"), None),
        ]);
        let first = ResourcePackManager::download_server_pack(&cache, id, &url, "").unwrap();
        manager.apply_server_pack(id, "", first.clone());
        let second = ResourcePackManager::download_server_pack(&cache, id, &url, "").unwrap();
        server.join().unwrap();
        manager.apply_server_pack(id, "", second.clone());
        assert_ne!(first, second);
        assert_eq!(manager.active_pack_dirs().next(), Some(second.as_path()));
        assert_eq!(
            std::fs::read(manager.resolve_asset("test/payload.txt").unwrap()).unwrap(),
            b"B"
        );
        assert_eq!(
            std::fs::read(first.join("assets/test/payload.txt")).unwrap(),
            b"A"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parallel_same_uuid_old_result_cannot_overwrite_active_new_content() {
        let root = std::env::temp_dir().join(format!("pomme-pack-{}", uuid::Uuid::new_v4()));
        let mut manager = ResourcePackManager::new(&root);
        let cache = manager.server_cache_dir().to_path_buf();
        let id = uuid::Uuid::new_v4();
        let (old_url, old_server) = serve(vec![(
            200,
            zip_pack("assets/test/payload.txt", b"old"),
            None,
        )]);
        let (new_url, new_server) = serve(vec![(
            200,
            zip_pack("assets/test/payload.txt", b"new"),
            None,
        )]);
        let old_cache = cache.clone();
        let old = std::thread::spawn(move || {
            ResourcePackManager::download_server_pack(&old_cache, id, &old_url, "")
        });
        let new = ResourcePackManager::download_server_pack(&cache, id, &new_url, "").unwrap();
        manager.apply_server_pack(id, "", new.clone());
        let old = old.join().unwrap().unwrap();
        old_server.join().unwrap();
        new_server.join().unwrap();
        assert_ne!(old, new);
        assert_eq!(
            std::fs::read(old.join("assets/test/payload.txt")).unwrap(),
            b"old"
        );
        assert_eq!(
            std::fs::read(manager.resolve_asset("test/payload.txt").unwrap()).unwrap(),
            b"new"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_extract_is_not_cached_and_hash_cache_never_replaces_existing_directory() {
        let root = std::env::temp_dir().join(format!("pomme-pack-{}", uuid::Uuid::new_v4()));
        let manager = ResourcePackManager::new(&root);
        let cache = manager.server_cache_dir();
        let id = uuid::Uuid::new_v4();
        let bad = zip_pack("../escaped.txt", b"bad");
        let bad_hash = sha1_smol::Sha1::from(&bad).digest().to_string();
        let good = zip_pack("assets/test/payload.txt", b"good");
        let hash = sha1_smol::Sha1::from(&good).digest().to_string();
        let legacy = cache.join(&hash);
        std::fs::create_dir(&legacy).unwrap();
        std::fs::write(legacy.join("partial"), b"do not overwrite").unwrap();
        let (url, server) = serve(vec![
            (200, bad.clone(), None),
            (200, bad, None),
            (200, good.clone(), None),
        ]);
        for _ in 0..2 {
            assert!(matches!(
                ResourcePackManager::download_server_pack(cache, id, &url, &bad_hash),
                Err(PackError::Extract(_))
            ));
            assert!(!cache.join(&bad_hash).exists());
        }
        let downloaded = ResourcePackManager::download_server_pack(cache, id, &url, &hash).unwrap();
        server.join().unwrap();
        assert_ne!(downloaded, legacy);
        assert!(valid_server_cache(&downloaded, &hash));
        assert_eq!(
            std::fs::read(legacy.join("partial")).unwrap(),
            b"do not overwrite"
        );
        assert!(!root.join("escaped.txt").exists());
        assert!(!std::fs::read_dir(cache).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".staging-")
        }));
        // A new valid hash cache is reusable without another HTTP request.
        let (url, server) = serve(vec![(200, good, None)]);
        let fresh = cache.join("fresh");
        let first = ResourcePackManager::download_server_pack(&fresh, id, &url, &hash).unwrap();
        server.join().unwrap();
        assert_eq!(
            ResourcePackManager::download_server_pack(&fresh, id, &url, &hash.to_ascii_uppercase())
                .unwrap(),
            first
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn network_and_zip_boundaries_reject_bad_inputs() {
        let root = std::env::temp_dir().join(format!("pomme-pack-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4();
        for url in [
            "file:///test.zip",
            "ftp://example.org/test.zip",
            "not a URL",
        ] {
            assert!(ResourcePackManager::download_server_pack(&root, id, url, "").is_err());
        }
        assert!(matches!(
            ResourcePackManager::download_server_pack(&root, id, "http://127.0.0.1/", "../bad"),
            Err(PackError::InvalidHash)
        ));
        let bytes = zip_pack("assets/test/payload.txt", b"good");
        let (url, server) = serve(vec![
            (404, bytes.clone(), None),
            (200, Vec::new(), Some(MAX_PACK_BYTES + 1)),
            (200, bytes, None),
        ]);
        assert!(matches!(
            ResourcePackManager::download_server_pack(&root, id, &url, ""),
            Err(PackError::Download(_))
        ));
        assert!(matches!(
            ResourcePackManager::download_server_pack(&root, id, &url, ""),
            Err(PackError::Download(_))
        ));
        assert!(matches!(
            ResourcePackManager::download_server_pack(&root, id, &url, &"0".repeat(40)),
            Err(PackError::HashMismatch)
        ));
        server.join().unwrap();
        for path in [
            "../escape",
            "a/../../escape",
            "a\\..\\escape",
            "C:/escape",
            "/escape",
            "\\escape",
            "a/.. /escape",
            "CON.txt",
        ] {
            assert!(
                extract_zip(&zip_pack(path, b"bad"), &root).is_err(),
                "accepted {path}"
            );
        }
        let mut oversized = zip_pack("assets/test/payload.txt", b"small");
        let central = oversized
            .windows(4)
            .position(|w| w == b"PK\x01\x02")
            .unwrap();
        oversized[central + 24..central + 28]
            .copy_from_slice(&((MAX_EXTRACTED_BYTES + 1) as u32).to_le_bytes());
        assert!(extract_zip(&oversized, &root).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
