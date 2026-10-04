use azalea_block::BlockState;
use azalea_buf::AzBuf;
use azalea_core::color::RgbColor;
use azalea_core::entity_id::MinecraftEntityId;
use azalea_core::position::BlockPos;
use azalea_inventory::ItemStack;
use azalea_registry::builtin::ParticleKind;

// the order of this enum must be kept in sync with ParticleKind, otherwise
// we get errors parsing particles.

/// A [`ParticleKind`] with data potentially attached to it.
#[cfg_attr(feature = "bevy_ecs", derive(bevy_ecs::component::Component))]
#[derive(AzBuf, Clone, Debug, PartialEq)]
pub enum Particle {
    AngryVillager,
    Block(BlockParticle),
    BlockMarker(BlockParticle),
    Bubble,
    SulfurBubbles,
    NoxiousGas,
    NoxiousGasCloud,
    Geyser(GeyserParticle),
    GeyserBase(GeyserBaseParticle),
    GeyserPoof(GeyserBaseParticle),
    GeyserPlume(GeyserParticle),
    Cloud,
    CopperFireFlame,
    Crit,
    DamageIndicator,
    DragonBreath(PowerParticle),
    DrippingLava,
    FallingLava,
    LandingLava,
    DrippingWater,
    FallingWater,
    Dust(DustParticle),
    DustColorTransition(DustColorTransitionParticle),
    Effect(ColorPowerParticle),
    ElderGuardian,
    EnchantedHit,
    Enchant,
    EndRod,
    EntityEffect(ColorParticle),
    ExplosionEmitter,
    Explosion,
    Gust,
    SmallGust,
    GustEmitterLarge,
    GustEmitterSmall,
    SonicBoom,
    FallingDust(BlockParticle),
    Firework,
    Fishing,
    Flame,
    Infested,
    CherryLeaves,
    PaleOakLeaves,
    TintedLeaves(ColorParticle),
    SculkSoul,
    SculkCharge(SculkChargeParticle),
    SculkChargePop,
    SoulFireFlame,
    Soul,
    Flash(ColorParticle),
    HappyVillager,
    Composter,
    Heart,
    InstantEffect(ColorPowerParticle),
    Item(ItemParticle),
    Vibration(Box<VibrationParticle>),
    Trail(Box<TrailParticle>),
    PauseMobGrowth,
    ResetMobGrowth,
    ItemSlime,
    ItemCobweb,
    ItemSnowball,
    LargeSmoke,
    Lava,
    Mycelium,
    Note,
    Poof,
    Portal,
    Rain,
    Smoke,
    WhiteSmoke,
    Sneeze,
    Spit,
    SquidInk,
    SweepAttack,
    TotemOfUndying,
    Underwater,
    Splash,
    Witch,
    BubblePop,
    CurrentDown,
    BubbleColumnUp,
    Nautilus,
    Dolphin,
    CampfireCosySmoke,
    CampfireSignalSmoke,
    DrippingHoney,
    FallingHoney,
    LandingHoney,
    FallingNectar,
    FallingSporeBlossom,
    Ash,
    CrimsonSpore,
    WarpedSpore,
    SporeBlossomAir,
    DrippingObsidianTear,
    FallingObsidianTear,
    LandingObsidianTear,
    ReversePortal,
    WhiteAsh,
    SmallFlame,
    Snowflake,
    DrippingDripstoneLava,
    FallingDripstoneLava,
    DrippingDripstoneWater,
    FallingDripstoneWater,
    GlowSquidInk,
    Glow,
    WaxOn,
    WaxOff,
    ElectricSpark,
    Scrape,
    Shriek(ShriekParticle),
    EggCrack,
    DustPlume,
    TrialSpawnerDetection,
    TrialSpawnerDetectionOminous,
    VaultConnection,
    DustPillar(BlockParticle),
    OminousSpawning,
    RaidOmen,
    TrialOmen,
    BlockCrumble(BlockParticle),
    Firefly,
    SulfurCubeGoo,
}

impl From<ParticleKind> for Particle {
    /// Convert a particle kind into particle data. Payload variants default to
    /// empty/default payloads.
    fn from(kind: ParticleKind) -> Self {
        match kind {
            ParticleKind::AngryVillager => Self::AngryVillager,
            ParticleKind::Block => Self::Block(BlockParticle::default()),
            ParticleKind::BlockMarker => Self::BlockMarker(BlockParticle::default()),
            ParticleKind::Bubble => Self::Bubble,
            ParticleKind::Cloud => Self::Cloud,
            ParticleKind::Crit => Self::Crit,
            ParticleKind::DamageIndicator => Self::DamageIndicator,
            ParticleKind::DragonBreath => Self::DragonBreath(PowerParticle::default()),
            ParticleKind::DrippingLava => Self::DrippingLava,
            ParticleKind::FallingLava => Self::FallingLava,
            ParticleKind::LandingLava => Self::LandingLava,
            ParticleKind::DrippingWater => Self::DrippingWater,
            ParticleKind::FallingWater => Self::FallingWater,
            ParticleKind::Dust => Self::Dust(DustParticle::default()),
            ParticleKind::DustColorTransition => {
                Self::DustColorTransition(DustColorTransitionParticle::default())
            }
            ParticleKind::Effect => Self::Effect(ColorPowerParticle::default()),
            ParticleKind::ElderGuardian => Self::ElderGuardian,
            ParticleKind::EnchantedHit => Self::EnchantedHit,
            ParticleKind::Enchant => Self::Enchant,
            ParticleKind::EndRod => Self::EndRod,
            ParticleKind::EntityEffect => Self::EntityEffect(ColorParticle::default()),
            ParticleKind::ExplosionEmitter => Self::ExplosionEmitter,
            ParticleKind::Explosion => Self::Explosion,
            ParticleKind::Gust => Self::Gust,
            ParticleKind::SonicBoom => Self::SonicBoom,
            ParticleKind::FallingDust => Self::FallingDust(BlockParticle::default()),
            ParticleKind::Firework => Self::Firework,
            ParticleKind::Fishing => Self::Fishing,
            ParticleKind::Flame => Self::Flame,
            ParticleKind::CherryLeaves => Self::CherryLeaves,
            ParticleKind::PaleOakLeaves => Self::PaleOakLeaves,
            ParticleKind::TintedLeaves => Self::TintedLeaves(ColorParticle::default()),
            ParticleKind::SculkSoul => Self::SculkSoul,
            ParticleKind::SculkCharge => Self::SculkCharge(SculkChargeParticle::default()),
            ParticleKind::SculkChargePop => Self::SculkChargePop,
            ParticleKind::SoulFireFlame => Self::SoulFireFlame,
            ParticleKind::Soul => Self::Soul,
            ParticleKind::Flash => Self::Flash(ColorParticle::default()),
            ParticleKind::HappyVillager => Self::HappyVillager,
            ParticleKind::Composter => Self::Composter,
            ParticleKind::Heart => Self::Heart,
            ParticleKind::InstantEffect => Self::InstantEffect(ColorPowerParticle::default()),
            ParticleKind::Item => Self::Item(ItemParticle::default()),
            ParticleKind::Vibration => Self::Vibration(Default::default()),
            ParticleKind::ItemSlime => Self::ItemSlime,
            ParticleKind::ItemSnowball => Self::ItemSnowball,
            ParticleKind::LargeSmoke => Self::LargeSmoke,
            ParticleKind::Lava => Self::Lava,
            ParticleKind::Mycelium => Self::Mycelium,
            ParticleKind::Note => Self::Note,
            ParticleKind::Poof => Self::Poof,
            ParticleKind::Portal => Self::Portal,
            ParticleKind::Rain => Self::Rain,
            ParticleKind::Smoke => Self::Smoke,
            ParticleKind::WhiteSmoke => Self::WhiteSmoke,
            ParticleKind::Sneeze => Self::Sneeze,
            ParticleKind::Spit => Self::Spit,
            ParticleKind::SquidInk => Self::SquidInk,
            ParticleKind::SweepAttack => Self::SweepAttack,
            ParticleKind::TotemOfUndying => Self::TotemOfUndying,
            ParticleKind::Underwater => Self::Underwater,
            ParticleKind::Splash => Self::Splash,
            ParticleKind::Witch => Self::Witch,
            ParticleKind::BubblePop => Self::BubblePop,
            ParticleKind::CurrentDown => Self::CurrentDown,
            ParticleKind::BubbleColumnUp => Self::BubbleColumnUp,
            ParticleKind::Nautilus => Self::Nautilus,
            ParticleKind::Dolphin => Self::Dolphin,
            ParticleKind::CampfireCosySmoke => Self::CampfireCosySmoke,
            ParticleKind::CampfireSignalSmoke => Self::CampfireSignalSmoke,
            ParticleKind::DrippingHoney => Self::DrippingHoney,
            ParticleKind::FallingHoney => Self::FallingHoney,
            ParticleKind::LandingHoney => Self::LandingHoney,
            ParticleKind::FallingNectar => Self::FallingNectar,
            ParticleKind::FallingSporeBlossom => Self::FallingSporeBlossom,
            ParticleKind::Ash => Self::Ash,
            ParticleKind::CrimsonSpore => Self::CrimsonSpore,
            ParticleKind::WarpedSpore => Self::WarpedSpore,
            ParticleKind::SporeBlossomAir => Self::SporeBlossomAir,
            ParticleKind::DrippingObsidianTear => Self::DrippingObsidianTear,
            ParticleKind::FallingObsidianTear => Self::FallingObsidianTear,
            ParticleKind::LandingObsidianTear => Self::LandingObsidianTear,
            ParticleKind::ReversePortal => Self::ReversePortal,
            ParticleKind::WhiteAsh => Self::WhiteAsh,
            ParticleKind::SmallFlame => Self::SmallFlame,
            ParticleKind::Snowflake => Self::Snowflake,
            ParticleKind::DrippingDripstoneLava => Self::DrippingDripstoneLava,
            ParticleKind::FallingDripstoneLava => Self::FallingDripstoneLava,
            ParticleKind::DrippingDripstoneWater => Self::DrippingDripstoneWater,
            ParticleKind::FallingDripstoneWater => Self::FallingDripstoneWater,
            ParticleKind::GlowSquidInk => Self::GlowSquidInk,
            ParticleKind::Glow => Self::Glow,
            ParticleKind::WaxOn => Self::WaxOn,
            ParticleKind::WaxOff => Self::WaxOff,
            ParticleKind::ElectricSpark => Self::ElectricSpark,
            ParticleKind::Scrape => Self::Scrape,
            ParticleKind::Shriek => Self::Shriek(ShriekParticle::default()),
            ParticleKind::EggCrack => Self::EggCrack,
            ParticleKind::DustPlume => Self::DustPlume,
            ParticleKind::SmallGust => Self::SmallGust,
            ParticleKind::GustEmitterLarge => Self::GustEmitterLarge,
            ParticleKind::GustEmitterSmall => Self::GustEmitterSmall,
            ParticleKind::Infested => Self::Infested,
            ParticleKind::ItemCobweb => Self::ItemCobweb,
            ParticleKind::TrialSpawnerDetection => Self::TrialSpawnerDetection,
            ParticleKind::TrialSpawnerDetectionOminous => Self::TrialSpawnerDetectionOminous,
            ParticleKind::VaultConnection => Self::VaultConnection,
            ParticleKind::DustPillar => Self::DustPillar(BlockParticle::default()),
            ParticleKind::OminousSpawning => Self::OminousSpawning,
            ParticleKind::RaidOmen => Self::RaidOmen,
            ParticleKind::TrialOmen => Self::TrialOmen,
            ParticleKind::Trail => Self::Trail(Box::default()),
            ParticleKind::BlockCrumble => Self::BlockCrumble(BlockParticle::default()),
            ParticleKind::Firefly => Self::Firefly,
            ParticleKind::CopperFireFlame => Self::CopperFireFlame,
            ParticleKind::PauseMobGrowth => Self::PauseMobGrowth,
            ParticleKind::ResetMobGrowth => Self::ResetMobGrowth,
            ParticleKind::SulfurBubbles => Self::SulfurBubbles,
            ParticleKind::NoxiousGas => Self::NoxiousGas,
            ParticleKind::NoxiousGasCloud => Self::NoxiousGasCloud,
            ParticleKind::Geyser => Self::Geyser(GeyserParticle::default()),
            ParticleKind::GeyserBase => Self::GeyserBase(GeyserBaseParticle::default()),
            ParticleKind::GeyserPoof => Self::GeyserPoof(GeyserBaseParticle::default()),
            ParticleKind::GeyserPlume => Self::GeyserPlume(GeyserParticle::default()),
            ParticleKind::SulfurCubeGoo => Self::SulfurCubeGoo,
        }
    }
}

impl Default for Particle {
    fn default() -> Self {
        Self::EntityEffect(ColorParticle::default())
    }
}

#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct BlockParticle {
    pub block_state: BlockState,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct DustParticle {
    pub color: RgbColor,
    pub scale: f32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct DustColorTransitionParticle {
    pub from: RgbColor,
    pub to: RgbColor,
    pub scale: f32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct ColorParticle {
    pub color: RgbColor,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct PowerParticle {
    pub power: f32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct TrailParticle {
    pub target: azalea_core::position::Vec3,
    pub color: i32,
    #[var]
    pub duration: u32,
}
#[derive(AzBuf, Clone, Debug, PartialEq)]
pub struct ColorPowerParticle {
    pub color: i32,
    pub power: f32,
}
impl Default for ColorPowerParticle {
    fn default() -> Self {
        Self {
            color: -1,
            power: 1.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ItemParticle {
    pub item: ItemStack,
}
impl AzBuf for ItemParticle {
    fn azalea_read(buf: &mut std::io::Cursor<&[u8]>) -> Result<Self, azalea_buf::BufReadError> {
        use azalea_buf::AzBufVar;
        use azalea_registry::builtin::ItemKind;
        let kind = ItemKind::azalea_read(buf)?;
        let count = i32::azalea_read_var(buf)?;
        let component_patch = azalea_inventory::DataComponentPatch::azalea_read(buf)?;
        if kind == ItemKind::Air || count == 0 {
            return Err(azalea_buf::BufReadError::Io {
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "item particle requires a non-empty item template",
                ),
            });
        }
        Ok(Self {
            item: ItemStack::Present(azalea_inventory::ItemStackData {
                kind,
                count,
                component_patch,
            }),
        })
    }
    fn azalea_write(&self, buf: &mut impl std::io::Write) -> std::io::Result<()> {
        use azalea_buf::AzBufVar;
        let ItemStack::Present(item) = &self.item else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "item particle requires a non-empty item stack",
            ));
        };
        if item.kind == azalea_registry::builtin::ItemKind::Air || item.count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "item particle requires a non-empty item template",
            ));
        }
        item.kind.azalea_write(buf)?;
        item.count.azalea_write_var(buf)?;
        item.component_patch.azalea_write(buf)
    }
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct VibrationParticle {
    pub position: PositionSource,
    #[var]
    pub ticks: u32,
}
#[derive(AzBuf, Clone, Debug, PartialEq)]
pub enum PositionSource {
    Block(BlockPos),
    Entity {
        #[var]
        id: MinecraftEntityId,
        y_offset: f32,
    },
}
impl Default for PositionSource {
    fn default() -> Self {
        Self::Block(BlockPos::default())
    }
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct SculkChargeParticle {
    pub roll: f32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct ShriekParticle {
    #[var]
    pub delay: i32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct GeyserParticle {
    pub water_blocks: i32,
}
#[derive(AzBuf, Clone, Debug, Default, PartialEq)]
pub struct GeyserBaseParticle {
    pub water_blocks: i32,
    pub burst_impulse_base: f32,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use azalea_registry::builtin::ItemKind;

    use super::{AzBuf, BlockParticle, ColorParticle, ItemParticle, Particle};

    #[test]
    fn native_color_and_block_payloads_leave_following_metadata_aligned() {
        let tinted_leaves_fixture = [43, 0x12, 0x34, 0x56, 0x78, 1];
        let mut input = Cursor::new(tinted_leaves_fixture.as_slice());
        let decoded = Particle::azalea_read(&mut input).unwrap();
        let Particle::TintedLeaves(ColorParticle { color }) = decoded else {
            panic!("expected tinted leaves color payload");
        };
        assert_eq!(
            (color.red(), color.green(), color.blue()),
            (0x34, 0x56, 0x78)
        );
        assert_eq!(input.position(), 5);
        assert!(bool::azalea_read(&mut input).unwrap());
        assert_eq!(input.position(), 6);

        for (id, fixture) in [(118, &[118, 0, 1][..]), (122, &[122, 0, 1][..])] {
            let mut input = Cursor::new(fixture);
            let decoded = Particle::azalea_read(&mut input).unwrap();
            match (id, decoded) {
                (118, Particle::DustPillar(BlockParticle { block_state }))
                | (122, Particle::BlockCrumble(BlockParticle { block_state })) => {
                    assert_eq!(block_state, Default::default());
                }
                _ => panic!("wrong block payload variant for particle {id}"),
            }
            assert_eq!(input.position(), 2);
            assert!(bool::azalea_read(&mut input).unwrap());
            assert_eq!(input.position(), 3);
        }
    }

    #[test]
    fn item_particle_uses_native_template_item_then_count_order() {
        let fixture = [1, 2, 0, 0, 0x5a];
        let mut input = Cursor::new(fixture.as_slice());
        let particle = ItemParticle::azalea_read(&mut input).unwrap();
        assert_eq!(input.position(), 4);
        assert_eq!(input.get_ref()[input.position() as usize], 0x5a);
        assert_eq!(particle.item.kind(), ItemKind::Stone);
        assert_eq!(particle.item.count(), 2);
        let mut output = Vec::new();
        particle.azalea_write(&mut output).unwrap();
        assert_eq!(output, fixture[..4]);

        let legacy = [2, 1, 0, 0];
        let mut input = Cursor::new(legacy.as_slice());
        let decoded = ItemParticle::azalea_read(&mut input).unwrap();
        assert_ne!(decoded.item.kind(), ItemKind::Stone);
        assert_ne!(decoded.item.count(), 2);
    }

    #[test]
    fn item_particle_rejects_empty_templates_on_read_and_write() {
        use azalea_buf::AzBufVar;

        for fixture in [
            &[0, 0, 0, 0][..], // Air/count 0
            &[0, 1, 0, 0][..], // Air/count 1
            &[1, 0, 0, 0][..], // Stone/count 0
        ] {
            assert!(ItemParticle::azalea_read(&mut Cursor::new(fixture)).is_err());
        }
        let mut negative = Vec::new();
        ItemKind::Stone.azalea_write(&mut negative).unwrap();
        (-1i32).azalea_write_var(&mut negative).unwrap();
        negative.extend_from_slice(&[0, 0]);
        assert!(ItemParticle::azalea_read(&mut Cursor::new(negative.as_slice())).is_ok());

        let mut ordinary_empty = Vec::new();
        azalea_inventory::ItemStack::default()
            .azalea_write(&mut ordinary_empty)
            .unwrap();
        assert_eq!(ordinary_empty, [0]);

        let mut output = Vec::new();
        assert_eq!(
            ItemParticle::default()
                .azalea_write(&mut output)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );

        // These non-empty stack values are legal as ordinary ItemStacks but
        // invalid as the required payload for an item particle.
        for (kind, count) in [(ItemKind::Air, 1), (ItemKind::Stone, 0)] {
            let mut particle = ItemParticle::azalea_read(&mut Cursor::new(&[1, 2, 0, 0])).unwrap();
            let azalea_inventory::ItemStack::Present(item) = &mut particle.item else {
                unreachable!();
            };
            item.kind = kind;
            item.count = count;
            let mut output = Vec::new();
            assert_eq!(
                particle.azalea_write(&mut output).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput,
                "{kind:?}/{count}"
            );
        }
    }
}
