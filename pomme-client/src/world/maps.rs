use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct MapData {
    pub colors: Vec<u8>,
    pub decorations: Vec<MapDecoration>,
    pub scale: u8,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapDecorationAsset {
    Player,
    Frame,
    RedMarker,
    BlueMarker,
    TargetX,
    TargetPoint,
    PlayerOffMap,
    PlayerOffLimits,
    WoodlandMansion,
    OceanMonument,
    WhiteBanner,
    OrangeBanner,
    MagentaBanner,
    LightBlueBanner,
    YellowBanner,
    LimeBanner,
    PinkBanner,
    GrayBanner,
    LightGrayBanner,
    CyanBanner,
    PurpleBanner,
    BlueBanner,
    BrownBanner,
    GreenBanner,
    RedBanner,
    BlackBanner,
    RedX,
    DesertVillage,
    PlainsVillage,
    SavannaVillage,
    SnowyVillage,
    TaigaVillage,
    JungleTemple,
    SwampHut,
    TrialChambers,
    Unknown(u32),
}

impl MapDecorationAsset {
    /// MapDecorationType registry order and asset IDs from Minecraft 26.2.
    pub const fn from_registry_id(id: u32) -> Self {
        use MapDecorationAsset::*;
        match id {
            0 => Player,
            1 => Frame,
            2 => RedMarker,
            3 => BlueMarker,
            4 => TargetX,
            5 => TargetPoint,
            6 => PlayerOffMap,
            7 => PlayerOffLimits,
            8 => WoodlandMansion,
            9 => OceanMonument,
            10 => WhiteBanner,
            11 => OrangeBanner,
            12 => MagentaBanner,
            13 => LightBlueBanner,
            14 => YellowBanner,
            15 => LimeBanner,
            16 => PinkBanner,
            17 => GrayBanner,
            18 => LightGrayBanner,
            19 => CyanBanner,
            20 => PurpleBanner,
            21 => BlueBanner,
            22 => BrownBanner,
            23 => GreenBanner,
            24 => RedBanner,
            25 => BlackBanner,
            26 => RedX,
            27 => DesertVillage,
            28 => PlainsVillage,
            29 => SavannaVillage,
            30 => SnowyVillage,
            31 => TaigaVillage,
            32 => JungleTemple,
            33 => SwampHut,
            34 => TrialChambers,
            other => Unknown(other),
        }
    }

    pub const fn asset_key(self) -> &'static str {
        use MapDecorationAsset::*;
        match self {
            Player => "player",
            Frame => "frame",
            RedMarker => "red_marker",
            BlueMarker => "blue_marker",
            TargetX => "target_x",
            TargetPoint => "target_point",
            PlayerOffMap => "player_off_map",
            PlayerOffLimits => "player_off_limits",
            WoodlandMansion => "woodland_mansion",
            OceanMonument => "ocean_monument",
            WhiteBanner => "white_banner",
            OrangeBanner => "orange_banner",
            MagentaBanner => "magenta_banner",
            LightBlueBanner => "light_blue_banner",
            YellowBanner => "yellow_banner",
            LimeBanner => "lime_banner",
            PinkBanner => "pink_banner",
            GrayBanner => "gray_banner",
            LightGrayBanner => "light_gray_banner",
            CyanBanner => "cyan_banner",
            PurpleBanner => "purple_banner",
            BlueBanner => "blue_banner",
            BrownBanner => "brown_banner",
            GreenBanner => "green_banner",
            RedBanner => "red_banner",
            BlackBanner => "black_banner",
            RedX => "red_x",
            DesertVillage => "desert_village",
            PlainsVillage => "plains_village",
            SavannaVillage => "savanna_village",
            SnowyVillage => "snowy_village",
            TaigaVillage => "taiga_village",
            JungleTemple => "jungle_temple",
            SwampHut => "swamp_hut",
            TrialChambers => "trial_chambers",
            Unknown(_) => "unknown",
        }
    }

    pub const fn show_on_item_frame(self) -> bool {
        !matches!(
            self,
            Self::Player
                | Self::RedMarker
                | Self::BlueMarker
                | Self::PlayerOffMap
                | Self::PlayerOffLimits
                | Self::Unknown(_)
        )
    }
}

#[derive(Clone, Debug)]
pub struct MapDecoration {
    pub asset: MapDecorationAsset,
    pub x: i8,
    pub y: i8,
    pub rotation: i8,
    pub name: Option<String>,
    pub show_on_item_frame: bool,
}

#[derive(Default)]
pub struct MapStore(pub HashMap<u32, MapData>);

impl MapStore {
    /// Apply a protocol patch only when its dimensions, indices and data length
    /// are consistent with the 128x128 map canvas.
    pub fn apply(
        &mut self,
        id: u32,
        scale: u8,
        locked: bool,
        patch: Option<(u8, u8, u8, u8, Vec<u8>)>,
        decorations: Option<Vec<MapDecoration>>,
    ) {
        let map = self.0.entry(id).or_insert_with(|| MapData {
            scale,
            locked,
            ..MapData::default()
        });
        if let Some((width, height, x, y, pixels)) = patch {
            let (w, h, x, y) = (width as usize, height as usize, x as usize, y as usize);
            if w > 0
                && h > 0
                && w <= 128
                && h <= 128
                && x <= 128 - w
                && y <= 128 - h
                && pixels.len() == w * h
            {
                if map.colors.len() != 128 * 128 {
                    map.colors = vec![0; 128 * 128];
                }
                for row in 0..h {
                    let dst = (y + row) * 128 + x;
                    map.colors[dst..dst + w].copy_from_slice(&pixels[row * w..(row + 1) * w]);
                }
            }
        }
        if let Some(decorations) = decorations {
            map.decorations = decorations;
        }
    }
}

/// Minecraft's 64 base map colors, with four brightness shades per color.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_update_only_valid_dimensions_and_empty_patches_preserve_pixels() {
        let mut maps = MapStore::default();
        maps.apply(7, 0, false, Some((2, 2, 3, 4, vec![1, 2, 3, 4])), None);
        let map = &maps.0[&7];
        assert_eq!(map.colors.len(), 128 * 128);
        assert_eq!(&map.colors[4 * 128 + 3..4 * 128 + 5], &[1, 2]);
        assert_eq!(&map.colors[5 * 128 + 3..5 * 128 + 5], &[3, 4]);

        maps.apply(7, 1, true, Some((0, 0, 0, 0, Vec::new())), None);
        assert_eq!(maps.0[&7].colors[4 * 128 + 3], 1);
        assert_eq!(maps.0[&7].scale, 0);
        assert!(!maps.0[&7].locked);
        maps.apply(7, 1, true, Some((2, 2, 127, 127, vec![9; 4])), None);
        assert_eq!(maps.0[&7].colors[4 * 128 + 3], 1);
    }

    #[test]
    fn decoration_registry_ids_keep_vanilla_sprite_assets_and_frame_visibility() {
        assert_eq!(
            MapDecorationAsset::from_registry_id(8).asset_key(),
            "woodland_mansion"
        );
        assert_eq!(
            MapDecorationAsset::from_registry_id(27).asset_key(),
            "desert_village"
        );
        assert!(!MapDecorationAsset::Player.show_on_item_frame());
        assert!(MapDecorationAsset::Frame.show_on_item_frame());
    }
}

pub fn palette(index: u8) -> [f32; 4] {
    const COLORS: [[u8; 3]; 64] = [
        [0, 0, 0],
        [127, 178, 56],
        [247, 233, 163],
        [199, 199, 199],
        [255, 0, 0],
        [160, 160, 255],
        [167, 167, 167],
        [0, 124, 0],
        [255, 255, 255],
        [164, 168, 184],
        [151, 109, 77],
        [112, 112, 112],
        [64, 64, 255],
        [143, 119, 72],
        [255, 252, 245],
        [216, 127, 51],
        [178, 76, 216],
        [102, 153, 216],
        [229, 229, 51],
        [127, 204, 25],
        [242, 127, 165],
        [76, 76, 76],
        [153, 153, 153],
        [76, 127, 153],
        [127, 63, 178],
        [51, 76, 178],
        [102, 76, 51],
        [102, 127, 51],
        [153, 51, 51],
        [25, 25, 25],
        [250, 238, 77],
        [92, 219, 213],
        [74, 128, 255],
        [0, 217, 58],
        [129, 86, 49],
        [112, 2, 0],
        [209, 177, 161],
        [159, 82, 36],
        [149, 87, 108],
        [112, 108, 138],
        [186, 133, 36],
        [103, 117, 53],
        [160, 77, 78],
        [57, 41, 35],
        [135, 107, 98],
        [87, 92, 92],
        [122, 73, 88],
        [76, 62, 92],
        [76, 50, 35],
        [76, 82, 42],
        [142, 60, 46],
        [37, 22, 16],
        [189, 48, 49],
        [148, 63, 97],
        [92, 25, 29],
        [22, 126, 134],
        [58, 142, 140],
        [86, 44, 62],
        [20, 180, 133],
        [100, 100, 100],
        [216, 175, 147],
        [127, 167, 150],
        [35, 118, 145],
        [133, 33, 34],
    ];
    let base = index as usize / 4;
    let shade = match index & 3 {
        0 => 180.0,
        1 => 220.0,
        2 => 255.0,
        _ => 135.0,
    };
    let c = COLORS[base];
    [
        (c[0] as f32 * shade / 255.0) / 255.0,
        (c[1] as f32 * shade / 255.0) / 255.0,
        (c[2] as f32 * shade / 255.0) / 255.0,
        1.0,
    ]
}
