//! Family-owned integration point for Java particle provider behavior.
//!
//! Implementations belong here, not in the shared particle dispatcher.
use super::{
    Appearance, ChunkStore, Particle, ParticleStore, ServerParticleKind, ServerParticleOptions,
};
use crate::renderer::chunk::atlas::AtlasUVMap;
use crate::renderer::chunk::mesher::BiomeClimate;
use crate::world::block::registry::BlockRegistry;
use glam::DVec3;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct State {
    pub(super) kind: ServerParticleKind,
}

pub(super) fn supports(kind: ServerParticleKind) -> bool {
    matches!(kind, ServerParticleKind::Crit | ServerParticleKind::EnchantedHit | ServerParticleKind::DamageIndicator | ServerParticleKind::Enchant | ServerParticleKind::Note | ServerParticleKind::Portal | ServerParticleKind::ReversePortal | ServerParticleKind::Glow | ServerParticleKind::WaxOn | ServerParticleKind::WaxOff | ServerParticleKind::ElectricSpark | ServerParticleKind::Scrape | ServerParticleKind::EggCrack | ServerParticleKind::DustPlume | ServerParticleKind::TrialSpawnerDetection | ServerParticleKind::TrialSpawnerDetectionOminous | ServerParticleKind::VaultConnection | ServerParticleKind::OminousSpawning | ServerParticleKind::PauseMobGrowth | ServerParticleKind::ResetMobGrowth | ServerParticleKind::Firework | ServerParticleKind::Flash | ServerParticleKind::SculkCharge | ServerParticleKind::SculkChargePop | ServerParticleKind::SonicBoom | ServerParticleKind::SweepAttack | ServerParticleKind::Gust | ServerParticleKind::SmallGust)
}

/// Return true only after this provider has actually spawned its particle.
pub(super) fn spawn(
    _store: &mut ParticleStore,
    _kind: ServerParticleKind,
    _options: ServerParticleOptions,
    _pos: DVec3,
    _vel: DVec3,
    _registry: &BlockRegistry,
    _chunks: &ChunkStore,
    _biome_climate: &HashMap<u32, BiomeClimate>,
) -> bool {
    false
}

/// Own the complete update, including previous position, age, lifetime and physics.
pub(super) fn tick(
    _particle: &mut Particle,
    _chunks: &ChunkStore,
    _atlas: &AtlasUVMap,
    _children: &mut Vec<Particle>,
) -> bool {
    true
}

pub(super) fn appearance(_particle: &Particle, _partial_tick: f32) -> Option<Appearance> {
    None
}
