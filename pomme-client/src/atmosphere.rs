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
    matches!(kind, ServerParticleKind::Cloud | ServerParticleKind::CopperFireFlame | ServerParticleKind::Flame | ServerParticleKind::SoulFireFlame | ServerParticleKind::SmallFlame | ServerParticleKind::LargeSmoke | ServerParticleKind::WhiteSmoke | ServerParticleKind::Sneeze | ServerParticleKind::Ash | ServerParticleKind::WhiteAsh | ServerParticleKind::CrimsonSpore | ServerParticleKind::WarpedSpore | ServerParticleKind::SporeBlossomAir | ServerParticleKind::FallingSporeBlossom | ServerParticleKind::Mycelium | ServerParticleKind::Underwater | ServerParticleKind::Firefly | ServerParticleKind::CherryLeaves | ServerParticleKind::PaleOakLeaves | ServerParticleKind::TintedLeaves | ServerParticleKind::AngryVillager | ServerParticleKind::HappyVillager | ServerParticleKind::Composter | ServerParticleKind::Heart | ServerParticleKind::Snowflake | ServerParticleKind::NoxiousGas | ServerParticleKind::SulfurCubeGoo | ServerParticleKind::DragonBreath | ServerParticleKind::Infested | ServerParticleKind::Soul | ServerParticleKind::SculkSoul)
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
