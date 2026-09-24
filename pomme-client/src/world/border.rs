/// Client-side world-border state. Network/app code owns packet decoding and
/// should apply decoded border updates here. This state is not an authority for
/// server-side movement or interaction constraints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderStatus {
    Growing,
    Shrinking,
    Stationary,
}

impl BorderStatus {
    pub const ALL: [Self; 3] = [Self::Stationary, Self::Growing, Self::Shrinking];

    /// Official 26.2 `BorderStatus.getColor()` RGB bytes (from common-jar
    /// bytecode).
    pub const fn color(self) -> [u8; 3] {
        match self {
            Self::Growing => [0x40, 0xFF, 0x80],
            Self::Shrinking => [0xFF, 0x30, 0x30],
            Self::Stationary => [0x20, 0xA0, 0xFF],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldBorder {
    center: [f64; 2],
    size: f64,
    previous_size: f64,
    lerp_from: f64,
    target_size: f64,
    lerp_duration: u64,
    lerp_elapsed: u64,
    absolute_max_size: i32,
    warning_blocks: i32,
    warning_time: i32,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            center: [0.0; 2],
            size: 59_999_968.0,
            previous_size: 59_999_968.0,
            lerp_from: 59_999_968.0,
            target_size: 59_999_968.0,
            lerp_duration: 0,
            lerp_elapsed: 0,
            absolute_max_size: 29_999_984,
            warning_blocks: 5,
            warning_time: 15,
        }
    }
}

impl WorldBorder {
    pub fn center(&self) -> [f64; 2] {
        self.center
    }

    pub fn set_center(&mut self, x: f64, z: f64) {
        self.center = [x, z];
    }

    /// Current size. During a lerp this is the most recently advanced tick's
    /// size; use `size_at` to interpolate between ticks for rendering.
    pub fn size(&self) -> f64 {
        self.size
    }

    pub fn target_size(&self) -> f64 {
        self.target_size
    }

    /// Status derived from the active lerp endpoints and remaining ticks.
    /// Matches 26.2: equal endpoints are static; a finished lerp is stationary.
    pub fn status(&self) -> BorderStatus {
        if self.lerp_elapsed >= self.lerp_duration {
            return BorderStatus::Stationary;
        }
        match self.target_size.total_cmp(&self.lerp_from) {
            std::cmp::Ordering::Less => BorderStatus::Shrinking,
            std::cmp::Ordering::Greater => BorderStatus::Growing,
            std::cmp::Ordering::Equal => BorderStatus::Stationary,
        }
    }

    pub fn set_size(&mut self, size: f64) {
        self.size = size;
        self.previous_size = size;
        self.lerp_from = size;
        self.target_size = size;
        self.lerp_duration = 0;
        self.lerp_elapsed = 0;
    }

    /// Starts a border-size interpolation from the packet-specified old size.
    /// `duration_ticks` is elapsed world-border updates, not wall-clock time.
    pub fn lerp_size_between(&mut self, old_size: f64, new_size: f64, duration_ticks: i64) {
        if duration_ticks <= 0 || old_size == new_size {
            self.set_size(new_size);
            return;
        }
        self.size = old_size;
        self.previous_size = old_size;
        self.lerp_from = old_size;
        self.target_size = new_size;
        self.lerp_duration = duration_ticks as u64;
        self.lerp_elapsed = 0;
    }

    /// Advances one WorldBorder update. Vanilla 26.2 advances lerp progress by
    /// one on each update; callers must not advance this from a render frame.
    pub fn tick(&mut self) {
        if self.lerp_elapsed >= self.lerp_duration {
            return;
        }
        self.previous_size = self.size;
        self.lerp_elapsed += 1;
        let progress = self.lerp_elapsed as f64 / self.lerp_duration as f64;
        self.size = self.lerp_from + (self.target_size - self.lerp_from) * progress;
        if self.lerp_elapsed == self.lerp_duration {
            self.size = self.target_size;
            self.previous_size = self.target_size;
        }
    }

    /// Size interpolated between the previous and current border updates.
    /// Mirrors the render partial-tick interpolation in 26.2's
    /// MovingBorderExtent.
    pub fn size_at(&self, partial_tick: f32) -> f64 {
        let t = f64::from(partial_tick.clamp(0.0, 1.0));
        self.previous_size + (self.size - self.previous_size) * t
    }

    /// Applies the full state carried by 26.2's InitializeBorder packet.
    pub fn initialize(
        &mut self,
        center_x: f64,
        center_z: f64,
        old_size: f64,
        new_size: f64,
        lerp_time: i64,
        absolute_max_size: i32,
        warning_blocks: i32,
        warning_time: i32,
    ) {
        self.set_center(center_x, center_z);
        self.lerp_size_between(old_size, new_size, lerp_time);
        self.set_absolute_max_size(absolute_max_size);
        self.set_warning_blocks(warning_blocks);
        self.set_warning_time(warning_time);
    }

    pub fn absolute_max_size(&self) -> i32 {
        self.absolute_max_size
    }

    pub fn set_absolute_max_size(&mut self, size: i32) {
        self.absolute_max_size = size.max(0);
    }

    pub fn warning_blocks(&self) -> i32 {
        self.warning_blocks
    }

    pub fn set_warning_blocks(&mut self, blocks: i32) {
        self.warning_blocks = blocks;
    }

    pub fn warning_time(&self) -> i32 {
        self.warning_time
    }

    pub fn set_warning_time(&mut self, ticks: i32) {
        self.warning_time = ticks;
    }

    /// Clamped extents ordered `[min_x, max_x, min_z, max_z]`.
    pub fn bounds_at(&self, partial_tick: f32) -> [f64; 4] {
        let half_size = self.size_at(partial_tick) * 0.5;
        let max = f64::from(self.absolute_max_size);
        [
            (self.center[0] - half_size).clamp(-max, max),
            (self.center[0] + half_size).clamp(-max, max),
            (self.center[1] - half_size).clamp(-max, max),
            (self.center[1] + half_size).clamp(-max, max),
        ]
    }

    /// Tests a point against the current half-open XZ border bounds.
    pub fn contains(&self, x: f64, z: f64) -> bool {
        let [min_x, max_x, min_z, max_z] = self.bounds_at(1.0);
        x >= min_x && x < max_x && z >= min_z && z < max_z
    }

    /// Whether the point is within `distance` of any current border edge.
    pub fn visible_from(&self, x: f64, z: f64, distance: f64) -> bool {
        let [min_x, max_x, min_z, max_z] = self.bounds_at(1.0);
        (x - min_x).abs() <= distance
            || (max_x - x).abs() <= distance
            || (z - min_z).abs() <= distance
            || (max_z - z).abs() <= distance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_uses_packet_old_size_and_completes_after_ticks() {
        let mut border = WorldBorder::default();
        border.set_size(40.0);
        border.lerp_size_between(20.0, 10.0, 2);
        assert_eq!(border.size(), 20.0);
        assert_eq!(border.target_size(), 10.0);
        border.tick();
        assert_eq!(border.size(), 15.0);
        assert_eq!(border.size_at(0.5), 17.5);
        border.tick();
        assert_eq!(border.size(), 10.0);
        assert_eq!(border.size_at(0.5), 10.0);
        border.tick();
        assert_eq!(border.size(), 10.0);
    }

    #[test]
    fn non_positive_duration_sets_new_size_immediately() {
        let mut border = WorldBorder::default();
        border.lerp_size_between(20.0, 10.0, 0);
        assert_eq!(border.size(), 10.0);
        assert_eq!(border.target_size(), 10.0);
        border.lerp_size_between(20.0, 5.0, -1);
        assert_eq!(border.size(), 5.0);
    }

    #[test]
    fn negative_center_and_absolute_max_clamp_each_extent() {
        let mut border = WorldBorder::default();
        border.set_center(-20.0, -10.0);
        border.set_size(40.0);
        border.set_absolute_max_size(25);
        assert_eq!(border.bounds_at(1.0), [-25.0, 0.0, -25.0, 10.0]);
        assert!(border.contains(-20.0, -10.0));
        assert!(!border.contains(0.0, -10.0));
    }

    #[test]
    fn negative_absolute_max_is_sanitized_to_zero_bounds() {
        let mut border = WorldBorder::default();
        border.set_absolute_max_size(-1);
        assert_eq!(border.bounds_at(1.0), [0.0; 4]);
    }

    #[test]
    fn initialize_sets_lerp_and_all_packet_metadata() {
        let mut border = WorldBorder::default();
        border.initialize(8.0, -4.0, 40.0, 20.0, 2, 30, 7, 90);
        assert_eq!(border.center(), [8.0, -4.0]);
        assert_eq!(border.size(), 40.0);
        assert_eq!(border.target_size(), 20.0);
        assert_eq!(border.absolute_max_size(), 30);
        assert_eq!(border.warning_blocks(), 7);
        assert_eq!(border.warning_time(), 90);
        border.tick();
        border.tick();
        assert_eq!(border.size(), 20.0);
    }

    #[test]
    fn status_tracks_lerp_direction_and_returns_to_stationary() {
        let mut border = WorldBorder::default();
        assert_eq!(border.status(), BorderStatus::Stationary);
        border.lerp_size_between(20.0, 30.0, 2);
        assert_eq!(border.status(), BorderStatus::Growing);
        border.tick();
        assert_eq!(border.status(), BorderStatus::Growing);
        border.tick();
        assert_eq!(border.status(), BorderStatus::Stationary);

        border.lerp_size_between(30.0, 10.0, 2);
        assert_eq!(border.status(), BorderStatus::Shrinking);
        border.set_size(10.0);
        assert_eq!(border.status(), BorderStatus::Stationary);
    }

    #[test]
    fn equal_lerp_endpoints_are_officially_stationary_immediately() {
        let mut border = WorldBorder::default();
        border.lerp_size_between(12.0, 12.0, 100);
        assert_eq!(border.status(), BorderStatus::Stationary);
        assert_eq!(border.size(), 12.0);
        assert_eq!(border.target_size(), 12.0);
        assert_eq!(border.size_at(0.5), 12.0);
    }

    #[test]
    fn official_status_colors_match_26_2_common_bytecode() {
        assert_eq!(BorderStatus::Growing.color(), [0x40, 0xFF, 0x80]);
        assert_eq!(BorderStatus::Shrinking.color(), [0xFF, 0x30, 0x30]);
        assert_eq!(BorderStatus::Stationary.color(), [0x20, 0xA0, 0xFF]);
    }

    #[test]
    fn warnings_are_stored_and_updated_independently() {
        let mut border = WorldBorder::default();
        border.set_warning_blocks(12);
        border.set_warning_time(300);
        assert_eq!(border.warning_blocks(), 12);
        assert_eq!(border.warning_time(), 300);
    }
}
