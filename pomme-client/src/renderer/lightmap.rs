//! Java 26.2 `Lightmap`'s 16x16 RGBA8 LUT, uploaded in `CameraUniform`.

pub const LEVELS: usize = 16;
pub const LUT_LEN: usize = LEVELS * LEVELS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub sky_factor: f32,
    pub block_factor: f32,
    pub night_vision_factor: f32,
    pub darkness_scale: f32,
    pub boss_overlay_world_darkening: f32,
    pub brightness: f32,
    pub block_light_tint: [f32; 3],
    pub sky_light_color: [f32; 3],
    pub ambient_color: [f32; 3],
    pub night_vision_color: [f32; 3],
}

impl Default for Settings {
    fn default() -> Self {
        // Java EnvironmentAttributes defaults and Options.gamma() default 0.5,
        // DarknessEffectScale default 1 (factor is zero without Darkness).
        Self {
            sky_factor: 1.0,
            block_factor: 1.4,
            night_vision_factor: 0.0,
            darkness_scale: 0.0,
            boss_overlay_world_darkening: 0.0,
            brightness: 0.5,
            block_light_tint: [1.0, 216.0 / 255.0, 140.0 / 255.0],
            sky_light_color: [1.0; 3],
            ambient_color: [0.0; 3],
            night_vision_color: [153.0 / 255.0; 3],
        }
    }
}

pub type Lut = [[f32; 4]; LUT_LEN];

fn srgb_unorm(value: f32) -> f32 {
    (value.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

fn not_gamma(color: [f32; 3]) -> [f32; 3] {
    let max = color[0].max(color[1]).max(color[2]);
    if max <= 0.0 {
        return [0.0; 3];
    }
    let scaled = 1.0 - (1.0 - max).powi(4);
    color.map(|channel| channel * scaled / max)
}

/// Generates the Java shader's block-x / sky-y lightmap, including RGBA8_UNORM
/// attachment quantization before the particle shader samples it.
pub fn generate(settings: Settings) -> Lut {
    std::array::from_fn(|index| {
        let block = index % LEVELS;
        let sky = index / LEVELS;
        let block_level = block as f32 / 15.0;
        let sky_level = sky as f32 / 15.0;
        let brightness = |level: f32| level / (4.0 - 3.0 * level);
        let parabolic = (2.0 * block_level - 1.0).powi(2);
        let block_tint: [f32; 3] = std::array::from_fn(|channel| {
            settings.block_light_tint[channel]
                + (1.0 - settings.block_light_tint[channel]) * (0.9 * parabolic)
        });
        let mut color = std::array::from_fn(|channel| {
            settings.ambient_color[channel]
                .max(settings.night_vision_color[channel] * settings.night_vision_factor)
                + settings.sky_light_color[channel] * (brightness(sky_level) * settings.sky_factor)
                + block_tint[channel] * (brightness(block_level) * settings.block_factor)
        });
        color = std::array::from_fn(|channel| {
            let darkened = color[channel] * (1.0 - settings.boss_overlay_world_darkening)
                + color[channel] * [0.7, 0.6, 0.6][channel] * settings.boss_overlay_world_darkening;
            darkened - settings.darkness_scale
        });
        color = color.map(|c| c.clamp(0.0, 1.0));
        let gamma = not_gamma(color);
        color = std::array::from_fn(|i| {
            srgb_unorm(color[i] + (gamma[i] - color[i]) * settings.brightness)
        });
        [color[0], color[1], color[2], 1.0]
    })
}

/// Java `sample_lightmap`'s linear, clamp-to-edge sample. UV coordinates are
/// in byte units (ordinary nibble lights are multiples of 16).
#[cfg(test)]
fn sample(lut: &Lut, block_uv: u8, sky_uv: u8) -> [f32; 4] {
    let x = (block_uv as f32 / 16.0).clamp(0.0, 15.0);
    let y = (sky_uv as f32 / 16.0).clamp(0.0, 15.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(15);
    let y1 = (y0 + 1).min(15);
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;
    std::array::from_fn(|c| {
        let top = lut[y0 * 16 + x0][c] * (1.0 - tx) + lut[y0 * 16 + x1][c] * tx;
        let bottom = lut[y1 * 16 + x0][c] * (1.0 - tx) + lut[y1 * 16 + x1][c] * tx;
        top * (1.0 - ty) + bottom * ty
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_lut_representative_dark_day_torch_fullbright_and_gamma_values() {
        let base = Settings::default();
        let lut = generate(base);
        assert_eq!(lut[0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(lut[15 * 16], [1.0; 4]); // sky 15, block 0
        assert_eq!(
            lut[4 * 16 + 8],
            [161.0 / 255.0, 141.0 / 255.0, 104.0 / 255.0, 1.0],
        ); // Java default tint/factors/gamma, RGBA8 quantization
        assert_eq!(lut[15 * 16 + 15], [1.0; 4]); // fullbright

        let night = generate(Settings {
            sky_factor: 0.0,
            ..base
        });
        assert_eq!(night[15 * 16], lut[0]);
        let gamma = generate(Settings {
            brightness: 1.0,
            ..base
        });
        assert!(gamma[3 * 16][0] > lut[3 * 16][0]);
        let vision_settings = Settings {
            night_vision_factor: 1.0,
            ..base
        };
        let vision = generate(vision_settings);
        assert!(vision[0][0] > lut[0][0]);
        let dark = generate(Settings {
            darkness_scale: 0.3,
            ..vision_settings
        });
        assert!(dark[0][0] < vision[0][0]);
    }

    #[test]
    fn rgba8_quantization_and_fractional_uv_match_linear_lightmap_sampling() {
        let lut = generate(Settings::default());
        assert_eq!(
            lut[4 * 16 + 8][0] * 255.0,
            (lut[4 * 16 + 8][0] * 255.0).round()
        );
        let a = sample(&lut, 8 * 16, 4 * 16);
        let b = sample(&lut, 9 * 16, 4 * 16);
        let mid = sample(&lut, 8 * 16 + 8, 4 * 16);
        for c in 0..4 {
            assert!((mid[c] - (a[c] + b[c]) * 0.5).abs() < 1e-6);
        }
        assert_eq!(sample(&lut, 255, 255), lut[255]);
    }
}
