//! Motion: easing curves, durations and the reduced-motion switch.
//!
//! Every animation in the game goes through this module, so that
//! **"reduce motion" is a single flag** rather than a hunt through the
//! drawing code.

/// An easing curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Easing {
    /// No easing.
    Linear,
    /// Fast start, gentle stop. The default for UI feedback.
    OutCubic,
    /// Slow start and stop.
    InOutCubic,
    /// Even snappier stop.
    OutQuint,
    /// Overshoots slightly and settles.
    OutBack,
    /// Very fast start, very gentle stop.
    OutExpo,
}

impl Easing {
    /// Evaluate the curve at `t` in `0..=1`.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::OutCubic => 1.0 - (1.0 - t).powi(3),
            Easing::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::OutQuint => 1.0 - (1.0 - t).powi(5),
            Easing::OutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                let u = t - 1.0;
                1.0 + C3 * u * u * u + C1 * u * u
            }
            Easing::OutExpo => {
                if t >= 1.0 {
                    1.0
                } else {
                    1.0 - (-10.0 * t).exp()
                }
            }
        }
    }
}

/// A duration step, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Duration {
    /// No animation.
    #[default]
    Instant,
    /// Hover and press feedback.
    Fast,
    /// The default for most transitions.
    Normal,
    /// Panel and screen transitions.
    Slow,
    /// The elimination animation.
    Slower,
}

impl Duration {
    /// The duration in milliseconds.
    pub const fn ms(self) -> u32 {
        match self {
            Duration::Instant => 0,
            Duration::Fast => 120,
            Duration::Normal => 200,
            Duration::Slow => 320,
            Duration::Slower => 480,
        }
    }
}

/// The motion system.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Motion {
    /// When `true`, every duration collapses to zero.
    pub reduced: bool,
}

impl Motion {
    /// A motion system honouring the user's preference.
    pub fn new(reduced: bool) -> Self {
        Motion { reduced }
    }

    /// The effective duration, in milliseconds.
    pub fn duration(&self, d: Duration) -> u32 {
        if self.reduced {
            0
        } else {
            d.ms()
        }
    }

    /// The progress of an animation at `elapsed` milliseconds.
    ///
    /// With reduced motion the animation is either finished or not started,
    /// never half-way.
    pub fn progress(&self, elapsed_ms: u32, d: Duration, easing: Easing) -> f32 {
        let total = self.duration(d);
        if total == 0 {
            return 1.0;
        }
        let t = (elapsed_ms as f32 / total as f32).clamp(0.0, 1.0);
        easing.apply(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_curve_is_anchored_at_the_ends() {
        for e in [
            Easing::Linear,
            Easing::OutCubic,
            Easing::InOutCubic,
            Easing::OutQuint,
            Easing::OutBack,
            Easing::OutExpo,
        ] {
            assert!((e.apply(0.0)).abs() < 1e-4, "{e:?} at 0");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-3, "{e:?} at 1");
        }
    }

    #[test]
    fn the_curves_are_monotonic_where_they_should_be() {
        // OutBack deliberately overshoots, so it is excluded.
        for e in [
            Easing::Linear,
            Easing::OutCubic,
            Easing::InOutCubic,
            Easing::OutQuint,
        ] {
            let mut last = -1.0;
            for i in 0..=20 {
                let v = e.apply(i as f32 / 20.0);
                assert!(v >= last - 1e-5, "{e:?} went backwards");
                last = v;
            }
        }
    }

    #[test]
    fn out_back_overshoots() {
        let mut max = 0.0f32;
        for i in 0..=100 {
            max = max.max(Easing::OutBack.apply(i as f32 / 100.0));
        }
        assert!(max > 1.0, "OutBack should overshoot, peaked at {max}");
    }

    #[test]
    fn reduced_motion_collapses_every_duration() {
        let m = Motion::new(true);
        for d in [
            Duration::Instant,
            Duration::Fast,
            Duration::Normal,
            Duration::Slow,
            Duration::Slower,
        ] {
            assert_eq!(m.duration(d), 0);
            assert_eq!(m.progress(50, d, Easing::OutCubic), 1.0);
        }
    }

    #[test]
    fn progress_reaches_one_at_the_end() {
        let m = Motion::default();
        assert!(m.progress(0, Duration::Normal, Easing::OutCubic) < 0.01);
        assert_eq!(m.progress(200, Duration::Normal, Easing::OutCubic), 1.0);
        assert!(m.progress(400, Duration::Normal, Easing::OutCubic) >= 1.0);
    }
}
