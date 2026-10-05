# Projectile underwater bubble trail follow-up (Java 26.2)

## Source contract

Java client sources in `minecraft-26.2-decompiled/src/net/minecraft/world/entity/projectile` emit four local `BUBBLE` particles per tick, all at `position - deltaMovement * 0.25` with the unchanged movement as velocity:

- `arrow/AbstractArrow.java`: after the in-ground early return, only when `isInWater()`; this applies to Arrow, SpectralArrow, and Trident.
- `hurtingprojectile/AbstractHurtingProjectile.java`: from `applyInertia`, only when `isInWater()`; Fireball, SmallFireball, DragonFireball, WitherSkull, WindCharge, and BreezeWindCharge share this base through `Fireball` or `AbstractWindCharge`.
- `ThrowableProjectile.java`: from `applyInertia`, only when `isInWater()`; Snowball, Egg, EnderPearl, ExperienceBottle, SplashPotion, and LingeringPotion use this path (potions inherit through `ThrowableItemProjectile`).
- `projectile/EyeOfEnder.java`: distinct owner emits four bubbles at `origin`, not the shared projectile offset. It remains in `particle_misc.rs` and must not be counted by the common base-family branch.

The common water-source set is **15 unique AddEntity kinds**, not 12: AbstractArrow has Arrow, SpectralArrow, Trident (3); AbstractHurtingProjectile has Fireball, SmallFireball, DragonFireball, WitherSkull, WindCharge, BreezeWindCharge (6); ThrowableProjectile has Snowball, Egg, EnderPearl, ExperienceBottle, SplashPotion, LingeringPotion (6). These are three inheritance-family counts, so 3+6+6=15. `EyeOfEnder` is a separate owner and a separate 16th entity kind for this bubble behavior. The other `12 projectile/thrown` figure in `docs/report-audit/nonliving-entity-plan.md` is a **different renderer inventory** (its list includes LlamaSpit/ShulkerBullet but not Arrow/SpectralArrow/Snowball/WindCharge/BreezeWindCharge) and must not be used as this water-source denominator.

There is no `deltaMovement != 0` condition. Arrow in-ground state suppresses its branch because Java returns earlier; generic stopped state is not a Java condition for these branches. Existing critical-arrow trail, Arrow color trail/pickup burst, SpectralArrow trail and other non-water projectile effects remain separate.

## Rust mapping and implementation

`EntityStore::client_particle_requests` owns AddEntity-tracked nonliving projectile visual ticks. The BUBBLE branch recognizes exactly those **15 kinds** above. It uses the existing `particle_tick::touches_water` bbox/fluid query (`EntityFluidInteraction.update` shape: deflated bounds, actual fluid surface heights, fail closed for unloaded chunks) rather than sampling only the entity center block. Arrow-family BUBBLE requires `!in_ground`; no velocity or generic `stopped` guard is applied. Every qualifying branch emits four requests with Java's position/velocity values.

`EyeOfEnder` continues to be owned by `particle_misc::tick`, which emits exactly four separate bubbles. AddEntity already stores all these entity kinds in `EntityStore::vehicles`; removal uses the existing `remove_entity` lifecycle and requires no new tracking structure.

## Automated checks and latest run

The aggregate test `entity::tests::projectile_water_bubbles_match_java_families_and_lifecycle` covers the **15 unique kinds** from the three Java bases: AbstractArrow (3), AbstractHurtingProjectile (6) and ThrowableProjectile (6), including concrete hurting-projectile subclasses and all thrown-item subclasses. It checks water four-particle cadence and exact motion, dry-air zero, zero velocity, in-ground Arrow suppression and remove/re-add. Its assertions select the Java per-projectile position/velocity signature separately from the independent common Entity water-entry splash bubbles.

From `Client/`, `mise exec -- cargo test -p pomme-client --locked --profile dev-fast projectile_water_bubbles_match_java_families_and_lifecycle -- --test-threads=1` — exit **0**, **1 passed**. `eye_of_ender_keeps_its_single_four_bubble_owner` — exit **0**, **1 passed**, confirming EyeOfEnder remains separately counted, exactly four bubbles. These are automated source-level checks, not GUI visual parity verification.
