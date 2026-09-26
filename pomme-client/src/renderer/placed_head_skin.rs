//! CPU-only, full-sheet skins for placed player heads. No UI faces or GPU
//! objects.
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use super::{SkinData, fetch_skin_texture_from_profile_property, process_legacy_skin};
use crate::assets::{AssetIndex, load_image, resolve_asset_path};
use crate::world::block_entity::PlayerHeadProfileSource;

const MAX_PENDING: usize = 32;
// Failed and pending entries also consume a slot: never more than 128
// sheets/keys.
pub(super) const MAX_ENTRIES: usize = 128;
const UNUSED_RETENTION: Duration = Duration::from_secs(60);
const DEFAULT_SOURCE: PlayerHeadProfileSource = PlayerHeadProfileSource::Default;
type Completion = (PlayerHeadProfileSource, Result<SkinData, String>);

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
}

impl PlacedHeadSkinCache {
    /// Load the same built-in slim Steve used by the skull pipeline, once at
    /// renderer startup, not during frame rendering or per-source admission.
    pub(super) fn load(jar: &Path, index: &Option<AssetIndex>) -> Result<Self, String> {
        let path = resolve_asset_path(
            jar,
            index,
            "minecraft/textures/entity/player/slim/steve.png",
        );
        let rgba = load_image(&path).map_err(|e| e.to_string())?.into_rgba8();
        let (width, height) = rgba.dimensions();
        let (pixels, width, height) = process_legacy_skin(rgba.into_raw(), width, height)?;
        Ok(Self::new(SkinData {
            pixels,
            width,
            height,
            slim: true,
        }))
    }

    fn new(fallback: SkinData) -> Self {
        let (tx, rx) = mpsc::sync_channel(MAX_PENDING);
        Self {
            entries: HashMap::new(),
            pending: 0,
            tx,
            rx,
            fallback,
        }
    }

    /// Called once before world drawing. No disk IO, HTTP, decode, waits, or
    /// worker creation on cache hits; even 1000 distinct heads admit at most
    /// 32 outstanding jobs (completed-but-undrained results still count).
    pub(super) fn update<'a>(
        &mut self,
        sources: impl Iterator<Item = &'a PlayerHeadProfileSource> + Clone,
        rt: &tokio::runtime::Runtime,
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
        self.entries.retain(|_, entry| {
            matches!(entry.state, State::Pending)
                || now.duration_since(entry.last_seen) < UNUSED_RETENTION
        });
        for source in sources {
            if !self.reserve(source, now) {
                continue;
            }
            let source = source.clone();
            let tx = self.tx.clone();
            rt.spawn(async move {
                let PlayerHeadProfileSource::EmbeddedTexturesProperty(value) = &source else {
                    unreachable!("only embedded properties are admitted");
                };
                let result = fetch_skin_texture_from_profile_property(value).await;
                // One result per reservation; channel capacity equals MAX_PENDING.
                // try_send never blocks a runtime worker (including on teardown).
                let _ = tx.try_send((source, result));
            });
        }
    }

    fn supported(source: &PlayerHeadProfileSource) -> bool {
        // Reject oversized input before hashing/cloning/decoding or spawning.
        // ResourceTexture and Profile deliberately remain fallback in this phase.
        matches!(source, PlayerHeadProfileSource::EmbeddedTexturesProperty(value)
            if value.len() <= super::MAX_TEXTURE_PROPERTY_BYTES)
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
        while let Ok((source, result)) = self.rx.try_recv() {
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
                    State::Ready(skin)
                }
                _ => State::Failed,
            };
        }
    }

    /// Only validated, decoded sheets can reach GPU upload. Unsupported sources
    /// (ResourceTexture and name/UUID Profile) never appear here.
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
        PlayerHeadProfileSource::EmbeddedTexturesProperty(value.into())
    }

    fn finish(
        cache: &mut PlacedHeadSkinCache,
        source: PlayerHeadProfileSource,
        result: Result<SkinData, String>,
    ) {
        assert!(cache.tx.try_send((source, result)).is_ok());
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
            PlayerHeadProfileSource::ResourceTexture("minecraft:custom.png".into()),
            PlayerHeadProfileSource::Profile("Steve".into()),
            PlayerHeadProfileSource::Profile("00000000-0000-0000-0000-000000000000".into()),
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
        // validated CPU sheet survives reload, but its old GPU set does not.
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
            PlayerHeadProfileSource::Profile("Steve".into()),
            PlayerHeadProfileSource::ResourceTexture("minecraft:textures/test.png".into()),
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
        cache.update(std::iter::once(&bad), &rt);
        // Finite wait in a test only; the frame path exclusively uses try_recv.
        let completion = cache.rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(completion.0, bad);
        assert!(completion.1.is_err());
        assert!(cache.tx.try_send(completion).is_ok());
        cache.update(std::iter::once(&bad), &rt);
        assert_eq!(cache.pending, 0);
        assert!(matches!(cache.entries[&bad].state, State::Failed));
        assert_eq!(cache.skin(&bad).0, &DEFAULT_SOURCE);
        assert!(cache.rx.try_recv().is_err());
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
        cache.update(std::iter::once(&visible), &rt);
        assert!(cache.entries.contains_key(&pending));
        assert!(cache.entries.contains_key(&visible));
        assert!(!cache.entries.contains_key(&absent));
        assert_eq!(cache.pending, 1);
        assert!(cache.reserve(&absent, Instant::now()));
    }
}
