//! Typed ingress for the dimension environment inputs consumed by the renderer.
use std::sync::Arc;

use simdnbt::owned::{NbtCompound, NbtList};

pub const SKY_LIGHT_LEVEL: f32 = 15.0;
pub const SKY_LIGHT_LEVEL_ATTRIBUTE: &str = "minecraft:gameplay/sky_light_level";
pub const BLOCK_LIGHT_TINT_ATTRIBUTE: &str = "minecraft:visual/block_light_tint";
pub const SKY_LIGHT_COLOR_ATTRIBUTE: &str = "minecraft:visual/sky_light_color";
pub const SKY_LIGHT_FACTOR_ATTRIBUTE: &str = "minecraft:visual/sky_light_factor";
pub const AMBIENT_LIGHT_COLOR_ATTRIBUTE: &str = "minecraft:visual/ambient_light_color";
pub const NIGHT_VISION_COLOR_ATTRIBUTE: &str = "minecraft:visual/night_vision_color";
pub const MAX_TIMELINES: usize = 4096;
const MAX_KEYFRAMES: usize = 4096;

pub fn rgb_attribute_value(tag: &simdnbt::owned::NbtTag) -> Option<i32> {
    match tag {
        simdnbt::owned::NbtTag::Int(value) => Some(*value),
        simdnbt::owned::NbtTag::String(value) => {
            let text = value.to_str();
            let digits = text.strip_prefix('#').unwrap_or(&text);
            (digits.len() == 6)
                .then(|| i32::from_str_radix(digits, 16).ok())
                .flatten()
        }
        _ => None,
    }
}

pub type TimelineEntries = Arc<Vec<(String, NbtCompound)>>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LightmapAttributes {
    pub sky_light_factor: Option<f32>,
    pub block_light_tint: Option<i32>,
    pub sky_light_color: Option<i32>,
    pub ambient_light_color: Option<i32>,
    pub night_vision_color: Option<i32>,
}

#[derive(Clone, Debug)]
pub struct DimensionEnvironmentInput {
    pub has_sky_light: bool,
    pub has_ceiling: bool,
    pub is_end_world: bool,
    pub has_end_flashes: bool,
    pub ambient_light: Option<f32>,
    pub sky_light_level: Option<f32>,
    pub lightmap_attributes: LightmapAttributes,
    pub water_evaporates: Option<bool>,
    pub default_dripstone_particle: Option<crate::world::environment_particles::AmbientParticle>,
    pub timeline_refs: Vec<String>,
    pub timeline_entries: TimelineEntries,
    pub timeline_entries_error: Option<String>,
}

pub fn resolve_dimension_environment(
    input: &DimensionEnvironmentInput,
    tags: &std::collections::HashMap<
        azalea_registry::identifier::Identifier,
        Vec<azalea_registry::identifier::Identifier>,
    >,
) -> Result<DimensionEnvironment, String> {
    if !input.timeline_refs.is_empty() {
        if let Some(error) = &input.timeline_entries_error {
            return Err(error.clone());
        }
    }
    let mut ids = Vec::new();
    for reference in &input.timeline_refs {
        if let Some(tag) = reference.strip_prefix('#') {
            let key = tag
                .parse::<azalea_registry::identifier::Identifier>()
                .map_err(|_| format!("invalid timeline tag reference: {reference}"))?;
            let members = tags
                .get(&key)
                .ok_or_else(|| format!("missing timeline tag: #{key}"))?;
            ids.extend(members.iter().map(ToString::to_string));
        } else {
            ids.push(reference.clone());
        }
        if ids.len() > MAX_TIMELINES {
            return Err("too many dimension timelines".into());
        }
    }
    let mut environment = from_dimension_fields(
        input.has_sky_light,
        input.has_ceiling,
        input.is_end_world,
        input.ambient_light,
        input.sky_light_level,
        input.timeline_entries.as_slice(),
        &ids,
    )?;
    environment.lightmap_attributes = input.lightmap_attributes;
    environment.has_end_flashes = input.has_end_flashes;
    environment.water_evaporates = input.water_evaporates.unwrap_or(false);
    if let Some(particle) = &input.default_dripstone_particle {
        environment.default_dripstone_particle = particle.clone();
    }
    Ok(environment)
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloatKeyframe {
    pub ticks: i32,
    pub value: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FloatTrack {
    pub period_ticks: Option<i32>,
    pub easing: String,
    pub modifier: Option<String>,
    pub keyframes: Vec<FloatKeyframe>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorKeyframe {
    pub ticks: i32,
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorTrack {
    pub period_ticks: Option<i32>,
    pub easing: String,
    pub modifier: Option<String>,
    pub keyframes: Vec<ColorKeyframe>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightmapTimeline {
    pub id: String,
    pub clock: String,
    pub period_ticks: Option<i32>,
    pub sky_light_factor: Option<FloatTrack>,
    pub sky_light_color: Option<ColorTrack>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightmapFrameAttributes {
    pub sky_light_factor: f32,
    pub block_light_tint: i32,
    pub sky_light_color: i32,
    pub ambient_light_color: i32,
    pub night_vision_color: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimelineInput {
    pub id: String,
    pub clock: String,
    pub tracks: Vec<(String, FloatTrack)>,
}

#[derive(Clone, Debug)]
pub struct AmbientParticleKeyframe {
    pub ticks: i32,
    pub value: Vec<crate::world::environment_particles::AmbientParticle>,
}

#[derive(Clone, Debug)]
pub struct AmbientParticleTimeline {
    pub id: String,
    pub clock: String,
    pub period_ticks: Option<i32>,
    pub easing: AmbientEasing,
    pub keyframes: Vec<AmbientParticleKeyframe>,
}

#[derive(Clone, Debug)]
pub enum AmbientEasing {
    Simple(String),
    CubicBezier { x1: f32, y1: f32, x2: f32, y2: f32 },
}

#[derive(Clone, Debug)]
pub struct DimensionEnvironment {
    pub has_weather: bool,
    /// Current vanilla rain/thunder intensities supplied by the caller.
    pub rain_level: f32,
    pub thunder_level: f32,
    pub ambient_light: f32,
    pub sky_light_level: f32,
    pub lightmap_attributes: LightmapAttributes,
    pub has_end_flashes: bool,
    pub lightmap_timelines: Vec<LightmapTimeline>,
    pub lightmap_track_errors: Vec<String>,
    pub water_evaporates: bool,
    pub default_dripstone_particle: crate::world::environment_particles::AmbientParticle,
    pub water_evaporates_timelines: Vec<BoolTimeline>,
    pub default_dripstone_particle_timelines: Vec<ParticleTimeline>,
    pub environment_attribute_errors: Vec<String>,
    /// Nonempty when native environment data could not be represented. Safe
    /// fallback values remain usable, but are not claimed as valid ingress.
    pub unsupported_reason: Option<String>,
    pub timelines: Vec<String>,
    pub tracks: Vec<TimelineInput>,
    pub ambient_particle_tracks: Vec<AmbientParticleTimeline>,
    pub ambient_particle_track_errors: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockSample {
    pub total_ticks: i64,
    pub partial_tick: f32,
    pub rate: f32,
}

impl ClockSample {
    /// Mirrors ClientClockManager.tick(gameTimeDelta): Java floors to a
    /// saturating int before adding that integer to its wrapping long total.
    pub fn advance_game_time(&mut self, game_time_delta: i64) {
        let elapsed = f64::from(self.partial_tick) + game_time_delta as f64 * f64::from(self.rate);
        if !elapsed.is_finite() {
            return;
        }
        let full_ticks = elapsed.floor() as i32;
        self.partial_tick = (elapsed - f64::from(full_ticks)) as f32;
        self.total_ticks = self.total_ticks.wrapping_add(i64::from(full_ticks));
    }

    /// Native timeline sampling is integer-only; rendering interpolation is
    /// deliberately separate so it cannot perturb timeline phase selection.
    pub fn timeline_ticks(self, period_ticks: Option<i32>) -> i64 {
        period_ticks.map_or(self.total_ticks, |period| {
            self.total_ticks.rem_euclid(i64::from(period))
        })
    }

    pub fn renderer_tick(self, render_partial_tick: f32) -> f64 {
        self.total_ticks as f64
            + f64::from(self.partial_tick)
            + f64::from(render_partial_tick) * f64::from(self.rate)
    }
}

impl Default for DimensionEnvironment {
    fn default() -> Self {
        Self {
            has_weather: true,
            rain_level: 0.0,
            thunder_level: 0.0,
            ambient_light: 0.0,
            sky_light_level: SKY_LIGHT_LEVEL,
            lightmap_attributes: LightmapAttributes::default(),
            has_end_flashes: false,
            lightmap_timelines: Vec::new(),
            lightmap_track_errors: Vec::new(),
            water_evaporates: false,
            default_dripstone_particle: crate::world::environment_particles::AmbientParticle {
                kind: crate::particle::ServerParticleKind::DrippingDripstoneWater,
                options: crate::particle::ServerParticleOptions::Simple,
                probability: 1.0,
            },
            water_evaporates_timelines: Vec::new(),
            default_dripstone_particle_timelines: Vec::new(),
            environment_attribute_errors: Vec::new(),
            unsupported_reason: None,
            timelines: Vec::new(),
            tracks: Vec::new(),
            ambient_particle_tracks: Vec::new(),
            ambient_particle_track_errors: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyLightEvaluation {
    pub sky_light_level: f32,
    pub sky_darken: u8,
    pub ambient_light: f32,
}

/// Shared positional query used by block animate-ticks and level event 1504.
pub fn dripstone_attributes_at(
    chunks: &crate::world::chunk::ChunkStore,
    pos: azalea_core::position::BlockPos,
    environment: &DimensionEnvironment,
    biome_water: &std::collections::HashMap<u32, BoolAttributeLayer>,
    biome_particles: &std::collections::HashMap<
        u32,
        crate::world::environment_particles::AmbientParticle,
    >,
    clock_ticks: impl FnMut(&str, &str, Option<i32>) -> Result<i64, String>,
) -> Result<(bool, crate::world::environment_particles::AmbientParticle), String> {
    let biome = chunks.biome_id_checked(pos.x, pos.y, pos.z).map(u32::from);
    evaluate_dripstone_attributes(
        environment,
        biome.and_then(|id| biome_water.get(&id)),
        biome.and_then(|id| biome_particles.get(&id)),
        clock_ticks,
    )
}

/// Resolve positional biome layers followed by timeline layers, matching the
/// Java EnvironmentAttributeSystem insertion order for these non-interpolated
/// values.
pub fn evaluate_dripstone_attributes(
    environment: &DimensionEnvironment,
    biome_water_evaporates: Option<&BoolAttributeLayer>,
    biome_particle: Option<&crate::world::environment_particles::AmbientParticle>,
    mut clock_ticks: impl FnMut(&str, &str, Option<i32>) -> Result<i64, String>,
) -> Result<(bool, crate::world::environment_particles::AmbientParticle), String> {
    if let Some(error) = environment.environment_attribute_errors.first() {
        return Err(error.clone());
    }
    let mut evaporates = biome_water_evaporates.map_or(environment.water_evaporates, |layer| {
        layer.apply(environment.water_evaporates)
    });
    let mut particle = biome_particle
        .cloned()
        .unwrap_or_else(|| environment.default_dripstone_particle.clone());
    for track in &environment.water_evaporates_timelines {
        let ticks = clock_ticks(&track.id, &track.clock, track.period_ticks)?;
        let tick = track
            .period_ticks
            .map_or(ticks, |p| ticks.rem_euclid(i64::from(p)));
        let value = track
            .keyframes
            .iter()
            .take_while(|(at, _)| i64::from(*at) <= tick)
            .last()
            .or_else(|| {
                Some(if track.period_ticks.is_some() {
                    track.keyframes.last()?
                } else {
                    track.keyframes.first()?
                })
            })
            .map(|(_, v)| *v)
            .unwrap_or(evaporates);
        evaporates = match track.modifier.as_str() {
            "override" => value,
            "and" => evaporates & value,
            "nand" => !(evaporates & value),
            "or" => evaporates | value,
            "nor" => !(evaporates | value),
            "xor" => evaporates ^ value,
            "xnor" => !(evaporates ^ value),
            other => return Err(format!("unsupported WATER_EVAPORATES modifier: {other}")),
        };
    }
    for track in &environment.default_dripstone_particle_timelines {
        let ticks = clock_ticks(&track.id, &track.clock, track.period_ticks)?;
        let tick = track
            .period_ticks
            .map_or(ticks, |p| ticks.rem_euclid(i64::from(p)));
        if let Some((_, value)) = track
            .keyframes
            .iter()
            .take_while(|(at, _)| i64::from(*at) <= tick)
            .last()
            .or_else(|| {
                Some(if track.period_ticks.is_some() {
                    track.keyframes.last()?
                } else {
                    track.keyframes.first()?
                })
            })
        {
            particle = value.clone();
        }
    }
    Ok((evaporates, particle))
}

/// Evaluate the gameplay sky-light attribute without advancing or mutating
/// clocks.
pub fn evaluate_sky_light(
    environment: &DimensionEnvironment,
    mut clock_ticks: impl FnMut(&TimelineInput) -> Result<i64, String>,
) -> Result<SkyLightEvaluation, String> {
    if let Some(reason) = &environment.unsupported_reason {
        if !reason.is_empty() {
            return Err(reason.clone());
        }
    }
    if !environment.ambient_light.is_finite() || !(0.0..=1.0).contains(&environment.ambient_light) {
        return Err("ambient_light outside finite range 0..=1".into());
    }
    if !environment.sky_light_level.is_finite()
        || !(0.0..=15.0).contains(&environment.sky_light_level)
    {
        return Err("SKY_LIGHT_LEVEL outside finite range 0..=15".into());
    }
    if !environment.rain_level.is_finite()
        || !environment.thunder_level.is_finite()
        || !(0.0..=1.0).contains(&environment.rain_level)
        || !(0.0..=1.0).contains(&environment.thunder_level)
    {
        return Err("weather levels outside finite range 0..=1".into());
    }

    let mut value = environment.sky_light_level;
    for timeline in &environment.tracks {
        let ticks = clock_ticks(timeline)?;
        for (attribute, track) in &timeline.tracks {
            if attribute != SKY_LIGHT_LEVEL_ATTRIBUTE {
                continue;
            }
            let argument = sample_float_track(track, ticks)?;
            value = match track.modifier.as_deref().unwrap_or("override") {
                "override" => argument,
                "add" => value + argument,
                "subtract" => value - argument,
                "multiply" => value * argument,
                "minimum" => value.min(argument),
                "maximum" => value.max(argument),
                modifier => {
                    return Err(format!("unsupported SKY_LIGHT_LEVEL modifier: {modifier}"));
                }
            };
            if !value.is_finite() {
                return Err("non-finite SKY_LIGHT_LEVEL result".into());
            }
        }
    }
    value = value.clamp(0.0, 15.0);
    if environment.has_weather {
        let effective_thunder = environment.thunder_level * environment.rain_level;
        let rain_only = environment.rain_level - effective_thunder;
        value = lerp(rain_only * 0.3125, value, 4.0);
        value = lerp(effective_thunder * 0.52734375, value, 4.0);
    }
    value = value.clamp(0.0, 15.0);
    Ok(SkyLightEvaluation {
        sky_light_level: value,
        sky_darken: (15.0 - value).trunc() as u8,
        ambient_light: environment.ambient_light,
    })
}

fn lerp(alpha: f32, from: f32, to: f32) -> f32 {
    from + alpha * (to - from)
}

/// Evaluate only the EnvironmentAttributes consumed by the particle lightmap.
/// Layer order mirrors Java: dimension defaults/explicit values, timelines,
/// weather, then ClientLevel's lightning-flash override.
pub fn evaluate_lightmap_attributes(
    environment: &DimensionEnvironment,
    rain_level: f32,
    thunder_level: f32,
    lightning_flash: bool,
    end_flash_intensity: f32,
    end_flash_creates_world_fog: bool,
    mut clock_ticks: impl FnMut(&LightmapTimeline) -> Result<f64, String>,
) -> Result<LightmapFrameAttributes, String> {
    if let Some(error) = environment.lightmap_track_errors.first() {
        return Err(error.clone());
    }
    let mut sky_factor = environment
        .lightmap_attributes
        .sky_light_factor
        .unwrap_or(1.0);
    if !sky_factor.is_finite() {
        return Err("non-finite SKY_LIGHT_FACTOR base value".into());
    }
    sky_factor = sky_factor.clamp(0.0, 1.0);
    let mut sky_color = environment
        .lightmap_attributes
        .sky_light_color
        .unwrap_or(-1);
    let mut attributes = LightmapFrameAttributes {
        sky_light_factor: 1.0,
        block_light_tint: environment
            .lightmap_attributes
            .block_light_tint
            .unwrap_or(-10_100),
        sky_light_color: -1,
        ambient_light_color: environment
            .lightmap_attributes
            .ambient_light_color
            .unwrap_or(-16_777_216),
        night_vision_color: environment
            .lightmap_attributes
            .night_vision_color
            .unwrap_or(-6_710_887),
    };

    for timeline in &environment.lightmap_timelines {
        let ticks = clock_ticks(timeline)?;
        if let Some(track) = &timeline.sky_light_factor {
            let argument = sample_float_track_at(track, ticks)?;
            sky_factor = match track.modifier.as_deref().unwrap_or("override") {
                "override" => argument,
                "add" => sky_factor + argument,
                "subtract" => sky_factor - argument,
                "multiply" => sky_factor * argument,
                "minimum" => sky_factor.min(argument),
                "maximum" => sky_factor.max(argument),
                modifier => {
                    return Err(format!("unsupported SKY_LIGHT_FACTOR modifier: {modifier}"));
                }
            };
        }
        if let Some(track) = &timeline.sky_light_color {
            let argument = sample_color_track(track, ticks)?;
            sky_color = apply_rgb_color_modifier(
                sky_color,
                argument,
                track.modifier.as_deref().unwrap_or("override"),
            )?;
        }
    }

    if environment.has_weather {
        let rain = rain_level.clamp(0.0, 1.0);
        let thunder = thunder_level.clamp(0.0, rain);
        let rain_only = rain - thunder;
        sky_factor = weather_float_layer(sky_factor, rain_only, 0.24, 0.3125);
        sky_factor = weather_float_layer(sky_factor, thunder, 0.24, 0.52734375);
        sky_color = weather_color_layer(sky_color, rain_only, 0.3125);
        sky_color = weather_color_layer(sky_color, thunder, 0.52734375);
    }

    if !sky_factor.is_finite() {
        return Err("non-finite SKY_LIGHT_FACTOR result".into());
    }
    sky_factor = sky_factor.clamp(0.0, 1.0);
    if lightning_flash {
        sky_factor = 1.0;
    }
    let end_flash = if end_flash_creates_world_fog {
        end_flash_intensity / 3.0
    } else {
        end_flash_intensity
    };
    sky_factor += end_flash;
    attributes.sky_light_factor = sky_factor;
    attributes.sky_light_color = sky_color;
    Ok(attributes)
}

fn weather_float_layer(value: f32, intensity: f32, target: f32, alpha: f32) -> f32 {
    let modifier = value + (target - value) * alpha;
    value + (modifier - value) * intensity
}

fn weather_color_layer(value: i32, intensity: f32, alpha: f32) -> i32 {
    let alpha_byte = (alpha * 255.0).floor() as u32;
    let target = ((alpha_byte << 24) | 0x007a_7aff) as i32;
    let modified = alpha_blend_color(value, target);
    srgb_lerp_color(intensity, value, modified)
}

fn color_channels(color: i32) -> [u32; 3] {
    let color = color as u32;
    [(color >> 16) & 255, (color >> 8) & 255, color & 255]
}

fn srgb_lerp_color(alpha: f32, from: i32, to: i32) -> i32 {
    let from = color_channels(from);
    let to = color_channels(to);
    let channel = |a: u32, b: u32| {
        (a as f32 + (b as f32 - a as f32) * alpha)
            .floor()
            .clamp(0.0, 255.0) as u32
    };
    ((channel(from[0], to[0]) << 16) | (channel(from[1], to[1]) << 8) | channel(from[2], to[2]))
        as i32
}

fn alpha_blend_color(subject: i32, argument: i32) -> i32 {
    let alpha = ((argument as u32 >> 24) & 255) as i32;
    if alpha == 0 {
        return subject;
    }
    let subject = color_channels(subject);
    let source = color_channels(argument);
    let blend = |dst: u32, src: u32| (src * alpha as u32 + dst * (255 - alpha) as u32) / 255;
    ((blend(subject[0], source[0]) << 16)
        | (blend(subject[1], source[1]) << 8)
        | blend(subject[2], source[2])) as i32
}

fn apply_rgb_color_modifier(subject: i32, argument: i32, modifier: &str) -> Result<i32, String> {
    let a = color_channels(subject);
    let b = color_channels(argument);
    let channel = |x: u32, y: u32| -> Result<u32, String> {
        match modifier {
            "override" => Ok(y),
            "multiply" => Ok(x * y / 255),
            "add" => Ok((x + y).min(255)),
            "subtract" => Ok(x.saturating_sub(y)),
            other => Err(format!("unsupported SKY_LIGHT_COLOR modifier: {other}")),
        }
    };
    Ok(((channel(a[0], b[0])? << 16) | (channel(a[1], b[1])? << 8) | channel(a[2], b[2])?) as i32)
}

fn sample_color_track(track: &ColorTrack, ticks: f64) -> Result<i32, String> {
    let constant = match track.easing.as_str() {
        "linear" => false,
        "constant" => true,
        other => return Err(format!("unsupported SKY_LIGHT_COLOR easing: {other}")),
    };
    let frames = &track.keyframes;
    if frames.is_empty()
        || frames.len() > MAX_KEYFRAMES
        || track.period_ticks.is_some_and(|period| period <= 0)
        || frames.iter().any(|frame| frame.ticks < 0)
        || frames.windows(2).any(|pair| pair[0].ticks > pair[1].ticks)
        || frames
            .windows(3)
            .any(|group| group[0].ticks == group[1].ticks && group[1].ticks == group[2].ticks)
        || track
            .period_ticks
            .is_some_and(|period| frames.iter().any(|frame| frame.ticks > period))
    {
        return Err("invalid SKY_LIGHT_COLOR keyframes".into());
    }
    if frames.len() == 1 {
        return Ok(frames[0].value);
    }
    let t = track
        .period_ticks
        .map_or(ticks, |period| ticks.rem_euclid(f64::from(period)));
    let first = f64::from(frames[0].ticks);
    let last = f64::from(frames.last().unwrap().ticks);
    let (from, from_tick, to, to_tick) = if let Some(period) = track.period_ticks {
        let period = f64::from(period);
        if t < first {
            let from = frames.last().unwrap();
            (from, f64::from(from.ticks) - period, &frames[0], first)
        } else if t >= last {
            let from = frames.last().unwrap();
            (from, f64::from(from.ticks), &frames[0], first + period)
        } else {
            color_segment(frames, t)
        }
    } else if t <= first {
        return Ok(frames[0].value);
    } else if t >= last {
        return Ok(frames.last().unwrap().value);
    } else {
        color_segment(frames, t)
    };
    if t <= from_tick {
        return Ok(from.value);
    }
    if t >= to_tick {
        return Ok(to.value);
    }
    if to_tick <= from_tick {
        return Err("invalid SKY_LIGHT_COLOR segment".into());
    }
    let alpha = if constant {
        0.0
    } else {
        ((t - from_tick) / (to_tick - from_tick)) as f32
    };
    Ok(srgb_lerp_color(alpha, from.value, to.value))
}

fn color_segment(
    frames: &[ColorKeyframe],
    ticks: f64,
) -> (&ColorKeyframe, f64, &ColorKeyframe, f64) {
    for pair in frames.windows(2) {
        if ticks < f64::from(pair[1].ticks) {
            return (
                &pair[0],
                f64::from(pair[0].ticks),
                &pair[1],
                f64::from(pair[1].ticks),
            );
        }
    }
    let last = frames.last().unwrap();
    (last, f64::from(last.ticks), last, f64::from(last.ticks))
}

/// Java `EndFlashState` is seeded from the dimension's default-clock time and
/// updates once per tick; render partial ticks interpolate adjacent
/// intensities.
pub fn end_flash_intensity(clock_time: f64) -> f32 {
    fn at_tick(tick: i64) -> f32 {
        let seed = tick / 600;
        if seed == 0 {
            return 0.0; // EndFlashState's initial seed/offset/duration are zero.
        }
        let mut rng = crate::util::JavaRandom::new(seed);
        let _ = rng.next_float();
        let offset = rng.next_int(201) as i64;
        let duration = 100 + rng.next_int((380_i64.min(600 - offset) - 100 + 1) as i32) as i64;
        let within = tick.rem_euclid(600);
        if within < offset || within > offset + duration {
            0.0
        } else {
            (((within - offset) as f32) * std::f32::consts::PI / duration as f32).sin()
        }
    }

    if !clock_time.is_finite() {
        return 0.0;
    }
    let tick = clock_time.floor() as i64;
    let partial = (clock_time - tick as f64) as f32;
    at_tick(tick - 1) + (at_tick(tick) - at_tick(tick - 1)) * partial
}

#[cfg(test)]
mod lightmap_frame_tests {
    use super::*;

    fn overworld_day() -> LightmapTimeline {
        LightmapTimeline {
            id: "minecraft:day".into(),
            clock: "minecraft:overworld".into(),
            period_ticks: Some(24_000),
            sky_light_factor: Some(FloatTrack {
                period_ticks: Some(24_000),
                easing: "linear".into(),
                modifier: Some("multiply".into()),
                keyframes: vec![
                    FloatKeyframe {
                        ticks: 730,
                        value: 1.0,
                    },
                    FloatKeyframe {
                        ticks: 11_270,
                        value: 1.0,
                    },
                    FloatKeyframe {
                        ticks: 13_140,
                        value: 0.24,
                    },
                    FloatKeyframe {
                        ticks: 22_860,
                        value: 0.24,
                    },
                ],
            }),
            sky_light_color: Some(ColorTrack {
                period_ticks: Some(24_000),
                easing: "linear".into(),
                modifier: Some("multiply".into()),
                keyframes: vec![
                    ColorKeyframe {
                        ticks: 730,
                        value: 0x00ff_ffff,
                    },
                    ColorKeyframe {
                        ticks: 11_270,
                        value: 0x00ff_ffff,
                    },
                    ColorKeyframe {
                        ticks: 13_140,
                        value: 0x007a_7aff,
                    },
                    ColorKeyframe {
                        ticks: 22_860,
                        value: 0x007a_7aff,
                    },
                ],
            }),
        }
    }

    fn at(environment: &DimensionEnvironment, ticks: f64) -> LightmapFrameAttributes {
        evaluate_lightmap_attributes(environment, 0.0, 0.0, false, 0.0, false, |_| Ok(ticks))
            .unwrap()
    }

    #[test]
    fn vanilla_day_timeline_reaches_lightmap_settings_and_lut_at_noon_sunrise_and_night() {
        let mut environment = DimensionEnvironment::default();
        environment.lightmap_timelines.push(overworld_day());
        let noon = at(&environment, 6_000.0);
        assert_eq!(noon.sky_light_factor, 1.0);
        assert_eq!(noon.sky_light_color as u32 & 0x00ff_ffff, 0x00ff_ffff);
        let sunrise = at(&environment, 12_000.0);
        assert!(sunrise.sky_light_factor < 1.0 && sunrise.sky_light_factor > 0.24);
        assert_ne!(sunrise.sky_light_color as u32 & 0x00ff_ffff, 0x00ff_ffff);
        assert_ne!(sunrise.sky_light_color as u32 & 0x00ff_ffff, 0x007a_7aff);
        let night = at(&environment, 18_000.0);
        assert_eq!(night.sky_light_factor, 0.24);
        assert_eq!(night.sky_light_color as u32 & 0x00ff_ffff, 0x007a_7aff);

        let mut settings = crate::renderer::lightmap::Settings::default();
        settings.sky_factor = night.sky_light_factor;
        settings.sky_light_color = color_channels(night.sky_light_color).map(|v| v as f32 / 255.0);
        let lut = crate::renderer::lightmap::generate(settings);
        assert_ne!(
            lut[15 * 16],
            crate::renderer::lightmap::generate(Default::default())[15 * 16]
        );
    }

    #[test]
    fn weather_flash_and_end_flash_follow_java_layer_priority() {
        let mut environment = DimensionEnvironment::default();
        environment.has_weather = true;
        let rain = evaluate_lightmap_attributes(&environment, 1.0, 0.0, false, 0.0, false, |_| {
            Ok(6_000.0)
        })
        .unwrap();
        assert!((rain.sky_light_factor - 0.7625).abs() < 1e-6);
        assert_eq!(rain.sky_light_color as u32 & 0x00ff_ffff, 0x00d5_d5ff);
        let thunder =
            evaluate_lightmap_attributes(&environment, 1.0, 1.0, false, 0.0, false, |_| {
                Ok(6_000.0)
            })
            .unwrap();
        assert!((thunder.sky_light_factor - 0.5992187).abs() < 1e-6);
        assert_eq!(thunder.sky_light_color as u32 & 0x00ff_ffff, 0x00b9_b9ff);
        let flash =
            evaluate_lightmap_attributes(&environment, 1.0, 1.0, true, 0.6, true, |_| Ok(6_000.0))
                .unwrap();
        assert!(
            (flash.sky_light_factor - 1.2).abs() < 1e-6,
            "lightning override precedes the 1/3 End flash addition"
        );
        let no_flash =
            evaluate_lightmap_attributes(&environment, 0.0, 0.0, false, 0.0, false, |_| {
                Ok(6_000.0)
            })
            .unwrap();
        assert_eq!(no_flash.sky_light_factor, 1.0);
    }

    #[test]
    fn explicit_dimension_values_are_timeline_base_not_replaced_by_defaults() {
        let mut environment = DimensionEnvironment::default();
        environment.lightmap_attributes.sky_light_factor = Some(0.8);
        environment.lightmap_attributes.sky_light_color = Some(0x0080_4020);
        let mut timeline = overworld_day();
        timeline.sky_light_factor.as_mut().unwrap().keyframes = vec![FloatKeyframe {
            ticks: 0,
            value: 0.5,
        }];
        timeline.sky_light_color.as_mut().unwrap().keyframes = vec![ColorKeyframe {
            ticks: 0,
            value: 0x0080_8080,
        }];
        environment.lightmap_timelines.push(timeline);
        let value = at(&environment, 0.0);
        assert!((value.sky_light_factor - 0.4).abs() < 1e-6);
        assert_eq!(value.sky_light_color as u32 & 0x00ff_ffff, 0x0040_2010);
        assert_eq!(value.block_light_tint, -10_100);
        let timeline = &mut environment.lightmap_timelines[0];
        timeline.sky_light_factor.as_mut().unwrap().modifier = Some("override".into());
        timeline.sky_light_color.as_mut().unwrap().modifier = Some("override".into());
        let overridden = at(&environment, 0.0);
        assert_eq!(overridden.sky_light_factor, 0.5);
        assert_eq!(overridden.sky_light_color as u32 & 0x00ff_ffff, 0x0080_8080);
    }

    #[test]
    fn end_flash_is_clock_seeded_and_fades_at_its_tick_edges() {
        let mut rng = crate::util::JavaRandom::new(1);
        let _ = rng.next_float();
        let offset = rng.next_int(201) as i64;
        let duration = 100 + rng.next_int(281) as i64;
        let middle = 600 + offset + duration / 2;
        assert_eq!(end_flash_intensity(600.0), 0.0);
        assert!(end_flash_intensity(middle as f64) > 0.99);
        assert!(end_flash_intensity((600 + offset - 1) as f64) <= 0.0);
    }
}

fn sample_float_track(track: &FloatTrack, ticks: i64) -> Result<f32, String> {
    let easing = match track.easing.as_str() {
        "linear" => false,
        "constant" => true,
        unsupported => return Err(format!("unsupported SKY_LIGHT_LEVEL easing: {unsupported}")),
    };
    let modifier = track.modifier.as_deref().unwrap_or("override");
    if !matches!(
        modifier,
        "override" | "add" | "subtract" | "multiply" | "minimum" | "maximum"
    ) {
        return Err(format!("unsupported SKY_LIGHT_LEVEL modifier: {modifier}"));
    }
    let frames = &track.keyframes;
    if frames.is_empty()
        || frames.len() > MAX_KEYFRAMES
        || track.period_ticks.is_some_and(|period| period <= 0)
        || frames
            .iter()
            .any(|frame| frame.ticks < 0 || !frame.value.is_finite())
        || frames.windows(2).any(|pair| pair[0].ticks > pair[1].ticks)
        || frames
            .windows(3)
            .any(|group| group[0].ticks == group[1].ticks && group[1].ticks == group[2].ticks)
        || track
            .period_ticks
            .is_some_and(|period| frames.iter().any(|frame| frame.ticks > period))
    {
        return Err("invalid SKY_LIGHT_LEVEL keyframes".into());
    }
    if frames.len() == 1 {
        return Ok(frames[0].value);
    }
    let t = track
        .period_ticks
        .map_or(ticks, |period| ticks.rem_euclid(i64::from(period)));
    let (from, from_tick, to, to_tick) = if let Some(period) = track.period_ticks {
        if t < i64::from(frames[0].ticks) {
            let from = frames.last().unwrap();
            (
                from,
                i64::from(from.ticks) - i64::from(period),
                &frames[0],
                i64::from(frames[0].ticks),
            )
        } else if t >= i64::from(frames.last().unwrap().ticks) {
            let from = frames.last().unwrap();
            (
                from,
                i64::from(from.ticks),
                &frames[0],
                i64::from(frames[0].ticks) + i64::from(period),
            )
        } else {
            segment(frames, t)
        }
    } else if t <= i64::from(frames[0].ticks) {
        return Ok(frames[0].value);
    } else if t >= i64::from(frames.last().unwrap().ticks) {
        return Ok(frames.last().unwrap().value);
    } else {
        segment(frames, t)
    };
    if t <= from_tick {
        return Ok(from.value);
    }
    if t >= to_tick {
        return Ok(to.value);
    }
    if to_tick <= from_tick {
        return Err("invalid SKY_LIGHT_LEVEL keyframe segment".into());
    }
    let alpha = if easing {
        0.0
    } else {
        (t - from_tick) as f32 / (to_tick - from_tick) as f32
    };
    Ok(lerp(alpha, from.value, to.value))
}

fn sample_float_track_at(track: &FloatTrack, ticks: f64) -> Result<f32, String> {
    let easing = match track.easing.as_str() {
        "linear" => 0,
        "constant" => 1,
        unsupported => return Err(format!("unsupported SKY_LIGHT_LEVEL easing: {unsupported}")),
    };
    let modifier = track.modifier.as_deref().unwrap_or("override");
    if !matches!(
        modifier,
        "override" | "add" | "subtract" | "multiply" | "minimum" | "maximum"
    ) {
        return Err(format!("unsupported SKY_LIGHT_LEVEL modifier: {modifier}"));
    }
    let frames = &track.keyframes;
    if frames.is_empty() || frames.len() > MAX_KEYFRAMES {
        return Err("invalid SKY_LIGHT_LEVEL keyframe count".into());
    }
    if track.period_ticks.is_some_and(|period| period <= 0) {
        return Err("invalid SKY_LIGHT_LEVEL timeline period".into());
    }
    if frames
        .iter()
        .any(|frame| frame.ticks < 0 || !frame.value.is_finite())
        || frames.windows(2).any(|pair| pair[0].ticks > pair[1].ticks)
        || frames
            .windows(3)
            .any(|group| group[0].ticks == group[1].ticks && group[1].ticks == group[2].ticks)
        || track
            .period_ticks
            .is_some_and(|period| frames.iter().any(|frame| frame.ticks > period))
    {
        return Err("invalid SKY_LIGHT_LEVEL keyframes".into());
    }
    if frames.len() == 1 {
        return Ok(frames[0].value);
    }

    let period = track.period_ticks;
    let t = period.map_or(ticks, |period| ticks.rem_euclid(f64::from(period)));
    let first = f64::from(frames[0].ticks);
    let last = f64::from(frames.last().unwrap().ticks);
    let (from, from_tick, to, to_tick) = if let Some(period) = period {
        let period = f64::from(period);
        if t < first {
            let from = frames.last().unwrap();
            (from, f64::from(from.ticks) - period, &frames[0], first)
        } else if t >= last {
            let from = frames.last().unwrap();
            (from, f64::from(from.ticks), &frames[0], first + period)
        } else {
            segment_at(frames, t)
        }
    } else if t <= first {
        return Ok(frames[0].value);
    } else if t >= last {
        return Ok(frames.last().unwrap().value);
    } else {
        segment_at(frames, t)
    };
    if t <= from_tick {
        return Ok(from.value);
    }
    if t >= to_tick {
        return Ok(to.value);
    }
    if to_tick <= from_tick {
        return Err("invalid float timeline keyframe segment".into());
    }
    let alpha = ((t - from_tick) / (to_tick - from_tick)) as f32;
    let alpha = if easing == 1 { 0.0 } else { alpha };
    Ok(lerp(alpha, from.value, to.value))
}

fn segment_at(frames: &[FloatKeyframe], ticks: f64) -> (&FloatKeyframe, f64, &FloatKeyframe, f64) {
    for pair in frames.windows(2) {
        if ticks < f64::from(pair[1].ticks) {
            return (
                &pair[0],
                f64::from(pair[0].ticks),
                &pair[1],
                f64::from(pair[1].ticks),
            );
        }
    }
    let last = frames.last().unwrap();
    (last, f64::from(last.ticks), last, f64::from(last.ticks))
}

fn segment(frames: &[FloatKeyframe], ticks: i64) -> (&FloatKeyframe, i64, &FloatKeyframe, i64) {
    for pair in frames.windows(2) {
        if ticks < i64::from(pair[1].ticks) {
            return (
                &pair[0],
                i64::from(pair[0].ticks),
                &pair[1],
                i64::from(pair[1].ticks),
            );
        }
    }
    let last = frames.last().unwrap();
    (last, i64::from(last.ticks), last, i64::from(last.ticks))
}

fn string(compound: &NbtCompound, key: &str) -> Option<String> {
    compound.string(key).map(|value| value.to_string())
}

fn color_track(compound: &NbtCompound, period_ticks: Option<i32>) -> Option<ColorTrack> {
    if (compound.get("ease").is_some() && string(compound, "ease").is_none())
        || (compound.get("modifier").is_some() && string(compound, "modifier").is_none())
    {
        return None;
    }
    let frames = match compound.list("keyframes")? {
        NbtList::Compound(frames) if frames.len() <= MAX_KEYFRAMES => frames,
        _ => return None,
    };
    let mut keyframes: Vec<ColorKeyframe> = Vec::with_capacity(frames.len());
    for frame in frames {
        let ticks = frame.int("ticks")?;
        let value = match frame.get("value")? {
            simdnbt::owned::NbtTag::Int(value) => *value,
            simdnbt::owned::NbtTag::String(value) => {
                let text = value.to_str();
                let digits = text.strip_prefix('#').unwrap_or(&text);
                i32::from_str_radix(digits, 16).ok()?
            }
            _ => return None,
        };
        let duplicate_tick_count = keyframes
            .iter()
            .rev()
            .take_while(|frame| frame.ticks == ticks)
            .count();
        if ticks < 0
            || keyframes.last().is_some_and(|last| last.ticks > ticks)
            || duplicate_tick_count >= 2
        {
            return None;
        }
        keyframes.push(ColorKeyframe { ticks, value });
    }
    if keyframes.is_empty()
        || period_ticks
            .is_some_and(|period| period <= 0 || keyframes.iter().any(|frame| frame.ticks > period))
    {
        return None;
    }
    Some(ColorTrack {
        period_ticks,
        easing: string(compound, "ease").unwrap_or_else(|| "linear".into()),
        modifier: string(compound, "modifier"),
        keyframes,
    })
}

fn float_track(compound: &NbtCompound) -> Option<FloatTrack> {
    if (compound.get("ease").is_some() && string(compound, "ease").is_none())
        || (compound.get("modifier").is_some() && string(compound, "modifier").is_none())
    {
        return None;
    }
    let frames = match compound.list("keyframes")? {
        NbtList::Compound(frames) if frames.len() <= MAX_KEYFRAMES => frames,
        _ => return None,
    };
    let mut keyframes: Vec<FloatKeyframe> = Vec::with_capacity(frames.len());
    for frame in frames {
        let ticks = frame.int("ticks")?;
        let value = frame.float("value")?;
        let duplicate_tick_count = keyframes
            .iter()
            .rev()
            .take_while(|frame| frame.ticks == ticks)
            .count();
        if ticks < 0
            || !value.is_finite()
            || keyframes.last().is_some_and(|last| last.ticks > ticks)
            || duplicate_tick_count >= 2
        {
            return None;
        }
        keyframes.push(FloatKeyframe { ticks, value });
    }
    if keyframes.is_empty() {
        return None;
    }
    Some(FloatTrack {
        period_ticks: None,
        easing: string(compound, "ease").unwrap_or_else(|| "linear".into()),
        modifier: string(compound, "modifier"),
        keyframes,
    })
}

pub const AMBIENT_PARTICLES_ATTRIBUTE: &str = "minecraft:visual/ambient_particles";
pub const WATER_EVAPORATES_ATTRIBUTE: &str = "minecraft:gameplay/water_evaporates";
pub const DEFAULT_DRIPSTONE_PARTICLE_ATTRIBUTE: &str =
    "minecraft:visual/default_dripstone_particle";

#[derive(Clone, Debug)]
pub struct BoolAttributeLayer {
    pub modifier: String,
    pub argument: bool,
}

impl BoolAttributeLayer {
    pub fn apply(&self, base: bool) -> bool {
        match self.modifier.as_str() {
            "override" => self.argument,
            "and" => base & self.argument,
            "nand" => !(base & self.argument),
            "or" => base | self.argument,
            "nor" => !(base | self.argument),
            "xor" => base ^ self.argument,
            "xnor" => !(base ^ self.argument),
            _ => base,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BoolTimeline {
    pub id: String,
    pub clock: String,
    pub period_ticks: Option<i32>,
    pub keyframes: Vec<(i32, bool)>,
    pub modifier: String,
}

#[derive(Clone, Debug)]
pub struct ParticleTimeline {
    pub id: String,
    pub clock: String,
    pub period_ticks: Option<i32>,
    pub keyframes: Vec<(i32, crate::world::environment_particles::AmbientParticle)>,
}

fn ambient_particle_value(
    tag: &simdnbt::owned::NbtTag,
) -> Option<Vec<crate::world::environment_particles::AmbientParticle>> {
    use simdnbt::owned::{NbtList, NbtTag};
    let expected = match tag {
        NbtTag::List(NbtList::Compound(entries)) => entries.len(),
        NbtTag::List(NbtList::Empty) => 0,
        _ => return None,
    };
    let mut attributes = NbtCompound::new();
    attributes.insert(AMBIENT_PARTICLES_ATTRIBUTE, tag.clone());
    let parsed = crate::net::connection::extract_ambient_attribute(&attributes)?;
    (parsed.len() == expected).then_some(parsed)
}

pub fn water_evaporates_layer(tag: &simdnbt::owned::NbtTag) -> Option<BoolAttributeLayer> {
    use simdnbt::owned::NbtTag;
    let (modifier, argument) = match tag {
        NbtTag::Byte(value) => ("override".to_owned(), *value != 0),
        NbtTag::Compound(value) => (
            value.string("modifier")?.to_str().into_owned(),
            value.get("argument")?.byte()? != 0,
        ),
        _ => return None,
    };
    matches!(
        modifier.as_str(),
        "override" | "and" | "nand" | "or" | "nor" | "xor" | "xnor"
    )
    .then_some(BoolAttributeLayer { modifier, argument })
}

pub fn water_evaporates_value(tag: &simdnbt::owned::NbtTag, base: bool) -> Option<bool> {
    Some(water_evaporates_layer(tag)?.apply(base))
}

pub fn dripstone_particle_value(
    tag: &simdnbt::owned::NbtTag,
) -> Option<crate::world::environment_particles::AmbientParticle> {
    use simdnbt::owned::NbtTag;
    let particle = match tag {
        NbtTag::Compound(value) => {
            if value
                .get("modifier")
                .is_some_and(|m| m.string().is_none_or(|s| s.to_str() != "override"))
            {
                return None;
            }
            value
                .get("argument")
                .and_then(|arg| arg.compound())
                .unwrap_or(value)
        }
        _ => return None,
    };
    let name = particle.string("type")?.to_str().into_owned();
    let kind = crate::particle::ServerParticleKind::from_name(
        name.strip_prefix("minecraft:").unwrap_or(&name),
    )?;
    let options = crate::net::connection::ambient_particle_options(kind, particle)?;
    Some(crate::world::environment_particles::AmbientParticle {
        kind,
        options,
        probability: 1.0,
    })
}

fn bool_timeline(
    id: &str,
    clock: &str,
    period: Option<i32>,
    track: &NbtCompound,
) -> Result<BoolTimeline, String> {
    use simdnbt::owned::NbtList;
    let modifier = string(track, "modifier").unwrap_or_else(|| "override".into());
    if clock.is_empty()
        || !matches!(
            modifier.as_str(),
            "override" | "and" | "nand" | "or" | "nor" | "xor" | "xnor"
        )
        || period.is_some_and(|p| p <= 0)
    {
        return Err(format!(
            "invalid WATER_EVAPORATES modifier/period in timeline {id}"
        ));
    }
    let Some(NbtList::Compound(frames)) = track.list("keyframes") else {
        return Err(format!(
            "invalid WATER_EVAPORATES keyframes in timeline {id}"
        ));
    };
    if frames.is_empty() || frames.len() > MAX_KEYFRAMES {
        return Err(format!(
            "invalid WATER_EVAPORATES keyframes in timeline {id}"
        ));
    }
    let mut keyframes = Vec::with_capacity(frames.len());
    for frame in frames {
        let tick = frame
            .int("ticks")
            .filter(|t| *t >= 0)
            .ok_or_else(|| format!("invalid WATER_EVAPORATES tick in timeline {id}"))?;
        let value = frame
            .byte("value")
            .map(|b| b != 0)
            .ok_or_else(|| format!("invalid WATER_EVAPORATES value in timeline {id}"))?;
        if keyframes.last().is_some_and(|(last, _)| *last > tick)
            || keyframes
                .iter()
                .rev()
                .take_while(|(last, _)| *last == tick)
                .count()
                >= 2
            || period.is_some_and(|p| tick > p)
        {
            return Err(format!(
                "invalid WATER_EVAPORATES frame order/range in timeline {id}"
            ));
        }
        keyframes.push((tick, value));
    }
    Ok(BoolTimeline {
        id: id.into(),
        clock: clock.into(),
        period_ticks: period,
        keyframes,
        modifier,
    })
}

fn dripstone_particle_timeline(
    id: &str,
    clock: &str,
    period: Option<i32>,
    track: &NbtCompound,
) -> Result<ParticleTimeline, String> {
    use simdnbt::owned::NbtList;
    if clock.is_empty() {
        return Err(format!(
            "invalid DEFAULT_DRIPSTONE_PARTICLE timeline clock: {id}"
        ));
    }
    if track.get("modifier").is_some() && string(track, "modifier").as_deref() != Some("override") {
        return Err(format!(
            "unsupported DEFAULT_DRIPSTONE_PARTICLE modifier in timeline {id}"
        ));
    }
    let Some(NbtList::Compound(frames)) = track.list("keyframes") else {
        return Err(format!(
            "invalid DEFAULT_DRIPSTONE_PARTICLE keyframes in timeline {id}"
        ));
    };
    if frames.is_empty() || frames.len() > MAX_KEYFRAMES || period.is_some_and(|p| p <= 0) {
        return Err(format!("invalid DEFAULT_DRIPSTONE_PARTICLE timeline {id}"));
    }
    let mut keyframes = Vec::new();
    for frame in frames {
        let tick = frame
            .int("ticks")
            .filter(|t| *t >= 0 && period.is_none_or(|p| *t <= p))
            .ok_or_else(|| format!("invalid DEFAULT_DRIPSTONE_PARTICLE tick in timeline {id}"))?;
        let value = frame
            .get("value")
            .and_then(dripstone_particle_value)
            .ok_or_else(|| format!("invalid DEFAULT_DRIPSTONE_PARTICLE value in timeline {id}"))?;
        if keyframes.last().is_some_and(
            |(last, _): &(i32, crate::world::environment_particles::AmbientParticle)| *last > tick,
        ) || keyframes
            .iter()
            .rev()
            .take_while(|(last, _)| *last == tick)
            .count()
            >= 2
        {
            return Err(format!(
                "invalid DEFAULT_DRIPSTONE_PARTICLE frame order in timeline {id}"
            ));
        }
        keyframes.push((tick, value));
    }
    Ok(ParticleTimeline {
        id: id.into(),
        clock: clock.into(),
        period_ticks: period,
        keyframes,
    })
}

fn ambient_particle_timeline(
    id: &str,
    clock: &str,
    period_ticks: Option<i32>,
    track: &NbtCompound,
) -> Result<AmbientParticleTimeline, String> {
    use simdnbt::owned::{NbtList, NbtTag};
    if track.get("modifier").is_some() && string(track, "modifier").as_deref() != Some("override") {
        return Err(format!(
            "unsupported AMBIENT_PARTICLES modifier in timeline {id}"
        ));
    }
    if period_ticks.is_some_and(|period| period <= 0) {
        return Err(format!("invalid AMBIENT_PARTICLES period in timeline {id}"));
    }
    let easing = match track.get("ease") {
        None => AmbientEasing::Simple("linear".into()),
        Some(NbtTag::String(name)) => {
            let name = name.to_string();
            if !is_easing_name(&name) {
                return Err(format!(
                    "unknown AMBIENT_PARTICLES easing {name} in timeline {id}"
                ));
            }
            AmbientEasing::Simple(name)
        }
        Some(NbtTag::Compound(cubic)) => {
            let Some(NbtTag::List(NbtList::Float(values))) = cubic.get("cubic_bezier") else {
                return Err(format!(
                    "invalid AMBIENT_PARTICLES cubic easing in timeline {id}"
                ));
            };
            if values.len() != 4
                || values.iter().any(|v| !v.is_finite())
                || !(0.0..=1.0).contains(&values[0])
                || !(0.0..=1.0).contains(&values[2])
            {
                return Err(format!(
                    "invalid AMBIENT_PARTICLES cubic controls in timeline {id}"
                ));
            }
            AmbientEasing::CubicBezier {
                x1: values[0],
                y1: values[1],
                x2: values[2],
                y2: values[3],
            }
        }
        _ => return Err(format!("invalid AMBIENT_PARTICLES easing in timeline {id}")),
    };
    let frames = match track.list("keyframes") {
        Some(NbtList::Compound(frames)) if !frames.is_empty() && frames.len() <= MAX_KEYFRAMES => {
            frames
        }
        _ => {
            return Err(format!(
                "invalid AMBIENT_PARTICLES keyframes in timeline {id}"
            ));
        }
    };
    let mut keyframes = Vec::with_capacity(frames.len());
    for frame in frames {
        let ticks = frame
            .int("ticks")
            .filter(|ticks| *ticks >= 0)
            .ok_or_else(|| format!("invalid AMBIENT_PARTICLES keyframe tick in timeline {id}"))?;
        if keyframes
            .last()
            .is_some_and(|previous: &AmbientParticleKeyframe| previous.ticks > ticks)
            || keyframes
                .iter()
                .rev()
                .take_while(|previous: &&AmbientParticleKeyframe| previous.ticks == ticks)
                .count()
                >= 2
            || period_ticks.is_some_and(|period| ticks > period)
        {
            return Err(format!(
                "invalid AMBIENT_PARTICLES keyframe order/range in timeline {id}"
            ));
        }
        let value = frame
            .get("value")
            .and_then(ambient_particle_value)
            .ok_or_else(|| format!("invalid AMBIENT_PARTICLES keyframe value in timeline {id}"))?;
        keyframes.push(AmbientParticleKeyframe { ticks, value });
    }
    Ok(AmbientParticleTimeline {
        id: id.into(),
        clock: clock.into(),
        period_ticks,
        easing,
        keyframes,
    })
}

fn is_easing_name(name: &str) -> bool {
    matches!(
        name,
        "constant"
            | "linear"
            | "in_back"
            | "in_bounce"
            | "in_circ"
            | "in_cubic"
            | "in_elastic"
            | "in_expo"
            | "in_quad"
            | "in_quart"
            | "in_quint"
            | "in_sine"
            | "in_out_back"
            | "in_out_bounce"
            | "in_out_circ"
            | "in_out_cubic"
            | "in_out_elastic"
            | "in_out_expo"
            | "in_out_quad"
            | "in_out_quart"
            | "in_out_quint"
            | "in_out_sine"
            | "out_back"
            | "out_bounce"
            | "out_circ"
            | "out_cubic"
            | "out_elastic"
            | "out_expo"
            | "out_quad"
            | "out_quart"
            | "out_quint"
            | "out_sine"
    )
}

/// Resolve time-based layers after dimension constants and the sampled biome
/// value. Ambient particles use the attribute's step keyframe lerp; the list
/// itself is never blended.
pub fn evaluate_ambient_particles(
    environment: &DimensionEnvironment,
    mut clock_ticks: impl FnMut(&str, &str, Option<i32>) -> Result<i64, String>,
) -> Result<Option<Vec<crate::world::environment_particles::AmbientParticle>>, String> {
    if let Some(error) = environment.ambient_particle_track_errors.first() {
        return Err(error.clone());
    }
    let mut resolved = None;
    for timeline in &environment.ambient_particle_tracks {
        let ticks = clock_ticks(&timeline.id, &timeline.clock, timeline.period_ticks)?;
        resolved = Some(sample_ambient_track(timeline, ticks)?);
    }
    Ok(resolved)
}

fn sample_ambient_track(
    track: &AmbientParticleTimeline,
    ticks: i64,
) -> Result<Vec<crate::world::environment_particles::AmbientParticle>, String> {
    let frames = &track.keyframes;
    let sample_ticks = track
        .period_ticks
        .map_or(ticks, |period| ticks.rem_euclid(i64::from(period)));
    if frames.len() == 1 {
        return Ok(frames[0].value.clone());
    }
    let (from, from_tick, to, to_tick) = if let Some(period) = track.period_ticks {
        if sample_ticks < i64::from(frames[0].ticks) {
            let from = frames.last().unwrap();
            (
                from,
                i64::from(from.ticks) - i64::from(period),
                &frames[0],
                i64::from(frames[0].ticks),
            )
        } else if sample_ticks >= i64::from(frames.last().unwrap().ticks) {
            let from = frames.last().unwrap();
            (
                from,
                i64::from(from.ticks),
                &frames[0],
                i64::from(frames[0].ticks) + i64::from(period),
            )
        } else {
            ambient_segment(frames, sample_ticks)
        }
    } else if sample_ticks <= i64::from(frames[0].ticks) {
        return Ok(frames[0].value.clone());
    } else if sample_ticks >= i64::from(frames.last().unwrap().ticks) {
        return Ok(frames.last().unwrap().value.clone());
    } else {
        ambient_segment(frames, sample_ticks)
    };
    if sample_ticks <= from_tick {
        return Ok(from.value.clone());
    }
    if sample_ticks >= to_tick {
        return Ok(to.value.clone());
    }
    if to_tick <= from_tick {
        return Err(format!(
            "invalid ambient keyframe segment in timeline {}",
            track.id
        ));
    }
    let alpha = (sample_ticks - from_tick) as f32 / (to_tick - from_tick) as f32;
    Ok(if easing_value(&track.easing, alpha) >= 1.0 {
        to.value.clone()
    } else {
        from.value.clone()
    })
}

fn ambient_segment(
    frames: &[AmbientParticleKeyframe],
    ticks: i64,
) -> (&AmbientParticleKeyframe, i64, &AmbientParticleKeyframe, i64) {
    for pair in frames.windows(2) {
        if ticks < i64::from(pair[1].ticks) {
            return (
                &pair[0],
                i64::from(pair[0].ticks),
                &pair[1],
                i64::from(pair[1].ticks),
            );
        }
    }
    let last = frames.last().unwrap();
    (last, i64::from(last.ticks), last, i64::from(last.ticks))
}

fn easing_value(easing: &AmbientEasing, x: f32) -> f32 {
    use AmbientEasing::{CubicBezier, Simple};
    match easing {
        CubicBezier { x1, y1, x2, y2 } => cubic_bezier_easing(x, *x1, *y1, *x2, *y2),
        Simple(name) => match name.as_str() {
            "constant" => 0.0,
            "linear" => x,
            "in_back" => x * x * (2.70158 * x - 1.70158),
            "in_bounce" => 1.0 - ease_out_bounce(1.0 - x),
            "in_circ" => 1.0 - (1.0 - x * x).sqrt(),
            "in_cubic" => x * x * x,
            "in_elastic" => {
                if x == 0.0 {
                    0.0
                } else if x == 1.0 {
                    1.0
                } else {
                    -(2.0_f32).powf(10.0 * x - 10.0) * (((x * 10.0 - 10.75) * 2.0943952).sin())
                }
            }
            "in_expo" => {
                if x == 0.0 {
                    0.0
                } else {
                    (2.0_f32).powf(10.0 * x - 10.0)
                }
            }
            "in_quad" => x * x,
            "in_quart" => x.powi(4),
            "in_quint" => x.powi(5),
            "in_sine" => 1.0 - (x * std::f32::consts::FRAC_PI_2).cos(),
            "in_out_back" => {
                if x < 0.5 {
                    4.0 * x * x * (7.189819 * x - 2.5949094) / 2.0
                } else {
                    let d = 2.0 * x - 2.0;
                    (d * d * (3.5949094 * d + 2.5949094) + 2.0) / 2.0
                }
            }
            "in_out_bounce" => {
                if x < 0.5 {
                    (1.0 - ease_out_bounce(1.0 - 2.0 * x)) / 2.0
                } else {
                    (1.0 + ease_out_bounce(2.0 * x - 1.0)) / 2.0
                }
            }
            "in_out_circ" => {
                if x < 0.5 {
                    (1.0 - (1.0 - (2.0 * x).powi(2)).sqrt()) / 2.0
                } else {
                    ((1.0 - (-2.0 * x + 2.0).powi(2)).sqrt() + 1.0) / 2.0
                }
            }
            "in_out_cubic" => {
                if x < 0.5 {
                    4.0 * x.powi(3)
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(3) / 2.0
                }
            }
            "in_out_elastic" => {
                if x == 0.0 {
                    0.0
                } else if x == 1.0 {
                    1.0
                } else {
                    let s = ((20.0 * x - 11.125) * 1.3962635).sin();
                    if x < 0.5 {
                        -((2.0_f32).powf(20.0 * x - 10.0) * s) / 2.0
                    } else {
                        (2.0_f32).powf(-20.0 * x + 10.0) * s / 2.0 + 1.0
                    }
                }
            }
            "in_out_expo" => {
                if x < 0.5 {
                    if x == 0.0 {
                        0.0
                    } else {
                        (2.0_f32).powf(20.0 * x - 10.0) / 2.0
                    }
                } else if x == 1.0 {
                    1.0
                } else {
                    (2.0 - (2.0_f32).powf(-20.0 * x + 10.0)) / 2.0
                }
            }
            "in_out_quad" => {
                if x < 0.5 {
                    2.0 * x * x
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(2) / 2.0
                }
            }
            "in_out_quart" => {
                if x < 0.5 {
                    8.0 * x.powi(4)
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(4) / 2.0
                }
            }
            "in_out_quint" => {
                if x < 0.5 {
                    16.0 * x.powi(5)
                } else {
                    1.0 - (-2.0 * x + 2.0).powi(5) / 2.0
                }
            }
            "in_out_sine" => -((std::f32::consts::PI * x).cos() - 1.0) / 2.0,
            "out_back" => 1.0 + 2.70158 * (x - 1.0).powi(3) + 1.70158 * (x - 1.0).powi(2),
            "out_bounce" => ease_out_bounce(x),
            "out_circ" => (1.0 - (x - 1.0).powi(2)).sqrt(),
            "out_cubic" => 1.0 - (1.0 - x).powi(3),
            "out_elastic" => {
                if x == 0.0 {
                    0.0
                } else if x == 1.0 {
                    1.0
                } else {
                    (2.0_f32).powf(-10.0 * x) * (((x * 10.0 - 0.75) * 2.0943952).sin()) + 1.0
                }
            }
            "out_expo" => {
                if x == 1.0 {
                    1.0
                } else {
                    1.0 - (2.0_f32).powf(-10.0 * x)
                }
            }
            "out_quad" => 1.0 - (1.0 - x).powi(2),
            "out_quart" => 1.0 - (1.0 - x).powi(4),
            "out_quint" => 1.0 - (1.0 - x).powi(5),
            "out_sine" => (x * std::f32::consts::FRAC_PI_2).sin(),
            _ => f32::NAN,
        },
    }
}

fn ease_out_bounce(x: f32) -> f32 {
    if x < 0.36363637 {
        7.5625 * x * x
    } else if x < 0.72727275 {
        7.5625 * (x - 0.54545456).powi(2) + 0.75
    } else if x < 0.9090909 {
        7.5625 * (x - 0.8181818).powi(2) + 0.9375
    } else {
        7.5625 * (x - 0.95454544).powi(2) + 0.984375
    }
}

fn cubic_bezier_easing(x: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let curve = |a: f32, b: f32, t: f32| {
        let aa = 3.0 * a - 3.0 * b + 1.0;
        let bb = -6.0 * a + 3.0 * b;
        let cc = 3.0 * a;
        ((aa * t + bb) * t + cc) * t
    };
    let gradient = |a: f32, b: f32, t: f32| {
        let aa = 3.0 * a - 3.0 * b + 1.0;
        let bb = -6.0 * a + 3.0 * b;
        let cc = 3.0 * a;
        (3.0 * aa * t + 2.0 * bb) * t + cc
    };
    let mut t = x;
    for _ in 0..4 {
        let error = curve(x1, x2, t) - x;
        if error.abs() < 1.0e-5 {
            return curve(y1, y2, t);
        }
        let slope = gradient(x1, x2, t);
        if slope < 1.0e-5 {
            break;
        }
        t -= (error / slope).clamp(-0.25, 0.25);
    }
    let (mut low, mut high) = (0.0, 1.0);
    while low < high {
        let error = curve(x1, x2, t) - x;
        if error.abs() < 1.0e-5 {
            return curve(y1, y2, t);
        }
        if error < 0.0 {
            low = t
        } else {
            high = t
        }
        t = (high + low) / 2.0;
    }
    curve(y1, y2, t)
}

/// Retain the dimension-level inputs and timeline tracks used by lighting and
/// the existing dimension environment evaluators.
pub fn from_dimension_fields(
    has_sky_light: bool,
    has_ceiling: bool,
    is_end_world: bool,
    ambient_light: Option<f32>,
    sky_light_level: Option<f32>,
    timeline_registry: &[(String, NbtCompound)],
    dimension_timeline_ids: &[String],
) -> Result<DimensionEnvironment, String> {
    let mut result = DimensionEnvironment {
        has_weather: has_sky_light && !has_ceiling && !is_end_world,
        ambient_light: ambient_light.ok_or("missing/invalid ambient_light")?,
        ..Default::default()
    };
    if !result.ambient_light.is_finite() || !(0.0..=1.0).contains(&result.ambient_light) {
        return Err("ambient_light outside finite range 0..=1".into());
    }
    if dimension_timeline_ids.len() > MAX_TIMELINES {
        return Err("too many dimension timelines".into());
    }
    result.timelines = dimension_timeline_ids.to_vec();
    if let Some(value) = sky_light_level {
        if !value.is_finite() || !(0.0..=15.0).contains(&value) {
            return Err("SKY_LIGHT_LEVEL outside finite range 0..=15".into());
        }
        result.sky_light_level = value;
    }
    for id in &result.timelines {
        if result
            .timelines
            .iter()
            .filter(|candidate| *candidate == id)
            .count()
            > 1
        {
            return Err(format!("duplicate dimension timeline id: {id}"));
        }
        let Some((_, nbt)) = timeline_registry
            .iter()
            .find(|(candidate, _)| candidate == id)
        else {
            return Err(format!("unknown dimension timeline id: {id}"));
        };
        let Some(tracks) = nbt.compound("tracks") else {
            continue;
        };
        let clock = string(nbt, "clock")
            .or_else(|| {
                nbt.compound("clock")
                    .and_then(|clock| string(clock, "value"))
            })
            .unwrap_or_default();
        let period_ticks = nbt.int("period_ticks");
        if let Some(value) = tracks.get(WATER_EVAPORATES_ATTRIBUTE) {
            match value {
                simdnbt::owned::NbtTag::Compound(track)
                    if !clock.is_empty()
                        && !(nbt.get("period_ticks").is_some() && period_ticks.is_none()) =>
                {
                    match bool_timeline(id, &clock, period_ticks, track) {
                        Ok(parsed) => result.water_evaporates_timelines.push(parsed),
                        Err(error) => result.environment_attribute_errors.push(error),
                    }
                }
                _ => result
                    .environment_attribute_errors
                    .push(format!("invalid WATER_EVAPORATES timeline: {id}")),
            }
        }
        if let Some(value) = tracks.get(DEFAULT_DRIPSTONE_PARTICLE_ATTRIBUTE) {
            match value {
                simdnbt::owned::NbtTag::Compound(track)
                    if !clock.is_empty()
                        && !(nbt.get("period_ticks").is_some() && period_ticks.is_none()) =>
                {
                    match dripstone_particle_timeline(id, &clock, period_ticks, track) {
                        Ok(parsed) => result.default_dripstone_particle_timelines.push(parsed),
                        Err(error) => result.environment_attribute_errors.push(error),
                    }
                }
                _ => result
                    .environment_attribute_errors
                    .push(format!("invalid DEFAULT_DRIPSTONE_PARTICLE timeline: {id}")),
            }
        }
        if let Some(value) = tracks.get(AMBIENT_PARTICLES_ATTRIBUTE) {
            let parsed = match value {
                simdnbt::owned::NbtTag::Compound(track)
                    if !clock.is_empty()
                        && !(nbt.get("period_ticks").is_some() && period_ticks.is_none()) =>
                {
                    ambient_particle_timeline(id, &clock, period_ticks, track)
                }
                simdnbt::owned::NbtTag::Compound(_) => Err(format!(
                    "invalid AMBIENT_PARTICLES timeline clock/period: {id}"
                )),
                _ => Err(format!("invalid AMBIENT_PARTICLES track codec: {id}")),
            };
            match parsed {
                Ok(track) => result.ambient_particle_tracks.push(track),
                Err(error) => result.ambient_particle_track_errors.push(error),
            }
        }
        if let Some(track) = tracks.compound(SKY_LIGHT_LEVEL_ATTRIBUTE) {
            let mut parsed =
                float_track(track).ok_or_else(|| format!("invalid SKY_LIGHT_LEVEL track: {id}"))?;
            parsed.period_ticks = period_ticks;
            if parsed.period_ticks.is_some_and(|period| {
                period <= 0 || parsed.keyframes.iter().any(|kf| kf.ticks > period)
            }) {
                return Err(format!("invalid SKY_LIGHT_LEVEL timeline period: {id}"));
            }
            result.tracks.push(TimelineInput {
                id: id.clone(),
                clock: clock.clone(),
                tracks: vec![(SKY_LIGHT_LEVEL_ATTRIBUTE.into(), parsed)],
            });
        }

        let mut lightmap_timeline = LightmapTimeline {
            id: id.clone(),
            clock: clock.clone(),
            period_ticks,
            sky_light_factor: None,
            sky_light_color: None,
        };
        if let Some(track) = tracks.compound(SKY_LIGHT_FACTOR_ATTRIBUTE) {
            match float_track(track) {
                Some(mut parsed) => {
                    parsed.period_ticks = period_ticks;
                    lightmap_timeline.sky_light_factor = Some(parsed);
                }
                None => result
                    .lightmap_track_errors
                    .push(format!("invalid SKY_LIGHT_FACTOR track: {id}")),
            }
        }
        if let Some(track) = tracks.compound(SKY_LIGHT_COLOR_ATTRIBUTE) {
            match color_track(track, period_ticks) {
                Some(parsed) => lightmap_timeline.sky_light_color = Some(parsed),
                None => result
                    .lightmap_track_errors
                    .push(format!("invalid SKY_LIGHT_COLOR track: {id}")),
            }
        }
        if lightmap_timeline.sky_light_factor.is_some()
            || lightmap_timeline.sky_light_color.is_some()
        {
            result.lightmap_timelines.push(lightmap_timeline);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dripstone_attributes_apply_positional_then_timeline_and_keep_particle_options() {
        let mut environment = DimensionEnvironment::default();
        environment.water_evaporates = false;
        environment.water_evaporates_timelines.push(BoolTimeline {
            id: "minecraft:evap".into(),
            clock: "minecraft:overworld".into(),
            period_ticks: Some(10),
            keyframes: vec![(0, true), (5, false)],
            modifier: "xor".into(),
        });
        let typed = crate::world::environment_particles::AmbientParticle {
            kind: crate::particle::ServerParticleKind::Dust,
            options: crate::particle::ServerParticleOptions::Dust {
                packed_color: 0x123456,
                scale: 1.5,
            },
            probability: 1.0,
        };
        environment
            .default_dripstone_particle_timelines
            .push(ParticleTimeline {
                id: "minecraft:particle".into(),
                clock: "minecraft:overworld".into(),
                period_ticks: Some(10),
                keyframes: vec![
                    (0, typed.clone()),
                    (
                        5,
                        crate::world::environment_particles::AmbientParticle {
                            kind: crate::particle::ServerParticleKind::Ash,
                            options: crate::particle::ServerParticleOptions::Simple,
                            probability: 1.0,
                        },
                    ),
                ],
            });
        let biome_layer = BoolAttributeLayer {
            modifier: "override".into(),
            argument: true,
        };
        let at = |tick| {
            evaluate_dripstone_attributes(&environment, Some(&biome_layer), None, |_, _, _| {
                Ok(tick)
            })
            .unwrap()
        };
        assert_eq!(
            at(4).0,
            false,
            "timeline xor biome value is applied after positional override"
        );
        assert_eq!(at(5).0, true);
        assert_eq!(at(4).1.kind, crate::particle::ServerParticleKind::Dust);
        assert!(
            matches!(at(4).1.options, crate::particle::ServerParticleOptions::Dust { packed_color: 0x123456, scale } if scale == 1.5)
        );
        assert_eq!(at(5).1.kind, crate::particle::ServerParticleKind::Ash);
        assert_eq!(
            at(10).1.kind,
            crate::particle::ServerParticleKind::Dust,
            "period wraps at exact boundary"
        );
    }

    #[test]
    fn water_evaporation_uses_attribute_over_dimension_name_and_tracks_timeline_changes() {
        let mut nether_defaults = DimensionEnvironment::default();
        nether_defaults.water_evaporates = true;
        let biome_override = BoolAttributeLayer {
            modifier: "override".into(),
            argument: false,
        };
        let timeline_at = |tick| {
            evaluate_dripstone_attributes(
                &nether_defaults,
                Some(&biome_override),
                None,
                |_, _, _| Ok(tick),
            )
            .unwrap()
            .0
        };
        assert!(
            !timeline_at(0),
            "positional false overrides a Nether-like dimension default"
        );

        let mut non_nether = DimensionEnvironment::default();
        non_nether.water_evaporates_timelines.push(BoolTimeline {
            id: "minecraft:evaporation".into(),
            clock: "minecraft:overworld".into(),
            period_ticks: Some(20),
            keyframes: vec![(0, true), (10, false)],
            modifier: "override".into(),
        });
        let at = |tick| {
            evaluate_dripstone_attributes(&non_nether, None, None, |_, _, _| Ok(tick))
                .unwrap()
                .0
        };
        assert!(at(0));
        assert!(!at(10));
        assert!(at(20), "timeline period wraps at its declared duration");
    }

    #[test]
    fn water_evaporates_boolean_modifier_codec_matches_java_modifier_library() {
        for (name, expect) in [
            ("and", false),
            ("nand", true),
            ("or", true),
            ("nor", false),
            ("xor", true),
            ("xnor", false),
        ] {
            let mut modifier = NbtCompound::new();
            modifier.insert("modifier", name);
            modifier.insert("argument", 1_i8);
            assert_eq!(
                water_evaporates_value(&simdnbt::owned::NbtTag::Compound(modifier), false),
                Some(expect)
            );
        }
        assert_eq!(
            water_evaporates_value(&simdnbt::owned::NbtTag::Byte(1), false),
            Some(true)
        );
        assert_eq!(
            water_evaporates_value(&simdnbt::owned::NbtTag::Byte(0), false),
            Some(false)
        );
    }

    #[test]
    fn signed_clock_phase_and_native_tick_advancement() {
        let sample = ClockSample {
            total_ticks: -1,
            partial_tick: 0.75,
            rate: 0.0,
        };
        assert_eq!(sample.timeline_ticks(Some(24_000)), 23_999);
        assert_eq!(
            ClockSample {
                total_ticks: i64::MIN,
                ..sample
            }
            .timeline_ticks(Some(24_000)),
            16_192
        );
        assert_eq!(
            ClockSample {
                total_ticks: (1_i64 << 53) + 1,
                ..sample
            }
            .timeline_ticks(Some(24_000)),
            12_993
        );
        assert_eq!(sample.renderer_tick(0.5), -0.25);
        assert_eq!(sample.timeline_ticks(Some(24_000)), 23_999);

        let mut stopped = sample;
        stopped.advance_game_time(20);
        assert_eq!((stopped.total_ticks, stopped.partial_tick), (-1, 0.75));
        let mut reverse = ClockSample {
            rate: -0.25,
            ..sample
        };
        reverse.advance_game_time(1);
        // Native floor(-0.25 + 0.75) is zero; the signed cursor remains -1.
        assert_eq!((reverse.total_ticks, reverse.partial_tick), (-1, 0.5));
        let mut huge = ClockSample {
            partial_tick: 0.0,
            rate: f32::MAX,
            ..sample
        };
        huge.advance_game_time(1);
        assert_eq!(huge.total_ticks, i64::from(i32::MAX) - 1);
    }

    #[test]
    fn dimension_defaults_and_trust_boundary_ranges() {
        let defaults =
            from_dimension_fields(true, false, false, Some(0.0), None, &[], &[]).unwrap();
        assert_eq!(defaults.sky_light_level, 15.0);
        assert!(defaults.has_weather);
        let end = from_dimension_fields(true, false, true, Some(0.25), None, &[], &[]).unwrap();
        assert_eq!(end.ambient_light, 0.25);
        assert!(!end.has_weather);
        let custom_end_type =
            from_dimension_fields(true, false, false, Some(0.25), None, &[], &[]).unwrap();
        assert!(custom_end_type.has_weather);
        assert!(from_dimension_fields(true, false, false, Some(f32::NAN), None, &[], &[]).is_err());
        assert!(
            from_dimension_fields(true, false, false, Some(0.0), Some(f32::INFINITY), &[], &[])
                .is_err()
        );
        assert!(
            from_dimension_fields(true, false, false, Some(0.0), Some(16.0), &[], &[]).is_err()
        );
        assert!(
            from_dimension_fields(
                true,
                false,
                false,
                Some(0.0),
                None,
                &[],
                &vec!["x".into(); MAX_TIMELINES + 1]
            )
            .is_err()
        );
    }

    #[test]
    fn end_weather_uses_world_key_and_dimension_flags() {
        assert!(
            !from_dimension_fields(true, false, true, Some(0.25), None, &[], &[])
                .unwrap()
                .has_weather
        );
        assert!(
            from_dimension_fields(true, false, false, Some(0.25), None, &[], &[])
                .unwrap()
                .has_weather
        );
        assert!(
            !from_dimension_fields(true, true, false, Some(0.25), None, &[], &[])
                .unwrap()
                .has_weather
        );
        assert!(
            !from_dimension_fields(false, false, false, Some(0.25), None, &[], &[])
                .unwrap()
                .has_weather
        );
    }

    #[test]
    fn native_day_sky_light_track_reaches_typed_environment() {
        let frame = |ticks, value| {
            let mut nbt = NbtCompound::new();
            nbt.insert("ticks", ticks);
            nbt.insert("value", value);
            nbt
        };
        let mut track = NbtCompound::new();
        track.insert(
            "keyframes",
            NbtList::from(vec![
                frame(133, 1.0f32),
                frame(11_867, 1.0),
                frame(13_670, 0.26666668),
                frame(22_330, 0.26666668),
            ]),
        );
        track.insert("modifier", "multiply");
        let mut factor_track = NbtCompound::new();
        factor_track.insert(
            "keyframes",
            NbtList::from(vec![
                frame(730, 1.0f32),
                frame(11_270, 1.0),
                frame(13_140, 0.24),
                frame(22_860, 0.24),
            ]),
        );
        factor_track.insert("modifier", "multiply");
        let color_frame = |ticks, value| {
            let mut nbt = NbtCompound::new();
            nbt.insert("ticks", ticks);
            nbt.insert("value", value);
            nbt
        };
        let mut color_track = NbtCompound::new();
        color_track.insert(
            "keyframes",
            NbtList::from(vec![
                color_frame(730, 0x00ff_ffff),
                color_frame(11_270, 0x00ff_ffff),
                color_frame(13_140, 0x007a_7aff),
                color_frame(22_860, 0x007a_7aff),
            ]),
        );
        color_track.insert("modifier", "multiply");
        let mut tracks = NbtCompound::new();
        tracks.insert(SKY_LIGHT_LEVEL_ATTRIBUTE, track);
        tracks.insert(SKY_LIGHT_FACTOR_ATTRIBUTE, factor_track);
        tracks.insert(SKY_LIGHT_COLOR_ATTRIBUTE, color_track);
        let mut day = NbtCompound::new();
        day.insert("clock", "minecraft:overworld");
        day.insert("period_ticks", 24_000);
        day.insert("tracks", tracks);

        let input = DimensionEnvironmentInput {
            has_sky_light: true,
            has_ceiling: false,
            is_end_world: false,
            has_end_flashes: false,
            ambient_light: Some(0.0),
            sky_light_level: Some(15.0),
            lightmap_attributes: LightmapAttributes::default(),
            water_evaporates: Some(true),
            default_dripstone_particle: None,
            timeline_refs: vec!["#custom:daily".into()],
            timeline_entries: Arc::new(vec![("minecraft:day".into(), day)]),
            timeline_entries_error: None,
        };
        let tags = std::collections::HashMap::from([(
            "custom:daily".parse().unwrap(),
            vec!["minecraft:day".parse().unwrap()],
        )]);
        let resolved = resolve_dimension_environment(&input, &tags).unwrap();
        assert!(resolved.water_evaporates);
        let sky_track = &resolved.tracks[0];
        assert_eq!(sky_track.id, "minecraft:day");
        assert_eq!(sky_track.clock, "minecraft:overworld");
        assert_eq!(sky_track.tracks[0].0, SKY_LIGHT_LEVEL_ATTRIBUTE);
        assert_eq!(sky_track.tracks[0].1.period_ticks, Some(24_000));
        assert_eq!(sky_track.tracks[0].1.modifier.as_deref(), Some("multiply"));
        assert_eq!(
            sky_track.tracks[0]
                .1
                .keyframes
                .iter()
                .map(|frame| (frame.ticks, frame.value))
                .collect::<Vec<_>>(),
            [
                (133, 1.0),
                (11_867, 1.0),
                (13_670, 0.26666668),
                (22_330, 0.26666668),
            ]
        );
        assert_eq!(resolved.lightmap_timelines.len(), 1);
        for (ticks, expected_factor, expected_color) in
            [(6_000.0, 1.0, 0x00ff_ffff), (18_000.0, 0.24, 0x007a_7aff)]
        {
            let lightmap =
                evaluate_lightmap_attributes(&resolved, 0.0, 0.0, false, 0.0, false, |_| Ok(ticks))
                    .unwrap();
            assert!((lightmap.sky_light_factor - expected_factor).abs() < 1e-6);
            assert_eq!(
                lightmap.sky_light_color as u32 & 0x00ff_ffff,
                expected_color
            );
        }
        for (ticks, expected) in [
            (6_000, 15.0),
            (18_000, 4.0),
            (12_768, 9.503051),
            (0, 14.1886),
            (133, 15.0),
            (11_867, 15.0),
            (13_670, 4.0),
            (22_330, 4.0),
            (-1, 14.1825),
            ((1_i64 << 53) + 1, 8.1303),
        ] {
            let evaluation = evaluate_sky_light(&resolved, |_| Ok(ticks)).unwrap();
            assert!(
                (evaluation.sky_light_level - expected).abs() < 0.0001,
                "tick {ticks}: {evaluation:?}"
            );
            assert_eq!(evaluation.sky_darken, (15.0 - expected).trunc() as u8);
            assert_eq!(evaluation.ambient_light, 0.0);
        }
        let rain = DimensionEnvironment {
            rain_level: 1.0,
            ..resolved.clone()
        };
        let value = evaluate_sky_light(&rain, |_| Ok(6_000))
            .unwrap()
            .sky_light_level;
        assert!((value - 11.5625).abs() < 0.0001);
        let thunder = DimensionEnvironment {
            rain_level: 1.0,
            thunder_level: 1.0,
            ..resolved.clone()
        };
        let value = evaluate_sky_light(&thunder, |_| Ok(6_000))
            .unwrap()
            .sky_light_level;
        assert!((value - 9.199219).abs() < 0.0001);
        assert_eq!(
            evaluate_sky_light(&resolved, |_| Ok(6_000))
                .unwrap()
                .sky_light_level,
            15.0
        );
        let ambient =
            from_dimension_fields(true, false, false, Some(0.25), None, &[], &[]).unwrap();
        assert_eq!(
            evaluate_sky_light(&ambient, |_| Ok(0))
                .unwrap()
                .ambient_light,
            0.25
        );
        let unsupported = DimensionEnvironment {
            unsupported_reason: Some("unknown frame encoding".into()),
            ..resolved.clone()
        };
        assert!(
            evaluate_sky_light(&unsupported, |_| Ok(0))
                .unwrap_err()
                .contains("unknown frame encoding")
        );
        let missing = DimensionEnvironmentInput {
            timeline_refs: vec!["#custom:later".into()],
            ..input.clone()
        };
        assert!(
            resolve_dimension_environment(&missing, &tags)
                .unwrap_err()
                .contains("missing timeline tag")
        );
        let later_tags = std::collections::HashMap::from([(
            "custom:later".parse().unwrap(),
            vec!["minecraft:day".parse().unwrap()],
        )]);
        assert_eq!(
            resolve_dimension_environment(&missing, &later_tags)
                .unwrap()
                .tracks
                .len(),
            1
        );
    }

    #[test]
    fn ambient_timeline_uses_discrete_attribute_override_and_periodic_clock_sampling() {
        use simdnbt::owned::NbtTag;

        use crate::particle::ServerParticleKind;

        let particle_value = |probability| {
            let mut particle = NbtCompound::new();
            particle.insert("type", "minecraft:ash");
            let mut entry = NbtCompound::new();
            entry.insert("particle", particle);
            entry.insert("probability", probability);
            NbtTag::List(NbtList::Compound(vec![entry]))
        };
        let frame = |ticks, value| {
            let mut frame = NbtCompound::new();
            frame.insert("ticks", ticks);
            frame.insert("value", value);
            frame
        };
        let track = |frames| {
            let mut track = NbtCompound::new();
            track.insert("modifier", "override");
            track.insert("ease", "linear");
            track.insert("keyframes", NbtList::from(frames));
            track
        };
        let timeline = |keyframes| {
            let mut tracks = NbtCompound::new();
            tracks.insert(AMBIENT_PARTICLES_ATTRIBUTE, track(keyframes));
            let mut timeline = NbtCompound::new();
            timeline.insert("clock", "minecraft:overworld");
            timeline.insert("period_ticks", 100);
            timeline.insert("tracks", tracks);
            timeline
        };
        let empty = NbtTag::List(NbtList::Empty);
        let absent = from_dimension_fields(true, false, false, Some(0.0), None, &[], &[]).unwrap();
        assert!(
            evaluate_ambient_particles(&absent, |_, _, _| Ok(0))
                .unwrap()
                .is_none()
        );

        let wrap = timeline(vec![
            frame(20, empty.clone()),
            frame(80, particle_value(1.0f32)),
        ]);
        let resolved = from_dimension_fields(
            true,
            false,
            false,
            Some(0.0),
            None,
            &[("minecraft:wrap".into(), wrap)],
            &["minecraft:wrap".into()],
        )
        .unwrap();
        let get = |ticks| {
            evaluate_ambient_particles(&resolved, |id, clock, period| {
                assert_eq!(id, "minecraft:wrap");
                assert_eq!(clock, "minecraft:overworld");
                assert_eq!(period, Some(100));
                Ok(ticks)
            })
            .unwrap()
            .unwrap()
        };
        assert_eq!(get(19)[0].kind, ServerParticleKind::Ash);
        assert!(get(20).is_empty());
        assert!(
            get(50).is_empty(),
            "not-interpolated list holds the prior keyframe"
        );
        assert_eq!(get(80)[0].kind, ServerParticleKind::Ash);
        assert_eq!(get(99)[0].kind, ServerParticleKind::Ash);
        assert_eq!(
            get(100)[0].kind,
            ServerParticleKind::Ash,
            "period wrap maps tick 100 to tick 0"
        );
        assert_eq!(get(0)[0].kind, ServerParticleKind::Ash);

        let first = timeline(vec![frame(0, particle_value(1.0f32))]);
        let second = timeline(vec![frame(0, empty)]);
        let ordered = from_dimension_fields(
            true,
            false,
            false,
            Some(0.0),
            None,
            &[
                ("minecraft:first".into(), first),
                ("minecraft:second".into(), second),
            ],
            &["minecraft:first".into(), "minecraft:second".into()],
        )
        .unwrap();
        assert!(
            evaluate_ambient_particles(&ordered, |_, _, _| Ok(0))
                .unwrap()
                .unwrap()
                .is_empty(),
            "later timeline override wins, including an explicit empty list"
        );
    }

    #[test]
    fn ambient_timeline_rejects_invalid_codec_values_without_breaking_sky_tracks() {
        use simdnbt::owned::NbtTag;
        let mut bad_particle = NbtCompound::new();
        bad_particle.insert("type", "minecraft:ash");
        let mut bad_entry = NbtCompound::new();
        bad_entry.insert("particle", bad_particle);
        bad_entry.insert("probability", 1.5f32);
        let mut track = NbtCompound::new();
        track.insert(
            "keyframes",
            NbtList::from(vec![{
                let mut frame = NbtCompound::new();
                frame.insert("ticks", 0);
                frame.insert("value", NbtTag::List(NbtList::Compound(vec![bad_entry])));
                frame
            }]),
        );
        let mut tracks = NbtCompound::new();
        tracks.insert(AMBIENT_PARTICLES_ATTRIBUTE, track);
        let mut timeline = NbtCompound::new();
        timeline.insert("clock", "minecraft:overworld");
        timeline.insert("tracks", tracks);
        let resolved = from_dimension_fields(
            true,
            false,
            false,
            Some(0.0),
            None,
            &[("minecraft:bad".into(), timeline)],
            &["minecraft:bad".into()],
        )
        .unwrap();
        assert_eq!(resolved.sky_light_level, 15.0);
        assert!(
            evaluate_ambient_particles(&resolved, |_, _, _| Ok(0))
                .unwrap_err()
                .contains("keyframe value")
        );
    }

    #[test]
    fn timeline_track_is_bounded_finite_and_ordered() {
        let frame = |ticks, value| {
            let mut nbt = NbtCompound::new();
            nbt.insert("ticks", ticks);
            nbt.insert("value", value);
            nbt
        };
        let mut track = NbtCompound::new();
        track.insert(
            "keyframes",
            NbtList::from(vec![frame(0, 1.0f32), frame(10, 0.25f32)]),
        );
        assert_eq!(float_track(&track).unwrap().keyframes.len(), 2);
        let invalid_track = |keyframes| {
            let mut track = NbtCompound::new();
            track.insert("keyframes", NbtList::from(keyframes));
            track
        };
        assert!(float_track(&invalid_track(vec![frame(10, 1.0f32), frame(0, 0.25f32)])).is_none());
        assert!(float_track(&invalid_track(vec![frame(0, f32::NAN)])).is_none());
        assert!(
            float_track(&invalid_track(vec![
                frame(0, 1.0f32),
                frame(0, 0.5),
                frame(0, 0.25),
            ]))
            .is_none()
        );
    }
}
