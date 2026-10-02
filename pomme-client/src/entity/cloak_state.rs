use glam::DVec3;

/// Vanilla `ClientAvatarState.moveCloak` interpolation state, advanced at 20
/// Hz.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CapeMotionState {
    pub prev_walk_dist: f32,
    pub walk_dist: f32,
    pub prev_bob: f32,
    pub bob: f32,
    pub fall_flying_ticks: u32,
    pub was_fall_flying: bool,
    pub last_position: Option<DVec3>,
}

impl CapeMotionState {
    /// Remote Avatar movement uses the per-level-tick position delta for walked
    /// distance, while AbstractClientPlayer.updateBob smooths horizontal
    /// velocity.
    pub fn tick_remote(
        &mut self,
        position: DVec3,
        velocity: DVec3,
        on_ground: bool,
        dead: bool,
        swimming: bool,
        fall_flying: bool,
    ) {
        let delta = self.last_position.map(|last| position - last);
        if !position.is_finite()
            || !velocity.is_finite()
            || delta.is_some_and(|d| !d.is_finite() || d.abs().max_element() > 10.0)
        {
            self.reset();
            self.last_position = position.is_finite().then_some(position);
            return;
        }
        self.last_position = Some(position);
        self.prev_walk_dist = self.walk_dist;
        if let Some(delta) = delta {
            self.walk_dist += glam::DVec2::new(delta.x, delta.z).length() as f32 * 0.6;
        }
        self.prev_bob = self.bob;
        let target = if on_ground && !dead && !swimming {
            glam::DVec2::new(velocity.x, velocity.z).length().min(0.1) as f32
        } else {
            0.0
        };
        self.bob += (target - self.bob) * 0.4;
        self.advance_fall_flying(fall_flying);
    }

    pub fn tick_mannequin(&mut self, fall_flying: bool) {
        self.prev_walk_dist = self.walk_dist;
        self.prev_bob = self.bob;
        self.advance_fall_flying(fall_flying);
    }

    fn advance_fall_flying(&mut self, fall_flying: bool) {
        self.fall_flying_ticks = if fall_flying {
            if self.was_fall_flying {
                self.fall_flying_ticks.saturating_add(1)
            } else {
                1
            }
        } else {
            0
        };
        self.was_fall_flying = fall_flying;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CloakState {
    pub prev: DVec3,
    pub current: DVec3,
    pub initialized: bool,
}

impl CloakState {
    /// Invalid positions discard stale history; the next finite sample starts
    /// fresh.
    pub fn tick(&mut self, position: DVec3) {
        if !position.is_finite() {
            self.reset();
            return;
        }
        if !self.initialized {
            self.prev = position;
            self.current = position;
            self.initialized = true;
            return;
        }

        // Preserve native order: previous=current, compute all deltas from current,
        // then independently snap or advance each axis by delta * 0.25.
        self.prev = self.current;
        let delta = position - self.current;
        for axis in 0..3 {
            if delta[axis] > 10.0 || delta[axis] < -10.0 {
                self.current[axis] = position[axis];
                self.prev[axis] = position[axis];
            } else {
                self.current[axis] += delta[axis] * 0.25;
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Native render interpolation is lerp(previous, current, partial).
    pub fn render_lerp(&self, partial: f64) -> DVec3 {
        self.prev.lerp(self.current, partial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_walk_and_bob_use_native_delta_coefficient_and_smoothing() {
        let mut state = CapeMotionState::default();
        state.tick_remote(
            DVec3::ZERO,
            DVec3::new(0.2, 0.0, 0.0),
            true,
            false,
            false,
            false,
        );
        assert_eq!(state.walk_dist, 0.0);
        assert_eq!(state.bob, 0.04);
        state.tick_remote(
            DVec3::new(3.0, 8.0, 4.0),
            DVec3::new(0.2, 0.0, 0.0),
            true,
            false,
            false,
            false,
        );
        assert_eq!(state.prev_walk_dist, 0.0);
        assert!((state.walk_dist - 3.0).abs() < 1e-6);
        assert!((state.prev_bob - 0.04).abs() < 1e-6);
        assert!((state.bob - 0.064).abs() < 1e-6);
    }

    #[test]
    fn bob_requires_ground_alive_and_not_swimming_and_flight_ticks_reset_natively() {
        let mut state = CapeMotionState::default();
        for (ground, dead, swimming) in [
            (false, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            state.tick_remote(
                DVec3::ZERO,
                DVec3::new(1.0, 0.0, 0.0),
                ground,
                dead,
                swimming,
                false,
            );
            assert_eq!(state.bob, 0.0);
        }
        state.tick_remote(DVec3::ZERO, DVec3::ZERO, true, false, false, true);
        assert_eq!(state.fall_flying_ticks, 1);
        state.tick_remote(DVec3::ZERO, DVec3::ZERO, true, false, false, true);
        assert_eq!(state.fall_flying_ticks, 2);
        state.tick_remote(DVec3::ZERO, DVec3::ZERO, true, false, false, false);
        assert_eq!(state.fall_flying_ticks, 0);
        state.tick_remote(DVec3::ZERO, DVec3::ZERO, true, false, false, true);
        assert_eq!(state.fall_flying_ticks, 1);
    }

    #[test]
    fn teleport_nonfinite_reset_and_instances_do_not_share_cape_history() {
        let mut a = CapeMotionState::default();
        let mut b = CapeMotionState::default();
        a.tick_remote(DVec3::ZERO, DVec3::ZERO, true, false, false, true);
        a.tick_remote(DVec3::X, DVec3::ZERO, true, false, false, true);
        b.tick_remote(DVec3::splat(20.0), DVec3::ZERO, true, false, false, false);
        assert_eq!(a.walk_dist, 0.6);
        assert_eq!(b.walk_dist, 0.0);
        a.tick_remote(
            DVec3::new(20.0, 0.0, 0.0),
            DVec3::ZERO,
            true,
            false,
            false,
            true,
        );
        assert_eq!(a.walk_dist, 0.0);
        assert_eq!(a.fall_flying_ticks, 0);
        assert_eq!(b.fall_flying_ticks, 0);
        a.tick_remote(
            DVec3::new(f64::NAN, 0.0, 0.0),
            DVec3::ZERO,
            true,
            false,
            false,
            false,
        );
        assert_eq!(a, CapeMotionState::default());
    }

    #[test]
    fn native_quarter_step_axis_snap_and_inclusive_ten_threshold() {
        let mut state = CloakState::default();
        state.tick(DVec3::ZERO);
        state.tick(DVec3::new(4.0, 10.0, 10.0001));
        assert_eq!(state.current, DVec3::new(1.0, 2.5, 10.0001));
        assert_eq!(state.prev, DVec3::ZERO);
        state.tick(DVec3::new(-20.0, -20.0, -20.0));
        assert_eq!(state.current, DVec3::new(-20.0, -20.0, -20.0));
        assert_eq!(state.prev.x, -20.0);
        assert_eq!(state.prev.y, 2.5);
        assert_eq!(state.prev.z, -20.0);
    }

    #[test]
    fn render_endpoints_partial_values_and_nonfinite_reset() {
        let mut state = CloakState::default();
        state.tick(DVec3::ZERO);
        state.tick(DVec3::splat(4.0));
        assert_eq!(state.render_lerp(0.0), DVec3::ZERO);
        assert_eq!(state.render_lerp(1.0), DVec3::ONE);
        assert_eq!(state.render_lerp(0.5), DVec3::splat(0.5));
        state.tick(DVec3::new(f64::NAN, 1.0, 2.0));
        assert_eq!(state, CloakState::default());
    }

    #[test]
    fn reset_instances_and_twenty_hz_updates_are_independent_of_physics() {
        let mut a = CloakState::default();
        let mut b = CloakState::default();
        a.tick(DVec3::ZERO);
        b.tick(DVec3::splat(100.0));
        let before_physics = a;
        for _physics_step in 0..120 {
            // 120 Hz physics has no access to cloak state.
        }
        assert_eq!(a, before_physics);
        for tick in 1..=40 {
            a.tick(DVec3::splat(tick as f64));
        }
        let expected = 37.0 + 3.0 * 0.75_f64.powi(40);
        assert!((a.current.x - expected).abs() < 1e-12);
        assert_eq!(a.current.x, a.current.y);
        assert_eq!(a.current.y, a.current.z);
        assert_eq!(b.current, DVec3::splat(100.0));
        a.reset();
        assert_eq!(a, CloakState::default());
        assert!(b.initialized);
    }
}
