//! Typed ingress for the dimension environment inputs consumed by the renderer.
use std::sync::Arc;

use simdnbt::owned::{NbtCompound, NbtList};

pub const SKY_LIGHT_LEVEL: f32 = 15.0;
pub const SKY_LIGHT_LEVEL_ATTRIBUTE: &str = "minecraft:gameplay/sky_light_level";
pub const MAX_TIMELINES: usize = 4096;
const MAX_KEYFRAMES: usize = 4096;

pub type TimelineEntries = Arc<Vec<(String, NbtCompound)>>;

#[derive(Clone, Debug)]
pub struct DimensionEnvironmentInput {
    pub has_sky_light: bool,
    pub has_ceiling: bool,
    pub is_end_world: bool,
    pub ambient_light: Option<f32>,
    pub sky_light_level: Option<f32>,
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
    from_dimension_fields(
        input.has_sky_light,
        input.has_ceiling,
        input.is_end_world,
        input.ambient_light,
        input.sky_light_level,
        input.timeline_entries.as_slice(),
        &ids,
    )
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
pub struct TimelineInput {
    pub id: String,
    pub clock: String,
    pub tracks: Vec<(String, FloatTrack)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionEnvironment {
    pub has_sky_light: bool,
    pub has_weather: bool,
    pub ambient_light: f32,
    pub sky_light_level: f32,
    /// Nonempty when native environment data could not be represented. Safe
    /// fallback values remain usable, but are not claimed as valid ingress.
    pub unsupported_reason: Option<String>,
    pub timelines: Vec<String>,
    pub tracks: Vec<TimelineInput>,
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
            has_sky_light: true,
            has_weather: true,
            ambient_light: 0.0,
            sky_light_level: SKY_LIGHT_LEVEL,
            unsupported_reason: None,
            timelines: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

fn string(compound: &NbtCompound, key: &str) -> Option<String> {
    compound.string(key).map(|value| value.to_string())
}

fn float_track(compound: &NbtCompound) -> Option<FloatTrack> {
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

/// Extract only the float SKY_LIGHT_LEVEL data needed downstream; unrelated
/// attributes/tracks remain in the normal registry holder and are not copied.
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
        has_sky_light,
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
        let Some(track) = tracks.compound(SKY_LIGHT_LEVEL_ATTRIBUTE) else {
            continue;
        };
        let mut parsed =
            float_track(track).ok_or_else(|| format!("invalid SKY_LIGHT_LEVEL track: {id}"))?;
        parsed.period_ticks = nbt.int("period_ticks");
        if parsed.period_ticks.is_some_and(|period| {
            period <= 0 || parsed.keyframes.iter().any(|kf| kf.ticks > period)
        }) {
            return Err(format!("invalid SKY_LIGHT_LEVEL timeline period: {id}"));
        }
        let clock = string(nbt, "clock")
            .or_else(|| {
                nbt.compound("clock")
                    .and_then(|clock| string(clock, "value"))
            })
            .unwrap_or_default();
        result.tracks.push(TimelineInput {
            id: id.clone(),
            clock,
            tracks: vec![(SKY_LIGHT_LEVEL_ATTRIBUTE.into(), parsed)],
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!((reverse.total_ticks, reverse.partial_tick), (0, 0.5));
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
        let mut tracks = NbtCompound::new();
        tracks.insert(SKY_LIGHT_LEVEL_ATTRIBUTE, track);
        let mut day = NbtCompound::new();
        day.insert("clock", "minecraft:overworld");
        day.insert("period_ticks", 24_000);
        day.insert("tracks", tracks);

        let input = DimensionEnvironmentInput {
            has_sky_light: true,
            has_ceiling: false,
            is_end_world: false,
            ambient_light: Some(0.0),
            sky_light_level: Some(15.0),
            timeline_refs: vec!["#custom:daily".into()],
            timeline_entries: Arc::new(vec![("minecraft:day".into(), day)]),
            timeline_entries_error: None,
        };
        let tags = std::collections::HashMap::from([(
            "custom:daily".parse().unwrap(),
            vec!["minecraft:day".parse().unwrap()],
        )]);
        let resolved = resolve_dimension_environment(&input, &tags).unwrap();
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
        track.insert(
            "keyframes",
            NbtList::from(vec![frame(10, 1.0f32), frame(0, 0.25f32)]),
        );
        assert!(float_track(&track).is_none());
        track.insert("keyframes", NbtList::from(vec![frame(0, f32::NAN)]));
        assert!(float_track(&track).is_none());
        track.insert(
            "keyframes",
            NbtList::from(vec![frame(0, 1.0f32), frame(0, 0.5), frame(0, 0.25)]),
        );
        assert!(float_track(&track).is_none());
    }
}
