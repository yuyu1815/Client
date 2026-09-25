use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct MapData {
    pub colors: Vec<u8>,
    pub decorations: Vec<MapDecoration>,
    pub scale: u8,
    pub locked: bool,
}

#[derive(Clone, Debug)]
pub struct MapDecoration {
    pub kind: String,
    pub x: i8,
    pub y: i8,
    pub rotation: i8,
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
        let map = self.0.entry(id).or_default();
        map.scale = scale;
        map.locked = locked;
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
        maps.apply(7, 1, true, Some((2, 2, 127, 127, vec![9; 4])), None);
        assert_eq!(maps.0[&7].colors[4 * 128 + 3], 1);
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
