//! CPU-only, full-sheet skins for placed player heads. No UI faces or GPU
//! objects.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use super::{
    SkinData, fetch_skin_texture, fetch_skin_texture_by_name,
    fetch_skin_texture_from_profile_property, process_legacy_skin,
};
use crate::assets::{AssetId, AssetIndex, resolve_asset_path_with_pack_dirs};
use crate::world::block_entity::PlayerHeadProfileSource;

const MAX_PENDING: usize = 32;
// Failed and pending entries also consume a slot: never more than 128
// sheets/keys.
pub(super) const MAX_ENTRIES: usize = 128;
const UNUSED_RETENTION: Duration = Duration::from_secs(60);
const DEFAULT_SOURCE: PlayerHeadProfileSource = PlayerHeadProfileSource::Default;
type Completion = (u64, PlayerHeadProfileSource, Result<SkinData, String>);

enum State {
    Pending,
    Ready(SkinData),
    Failed,
}

struct Entry {
    state: State,
    last_seen: Instant,
}

pub(super) struct PlacedHeadSkinCache {
    entries: HashMap<PlayerHeadProfileSource, Entry>,
    pending: usize,
    tx: SyncSender<Completion>,
    rx: Receiver<Completion>,
    fallback: SkinData,
    generation: u64,
    revision: u64,
}

impl PlacedHeadSkinCache {
    /// Load slim Steve through the same pack-aware loader as patched heads,
    /// only at startup or asset reload, never during frame rendering.
    pub(super) fn load(
        jar: &Path,
        index: &Option<AssetIndex>,
        packs: &[PathBuf],
    ) -> Result<Self, String> {
        Ok(Self::new(Self::resource_skin(
            "minecraft:entity/player/slim/steve",
            jar,
            index,
            packs,
        )?))
    }

    pub(super) fn reload(
        &mut self,
        jar: &Path,
        index: &Option<AssetIndex>,
        packs: &[PathBuf],
    ) -> Result<(), String> {
        self.invalidate();
        self.fallback = Self::load(jar, index, packs)?.fallback;
        Ok(())
    }

    fn new(fallback: SkinData) -> Self {
        let (tx, rx) = mpsc::sync_channel(MAX_PENDING);
        Self {
            entries: HashMap::new(),
            pending: 0,
            tx,
            rx,
            fallback,
            generation: 0,
            revision: 0,
        }
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Changes when sheets become ready or are retired; GUI bakes use this
    /// stamp.
    pub(super) fn revision(&self) -> u64 {
        self.revision
    }

    /// Drop all resource-dependent sheets and give the next generation its own
    /// bounded channel. Old jobs cannot occupy new reservations or publish
    /// skins.
    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.revision = self.revision.wrapping_add(1);
        self.entries.clear();
        self.pending = 0;
        (self.tx, self.rx) = mpsc::sync_channel(MAX_PENDING);
    }

    /// Called once before world drawing. No disk IO, HTTP, decode, waits, or
    /// worker creation on cache hits; even 1000 distinct heads admit at most
    /// 32 outstanding jobs per generation (undrained results still count).
    pub(super) fn update<'a>(
        &mut self,
        sources: impl Iterator<Item = &'a PlayerHeadProfileSource> + Clone,
        rt: &tokio::runtime::Runtime,
        jar: &Path,
        index: &Option<AssetIndex>,
        pack_dirs: &[PathBuf],
    ) {
        self.drain();
        let now = Instant::now();
        for source in sources.clone() {
            if Self::supported(source) {
                if let Some(entry) = self.entries.get_mut(source) {
                    entry.last_seen = now;
                }
            }
        }
        let previous_len = self.entries.len();
        self.entries.retain(|_, entry| {
            matches!(entry.state, State::Pending)
                || now.duration_since(entry.last_seen) < UNUSED_RETENTION
        });
        if self.entries.len() != previous_len {
            self.revision = self.revision.wrapping_add(1);
        }
        for source in sources {
            if !self.reserve(source, now) {
                continue;
            }
            let source = source.clone();
            let tx = self.tx.clone();
            let generation = self.generation;
            let assets = (jar.to_owned(), index.clone(), pack_dirs.to_vec());
            let fallback = SkinData {
                pixels: self.fallback.pixels.clone(),
                width: self.fallback.width,
                height: self.fallback.height,
                slim: self.fallback.slim,
            };
            rt.spawn(async move {
                let result = tokio::time::timeout(
                    Duration::from_secs(30),
                    Self::resolve(&source, assets, fallback),
                )
                .await
                .unwrap_or_else(|_| Err("head skin resolution timed out".into()));
                // One result per reservation; never block a runtime worker.
                let _ = tx.try_send((generation, source, result));
            });
        }
    }

    fn supported(source: &PlayerHeadProfileSource) -> bool {
        use crate::world::block_entity::valid_player_head_resource_texture;
        let Some(patch) = source.patch() else {
            return false;
        };
        if [&patch.texture, &patch.cape, &patch.elytra]
            .iter()
            .any(|v| {
                v.as_ref()
                    .is_some_and(|v| !valid_player_head_resource_texture(v))
            })
            || patch
                .model
                .as_ref()
                .is_some_and(|v| !matches!(v.as_str(), "slim" | "wide"))
        {
            return false;
        }
        match source {
            PlayerHeadProfileSource::Static {
                name, properties, ..
            } => {
                name.as_ref().is_none_or(|v| v.len() <= 16)
                    && properties.len() <= 16
                    && properties.iter().all(|p| {
                        p.name.len() <= 192
                            && p.value.len() <= super::MAX_TEXTURE_PROPERTY_BYTES
                            && p.signature.as_ref().is_none_or(|v| v.len() <= 3072)
                    })
            }
            PlayerHeadProfileSource::DynamicName { name, .. } => {
                crate::player::valid_player_name(name)
            }
            PlayerHeadProfileSource::DynamicId { .. } => true,
            PlayerHeadProfileSource::Default => false,
        }
    }

    async fn resolve(
        source: &PlayerHeadProfileSource,
        assets: (PathBuf, Option<AssetIndex>, Vec<PathBuf>),
        fallback: SkinData,
    ) -> Result<SkinData, String> {
        let patch = source.patch().ok_or("default profile")?;
        // A body patch replaces the resolved body, not the profile identity.
        // Avoid a needless HTTP lookup when the final body is already local.
        let mut skin = if let Some(texture) = &patch.texture {
            let texture = texture.clone();
            tokio::task::spawn_blocking(move || {
                Self::resource_skin(&texture, &assets.0, &assets.1, &assets.2)
            })
            .await
            .map_err(|e| e.to_string())??
        } else {
            let result = match source {
                PlayerHeadProfileSource::Static { properties, .. } => {
                    if let Some(property) = properties.iter().find(|p| p.name == "textures") {
                        fetch_skin_texture_from_profile_property(&property.value).await
                    } else {
                        // Official Static.resolveProfile is completedFuture(partialProfile).
                        // NEVER replace missing static properties with a name/id lookup.
                        Ok(fallback)
                    }
                }
                PlayerHeadProfileSource::DynamicName { name, .. } => {
                    fetch_skin_texture_by_name(name).await
                }
                PlayerHeadProfileSource::DynamicId { id, .. } => {
                    fetch_skin_texture(&id.simple().to_string()).await
                }
                PlayerHeadProfileSource::Default => unreachable!(),
            };
            result?
        };
        if let Some(model) = &patch.model {
            skin.slim = model == "slim";
        }
        Ok(skin)
    }

    fn resource_skin(
        texture: &str,
        jar: &Path,
        index: &Option<AssetIndex>,
        packs: &[PathBuf],
    ) -> Result<SkinData, String> {
        if !crate::world::block_entity::valid_player_head_resource_texture(texture) {
            return Err("invalid head texture asset".into());
        }
        let key = AssetId::parse(texture).asset_key("textures", ".png");
        let path = resolve_asset_path_with_pack_dirs(jar, index, &key, packs);
        // Bound both encoded input and decoder allocation, including pack files.
        use std::io::Read;
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("head texture exceeds 2 MiB".into());
        }
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(64);
        limits.max_image_height = Some(64);
        limits.max_alloc = Some(64 * 64 * 16);
        reader.limits(limits);
        let rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
        let (width, height) = rgba.dimensions();
        if width != 64 || !matches!(height, 32 | 64) {
            return Err("invalid head texture dimensions".into());
        }
        let (pixels, width, height) = if height == 32 {
            process_legacy_skin(rgba.into_raw(), width, height)?
        } else {
            (rgba.into_raw(), width, height)
        };
        Ok(SkinData {
            pixels,
            width,
            height,
            slim: true,
        })
    }

    fn reserve(&mut self, source: &PlayerHeadProfileSource, now: Instant) -> bool {
        if !Self::supported(source)
            || self.pending >= MAX_PENDING
            || self.entries.len() >= MAX_ENTRIES
            || self.entries.contains_key(source)
        {
            return false;
        }
        self.entries.insert(
            source.clone(),
            Entry {
                state: State::Pending,
                last_seen: now,
            },
        );
        self.pending += 1;
        true
    }

    fn drain(&mut self) {
        while let Ok((generation, source, result)) = self.rx.try_recv() {
            if generation != self.generation {
                continue;
            }
            // A completion only updates its complete source key, never a block
            // position or the source now replacing it. Pending keys are not evicted.
            let Some(entry) = self.entries.get_mut(&source) else {
                continue;
            };
            if !matches!(entry.state, State::Pending) {
                continue;
            }
            self.pending -= 1;
            entry.state = match result {
                Ok(skin)
                    if skin.width == 64
                        && skin.height == 64
                        && skin.pixels.len() == 64 * 64 * 4 =>
                {
                    self.revision = self.revision.wrapping_add(1);
                    State::Ready(skin)
                }
                _ => State::Failed,
            };
        }
    }

    /// Only validated, decoded sheets can reach GPU upload.
    pub(super) fn ready(&self) -> impl Iterator<Item = (&PlayerHeadProfileSource, &SkinData)> {
        self.entries
            .iter()
            .filter_map(|(source, entry)| match &entry.state {
                State::Ready(skin) => Some((source, skin)),
                _ => None,
            })
    }

    /// Select by the current *resolved* source, never by position or last draw.
    /// Generic only so descriptor selection can be tested without a Vulkan
    /// device.
    pub(super) fn texture<'a, T>(
        &self,
        source: Option<&PlayerHeadProfileSource>,
        slots: &'a HashMap<PlayerHeadProfileSource, T>,
        fallback: &'a T,
    ) -> &'a T {
        let Some(source) = source else {
            return fallback;
        };
        let (key, _) = self.skin(source);
        if *key == PlayerHeadProfileSource::Default {
            fallback
        } else {
            slots.get(key).unwrap_or(fallback)
        }
    }

    /// Resolved texture key + borrowed RGBA sheet for the GPU phase.
    /// Fallback returns Default as its key, so it cannot be uploaded under a
    /// pending/failed key and then mistaken for that source's completed skin.
    pub(super) fn skin(
        &self,
        source: &PlayerHeadProfileSource,
    ) -> (&PlayerHeadProfileSource, &SkinData) {
        if Self::supported(source) {
            if let Some((
                key,
                Entry {
                    state: State::Ready(skin),
                    ..
                },
            )) = self.entries.get_key_value(source)
            {
                return (key, skin);
            }
        }
        (&DEFAULT_SOURCE, &self.fallback)
    }
}

// ponytail: visible entries (including failures) stay pinned; excess sources
// use Steve rather than cycling downloads through 128 slots every frame.
// Unused entries expire after 60s; reappearance can then retry. Add fair/LRU
// admission only with a separate bounded retry budget, not eviction-on-insert.

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(byte: u8) -> SkinData {
        SkinData {
            pixels: vec![byte; 64 * 64 * 4],
            width: 64,
            height: 64,
            slim: true,
        }
    }

    fn source(value: &str) -> PlayerHeadProfileSource {
        PlayerHeadProfileSource::Static {
            name: None,
            id: None,
            properties: vec![crate::world::block_entity::PlayerHeadProfileProperty {
                name: "textures".into(),
                value: value.into(),
                signature: None,
            }],
            patch: Default::default(),
        }
    }

    fn finish(
        cache: &mut PlacedHeadSkinCache,
        source: PlayerHeadProfileSource,
        result: Result<SkinData, String>,
    ) {
        assert!(
            cache
                .tx
                .try_send((cache.generation, source, result))
                .is_ok()
        );
        cache.drain();
    }

    #[test]
    fn descriptor_selection_keeps_ready_a_and_b_distinct_in_one_frame() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let a = source("profile A");
        let b = source("profile B");
        for key in [&a, &b] {
            assert!(cache.reserve(key, Instant::now()));
        }
        let mut slots = HashMap::new();
        let steve = 0u64;
        assert_eq!(*cache.texture(Some(&a), &slots, &steve), steve);
        finish(&mut cache, a.clone(), Ok(sheet(17)));
        finish(&mut cache, b.clone(), Ok(sheet(23)));
        // Model the upload handoff in the very same frame as drain(), without
        // another update or a block-position change. Both sheets are distinct.
        for (key, skin) in cache.ready() {
            slots.insert(key.clone(), u64::from(skin.pixels[0]));
        }
        for key in [&a, &b, &a, &b] {
            let expected = if key == &a { 17 } else { 23 };
            assert_eq!(*cache.texture(Some(key), &slots, &steve), expected);
        }
    }

    #[test]
    fn descriptor_selection_defaults_missing_pending_failed_and_unsupported() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let pending = source("pending");
        let failed = source("failed");
        let ready_without_gpu = source("ready but not uploaded");
        for key in [&pending, &failed, &ready_without_gpu] {
            assert!(cache.reserve(key, Instant::now()));
        }
        finish(&mut cache, failed.clone(), Err("invalid URL".into()));
        finish(&mut cache, ready_without_gpu.clone(), Ok(sheet(7)));
        // Even a stale slot under an unresolved key must not be selected.
        let slots = HashMap::from([(pending.clone(), 11), (failed.clone(), 12)]);
        let steve = 0;
        assert_eq!(*cache.texture(None, &slots, &steve), steve);
        for key in [
            pending,
            failed,
            ready_without_gpu,
            source("unknown"),
            PlayerHeadProfileSource::Default,
            PlayerHeadProfileSource::DynamicName {
                name: "Steve".into(),
                patch: Default::default(),
            },
            PlayerHeadProfileSource::DynamicId {
                id: uuid::Uuid::nil(),
                patch: Default::default(),
            },
        ] {
            assert_eq!(*cache.texture(Some(&key), &slots, &steve), steve);
        }
    }

    #[test]
    fn descriptor_selection_rechecks_changed_source_instead_of_reusing_old_slot() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let old = source("old");
        let changed = source("changed");
        assert!(cache.reserve(&old, Instant::now()));
        finish(&mut cache, old.clone(), Ok(sheet(1)));
        let mut slots = HashMap::from([(old.clone(), 101)]);
        let steve = 0;
        assert_eq!(*cache.texture(Some(&old), &slots, &steve), 101);
        assert!(cache.reserve(&changed, Instant::now()));
        assert_eq!(*cache.texture(Some(&changed), &slots, &steve), steve);
        finish(&mut cache, changed.clone(), Ok(sheet(2)));
        assert_eq!(*cache.texture(Some(&changed), &slots, &steve), steve);
        slots.insert(changed.clone(), 102);
        assert_eq!(*cache.texture(Some(&changed), &slots, &steve), 102);
        assert_eq!(*cache.texture(Some(&old), &slots, &steve), 101);
    }

    #[test]
    fn descriptor_selection_after_invalidate_never_returns_retired_set() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let key = source("ready");
        assert!(cache.reserve(&key, Instant::now()));
        finish(&mut cache, key.clone(), Ok(sheet(1)));
        let mut slots = HashMap::from([(key.clone(), 101)]);
        let steve = 0;
        assert_eq!(*cache.texture(Some(&key), &slots, &steve), 101);
        // The GPU invalidation path drains all slots after device idle. The
        // validated CPU sheet survives a GPU-only rebuild, but its old set does not.
        let retired: Vec<_> = slots.drain().collect();
        assert_eq!(retired.len(), 1);
        assert_eq!(*cache.texture(Some(&key), &slots, &steve), steve);
        slots.insert(key.clone(), 202);
        assert_eq!(*cache.texture(Some(&key), &slots, &steve), 202);
        cache.entries.remove(&key);
        assert_eq!(*cache.texture(Some(&key), &slots, &steve), steve);
    }

    #[test]
    fn descriptor_selection_preserves_transparent_hat_on_the_full_sheet() {
        let mut cache = PlacedHeadSkinCache::new(sheet(255));
        let key = source("opaque head with transparent and translucent hat");
        let mut skin = sheet(255);
        // Head UV (8,8); hat UV (40,8). Keep RGB even for alpha-zero texels.
        let head_alpha = (8 * 64 + 8) * 4 + 3;
        let hat_alpha = (8 * 64 + 40) * 4 + 3;
        skin.pixels[hat_alpha] = 0;
        skin.pixels[hat_alpha + 4] = 192;
        let expected = skin.pixels.clone();
        assert!(cache.reserve(&key, Instant::now()));
        finish(&mut cache, key.clone(), Ok(skin));
        let slots: HashMap<_, _> = cache
            .ready()
            .map(|(key, skin)| (key.clone(), (17, skin.pixels.clone())))
            .collect();
        let steve = (0, sheet(255).pixels);
        let selected = cache.texture(Some(&key), &slots, &steve);
        assert_eq!(selected.0, 17);
        assert_eq!(selected.1, expected);
        assert_eq!(selected.1[head_alpha], 255);
        assert_eq!(selected.1[hat_alpha], 0);
        assert_eq!(selected.1[hat_alpha + 4], 192);
    }

    #[test]
    fn deduplicates_complete_source_and_guards_replacement() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let now = Instant::now();
        let old = source("old complete property");
        let changed = source("changed complete property");
        assert!(cache.reserve(&old, now));
        assert!(!cache.reserve(&old, now));
        assert!(cache.reserve(&changed, now));
        assert_eq!(cache.pending, 2);
        finish(&mut cache, old.clone(), Ok(sheet(17)));
        assert_eq!(cache.skin(&old).0, &old);
        assert_eq!(cache.skin(&old).1.pixels, vec![17; 64 * 64 * 4]);
        assert_eq!(cache.skin(&changed).0, &DEFAULT_SOURCE);
        // Duplicate and unknown completions cannot overwrite a ready entry.
        finish(&mut cache, old.clone(), Ok(sheet(99)));
        finish(&mut cache, source("not reserved"), Ok(sheet(99)));
        assert_eq!(cache.pending, 1);
        assert_eq!(cache.skin(&old).1.pixels[0], 17);
        finish(&mut cache, changed.clone(), Ok(sheet(23)));
        assert_eq!(cache.skin(&changed).0, &changed);
        assert_eq!(cache.skin(&changed).1.pixels[0], 23);
        assert_eq!(cache.pending, 0);
        assert!(!cache.reserve(&changed, now));
    }

    #[test]
    fn bounds_pending_ready_and_failed_without_eviction_churn() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let now = Instant::now();
        for batch in 0..4 {
            for i in 0..MAX_PENDING {
                assert!(cache.reserve(&source(&format!("{batch}/{i}")), now));
            }
            assert_eq!(cache.pending, MAX_PENDING);
            assert!(!cache.reserve(&source("overflow"), now));
            for i in 0..MAX_PENDING {
                finish(&mut cache, source(&format!("{batch}/{i}")), Ok(sheet(1)));
            }
        }
        assert_eq!(cache.entries.len(), MAX_ENTRIES);
        assert_eq!(cache.ready().count(), MAX_ENTRIES);
        for i in 0..1000 {
            assert!(!cache.reserve(&source(&format!("extra {i}")), now));
        }
        assert_eq!(cache.pending, 0);
        assert_eq!(cache.entries.len(), 128);

        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let bad = source("invalid base64!");
        assert!(cache.reserve(&bad, now));
        finish(&mut cache, bad.clone(), Err("rejected".into()));
        for _ in 0..1000 {
            assert!(!cache.reserve(&bad, now));
            assert_eq!(cache.skin(&bad).0, &DEFAULT_SOURCE);
        }
        assert_eq!(cache.pending, 0);
    }

    #[test]
    fn rejects_oversized_property_before_admission_and_defaults_unsupported_sources() {
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        for source in [
            source(&"A".repeat(super::super::MAX_TEXTURE_PROPERTY_BYTES + 1)),
            PlayerHeadProfileSource::Default,
            PlayerHeadProfileSource::DynamicName {
                name: "unsafe name".into(),
                patch: Default::default(),
            },
            PlayerHeadProfileSource::Static {
                name: None,
                id: None,
                properties: Vec::new(),
                patch: crate::world::block_entity::PlayerHeadSkinPatch {
                    texture: Some("minecraft:../unsafe".into()),
                    ..Default::default()
                },
            },
        ] {
            assert!(!cache.reserve(&source, Instant::now()));
            let (key, fallback) = cache.skin(&source);
            assert_eq!(key, &DEFAULT_SOURCE);
            assert_eq!(
                (fallback.width, fallback.height, fallback.slim),
                (64, 64, true)
            );
        }
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn real_worker_rejects_encoded_input_and_completes_without_network() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let bad = source("not base64!");
        cache.update(std::iter::once(&bad), &rt, Path::new("."), &None, &[]);
        // Finite wait in a test only; the frame path exclusively uses try_recv.
        let completion = cache.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(completion.1, bad);
        assert!(completion.2.is_err());
        assert!(cache.tx.try_send(completion).is_ok());
        cache.update(std::iter::once(&bad), &rt, Path::new("."), &None, &[]);
        assert_eq!(cache.pending, 0);
        assert!(matches!(cache.entries[&bad].state, State::Failed));
        assert_eq!(cache.skin(&bad).0, &DEFAULT_SOURCE);
        assert!(cache.rx.try_recv().is_err());
    }

    #[test]
    fn nbt_stored_profiles_admit_list_map_static_dynamic_and_patch() {
        use azalea_registry::builtin::BlockEntityKind;
        use simdnbt::owned::{NbtCompound, NbtList};

        use crate::world::block_entity::{PlayerHeadSkinPatch, StoredBlockEntity};
        let mut property = NbtCompound::new();
        property.insert("name", "textures");
        property.insert("value", "embedded");
        let mut list_profile = NbtCompound::new();
        list_profile.insert("properties", NbtList::Compound(vec![property]));
        let mut map = NbtCompound::new();
        map.insert("textures", NbtList::String(vec!["embedded".into()]));
        let mut map_profile = NbtCompound::new();
        map_profile.insert("properties", map);
        let stored = |profile: NbtCompound| {
            let mut nbt = NbtCompound::new();
            nbt.insert("profile", profile);
            StoredBlockEntity::new(BlockEntityKind::Skull, nbt)
                .player_head_profile_source
                .unwrap()
        };
        let list = stored(list_profile.clone());
        assert_eq!(list, stored(map_profile));
        assert_eq!(list, source("embedded"));
        // Preserve all profile fields and signatures, not only the selected value.
        list_profile.insert("name", "Alex");
        list_profile.insert("id", simdnbt::owned::NbtTag::IntArray(vec![0, 0, 0, 1]));
        list_profile.insert("texture", "custom:entity/skin");
        list_profile.insert("cape", "custom:cape");
        list_profile.insert("elytra", "custom:elytra");
        list_profile.insert("model", "wide");
        let full = stored(list_profile);
        assert!(
            matches!(&full, PlayerHeadProfileSource::Static { name: Some(name), id: Some(id), properties, patch }
            if name == "Alex" && *id == uuid::Uuid::from_u128(1) && properties[0].value == "embedded"
                && patch.cape.as_deref() == Some("custom:cape") && patch.elytra.as_deref() == Some("custom:elytra"))
        );
        let mut name = NbtCompound::new();
        name.insert("name", "Alex");
        let dynamic_name = stored(name.clone());
        assert!(
            matches!(&dynamic_name, PlayerHeadProfileSource::DynamicName { name, .. } if name == "Alex")
        );
        let mut id = NbtCompound::new();
        id.insert("id", simdnbt::owned::NbtTag::IntArray(vec![0, 0, 0, 1]));
        let dynamic_id = stored(id);
        assert!(
            matches!(&dynamic_id, PlayerHeadProfileSource::DynamicId { id, .. } if *id == uuid::Uuid::from_u128(1))
        );
        name.insert("id", simdnbt::owned::NbtTag::IntArray(vec![0, 0, 0, 1]));
        let static_empty = stored(name);
        assert!(
            matches!(&static_empty, PlayerHeadProfileSource::Static { properties, .. } if properties.is_empty())
        );
        let mut patch = NbtCompound::new();
        patch.insert("texture", "custom:skin");
        let patch_only = stored(patch);
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        for key in [
            &list,
            &full,
            &dynamic_name,
            &dynamic_id,
            &static_empty,
            &patch_only,
        ] {
            assert!(cache.reserve(key, Instant::now()));
        }
        assert_eq!(cache.pending, 6);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        // No live HTTP: a static identity with no properties returns the default,
        // whereas an accidental dynamic substitution would await a real lookup.
        let skin = rt.block_on(async {
            tokio::time::timeout(
                Duration::from_secs(1),
                PlacedHeadSkinCache::resolve(
                    &static_empty,
                    (PathBuf::new(), None, vec![]),
                    sheet(42),
                ),
            )
            .await
            .unwrap()
            .unwrap()
        });
        assert_eq!(skin.pixels[0], 42);
        let mut invalid = NbtCompound::new();
        invalid.insert("texture", "minecraft:../secret");
        assert_eq!(stored(invalid), PlayerHeadProfileSource::Default);
        let unsafe_patch = PlayerHeadProfileSource::DynamicId {
            id: uuid::Uuid::nil(),
            patch: PlayerHeadSkinPatch {
                texture: Some("file:/secret".into()),
                ..Default::default()
            },
        };
        assert!(!cache.reserve(&unsafe_patch, Instant::now()));
    }

    #[test]
    fn pack_patch_resolution_preserves_alpha_and_reload_rejects_old_jobs() {
        use crate::world::block_entity::PlayerHeadSkinPatch;
        let root = std::env::temp_dir().join(format!("pomme-head-{}", uuid::Uuid::new_v4()));
        let jar = root.join("jar");
        let pack = root.join("pack");
        let relative = "custom/textures/head.png";
        for (dir, byte) in [(&jar, 11), (&pack.join("assets"), 23)] {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            image::RgbaImage::from_pixel(64, 64, image::Rgba([byte, byte, byte, 0]))
                .save(path)
                .unwrap();
        }
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let source = PlayerHeadProfileSource::Static {
            name: Some("Alex".into()),
            id: Some(uuid::Uuid::nil()),
            properties: vec![],
            patch: PlayerHeadSkinPatch {
                texture: Some("custom:head".into()),
                model: Some("wide".into()),
                ..Default::default()
            },
        };
        let skin = rt
            .block_on(PlacedHeadSkinCache::resolve(
                &source,
                (jar.clone(), None, vec![pack.clone()]),
                sheet(0),
            ))
            .unwrap();
        assert_eq!(&skin.pixels[..4], &[23, 23, 23, 0]);
        assert!(!skin.slim);
        assert!(PlacedHeadSkinCache::resource_skin("custom:../head", &jar, &None, &[]).is_err());
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        assert!(cache.reserve(&source, Instant::now()));
        let old_generation = cache.generation();
        let old_tx = cache.tx.clone();
        cache.invalidate();
        assert_eq!(cache.revision(), 1);
        assert_eq!(cache.pending, 0);
        assert!(
            old_tx
                .try_send((old_generation, source.clone(), Ok(sheet(99))))
                .is_err()
        );
        assert!(cache.reserve(&source, Instant::now()));
        // Even if an old generation is delivered on the new channel, ignore it.
        cache
            .tx
            .try_send((old_generation, source.clone(), Ok(sheet(99))))
            .ok()
            .unwrap();
        cache.drain();
        assert_eq!(cache.pending, 1);
        assert_eq!(cache.skin(&source).0, &DEFAULT_SOURCE);
        finish(&mut cache, source.clone(), Ok(skin));
        assert_eq!(cache.revision(), 2);
        assert_eq!(cache.skin(&source).1.pixels[0], 23);
        cache.invalidate();
        assert_eq!(cache.skin(&source).0, &DEFAULT_SOURCE);
        image::RgbaImage::from_pixel(64, 64, image::Rgba([31, 31, 31, 0]))
            .save(pack.join("assets").join(relative))
            .unwrap();
        let reloaded = rt
            .block_on(PlacedHeadSkinCache::resolve(
                &source,
                (jar, None, vec![pack]),
                sheet(0),
            ))
            .unwrap();
        assert!(cache.reserve(&source, Instant::now()));
        finish(&mut cache, source.clone(), Ok(reloaded));
        assert_eq!(cache.skin(&source).1.pixels[0], 31);
        assert_eq!(cache.revision(), 4);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn head_fallback_reload_tracks_pack_changes_and_keeps_valid_sheet_on_error() {
        let root = std::env::temp_dir().join(format!("pomme-head-{}", uuid::Uuid::new_v4()));
        let jar = root.join("jar");
        let pack = root.join("pack");
        let relative = "minecraft/textures/entity/player/slim/steve.png";
        for (dir, byte) in [(&jar, 11), (&pack.join("assets"), 23)] {
            let path = dir.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            image::RgbaImage::from_pixel(64, 64, image::Rgba([byte, byte, byte, 0]))
                .save(path)
                .unwrap();
        }
        let mut cache = PlacedHeadSkinCache::load(&jar, &None, &[]).unwrap();
        assert_eq!(cache.skin(&DEFAULT_SOURCE).1.pixels[0], 11);
        let pending = source("pending before reload");
        assert!(cache.reserve(&pending, Instant::now()));
        let old_tx = cache.tx.clone();
        cache.reload(&jar, &None, &[pack.clone()]).unwrap();
        assert_eq!(cache.generation(), 1);
        assert_eq!(cache.revision(), 1);
        assert_eq!(cache.pending, 0);
        assert!(cache.entries.is_empty());
        assert!(
            old_tx
                .try_send((0, pending.clone(), Ok(sheet(99))))
                .is_err()
        );
        let fallback = cache.skin(&pending).1;
        assert_eq!(&fallback.pixels[..4], &[23, 23, 23, 0]);
        let slots = HashMap::from([(DEFAULT_SOURCE, fallback.pixels[0])]);
        assert_eq!(
            *cache.texture(Some(&pending), &slots, &slots[&DEFAULT_SOURCE]),
            23
        );
        image::RgbaImage::from_pixel(64, 64, image::Rgba([31, 31, 31, 0]))
            .save(pack.join("assets").join(relative))
            .unwrap();
        cache.reload(&jar, &None, &[pack.clone()]).unwrap();
        assert_eq!(cache.skin(&DEFAULT_SOURCE).1.pixels[0], 31);
        cache.reload(&jar, &None, &[]).unwrap();
        assert_eq!(cache.skin(&DEFAULT_SOURCE).1.pixels[0], 11);
        std::fs::write(pack.join("assets").join(relative), b"invalid PNG").unwrap();
        assert!(cache.reload(&jar, &None, &[pack.clone()]).is_err());
        assert_eq!(cache.skin(&DEFAULT_SOURCE).1.pixels[0], 11);
        assert_eq!(cache.generation(), 4);
        assert_eq!(cache.revision(), 4);
        let startup = PlacedHeadSkinCache::load(&jar, &None, &[pack]);
        assert!(startup.is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encoded_untrusted_urls_fail_without_http() {
        use base64::Engine;
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        for url in [
            "file:///secret",
            "http://127.0.0.1/skin",
            "https://textures.minecraft.net:444/skin",
            "https://evil.example/skin",
        ] {
            let json = serde_json::json!({"textures": {"SKIN": {"url": url}}});
            let value = base64::engine::general_purpose::STANDARD.encode(json.to_string());
            let key = source(&value);
            assert!(PlacedHeadSkinCache::supported(&key));
            assert!(
                rt.block_on(PlacedHeadSkinCache::resolve(
                    &key,
                    (PathBuf::new(), None, vec![]),
                    sheet(0)
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn unused_keys_expire_but_pending_and_visible_keys_stay_pinned() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut cache = PlacedHeadSkinCache::new(sheet(0));
        let old_time = Instant::now() - UNUSED_RETENTION - Duration::from_secs(1);
        let pending = source("pending");
        let absent = source("absent");
        let visible = source("visible");
        for key in [&pending, &absent, &visible] {
            assert!(cache.reserve(key, old_time));
        }
        finish(&mut cache, absent.clone(), Err("failed".into()));
        finish(&mut cache, visible.clone(), Ok(sheet(7)));
        cache.update(std::iter::once(&visible), &rt, Path::new("."), &None, &[]);
        assert!(cache.entries.contains_key(&pending));
        assert!(cache.entries.contains_key(&visible));
        assert!(!cache.entries.contains_key(&absent));
        assert_eq!(cache.pending, 1);
        assert!(cache.reserve(&absent, Instant::now()));
    }
}
