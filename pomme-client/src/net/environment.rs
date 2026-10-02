//! Typed ingress for the dimension environment inputs consumed by the renderer.
use simdnbt::owned::{NbtCompound, NbtList};

pub const SKY_LIGHT_LEVEL: f32 = 15.0;
const MAX_TIMELINES: usize = 4096;
const MAX_KEYFRAMES: usize = 4096;

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
    pub timelines: Vec<String>,
    pub tracks: Vec<TimelineInput>,
}

impl Default for DimensionEnvironment {
    fn default() -> Self {
        Self {
            has_sky_light: true,
            has_weather: true,
            ambient_light: 0.0,
            sky_light_level: SKY_LIGHT_LEVEL,
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
    ambient_light: Option<f32>,
    sky_light_level: Option<f32>,
    timeline_registry: &[(String, NbtCompound)],
    dimension_timeline_ids: &[String],
) -> Result<DimensionEnvironment, String> {
    let mut result = DimensionEnvironment {
        has_sky_light,
        has_weather: has_sky_light && !has_ceiling,
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
    for (id, nbt) in timeline_registry {
        if !result.timelines.iter().any(|used| used == id) {
            continue;
        }
        let Some(tracks) = nbt.compound("tracks") else {
            continue;
        };
        let Some(track) = tracks.compound("minecraft:sky_light_level") else {
            continue;
        };
        if let Some(mut parsed) = float_track(track) {
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
                tracks: vec![("minecraft:sky_light_level".into(), parsed)],
            });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_defaults_and_trust_boundary_ranges() {
        let defaults = from_dimension_fields(true, false, Some(0.0), None, &[], &[]).unwrap();
        assert_eq!(defaults.sky_light_level, 15.0);
        assert!(defaults.has_weather);
        assert!(from_dimension_fields(true, false, Some(f32::NAN), None, &[], &[]).is_err());
        assert!(
            from_dimension_fields(true, false, Some(0.0), Some(f32::INFINITY), &[], &[]).is_err()
        );
        assert!(from_dimension_fields(true, false, Some(0.0), Some(16.0), &[], &[]).is_err());
        assert!(
            from_dimension_fields(
                true,
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
