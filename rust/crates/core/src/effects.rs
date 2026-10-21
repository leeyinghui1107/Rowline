//! Decorative effects: the particles that burst when a line is cleared.
//!
//! Effects are **purely decorative**: they never influence the board, the
//! score or any rule.  They can be switched off completely (the reduced-motion
//! preference), in which case the system simply never spawns anything.
//!
//! The module is pure logic so that it can be unit-tested; the renderer
//! draws the particles it returns.

use crate::input::Point;
use crate::render::Color;
use crate::rng::Lcg;

/// One particle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    /// The position, in logical units.
    pub pos: Point,
    /// The velocity, in logical units per second.
    pub vel: Point,
    /// The remaining life, in seconds.
    pub life: f32,
    /// The total life, in seconds.
    pub life_max: f32,
    /// The size, in logical units.
    pub size: f32,
    /// The colour.
    pub color: Color,
}

impl Particle {
    /// The opacity, from 1 down to 0.
    pub fn opacity(&self) -> f32 {
        if self.life_max <= 0.0 {
            0.0
        } else {
            (self.life / self.life_max).clamp(0.0, 1.0)
        }
    }
}

/// The particle system.
#[derive(Clone, Debug, Default)]
pub struct EffectSystem {
    particles: Vec<Particle>,
    rng: Lcg,
}

impl EffectSystem {
    /// A system with a fixed seed, so the bursts are reproducible.
    pub fn new(seed: u32) -> Self {
        EffectSystem {
            particles: Vec::new(),
            rng: Lcg::with_seed(seed),
        }
    }

    /// The live particles.
    pub fn particles(&self) -> &[Particle] {
        &self.particles
    }

    /// `true` when nothing is alive.
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }

    /// Remove every particle.
    pub fn clear(&mut self) {
        self.particles.clear();
    }

    /// Spawn a burst of particles at a point.
    ///
    /// The count is clamped so that a huge line cannot flood the system.
    pub fn burst(&mut self, at: Point, color: Color, count: usize, speed: f32) {
        let n = count.min(48);
        for _ in 0..n {
            let a = (self.rng.next_u32() % 360) as f32 * std::f32::consts::TAU / 360.0;
            let v = speed * (0.4 + 0.6 * (self.rng.next_u32() % 100) as f32 / 100.0);
            let life = 0.35 + 0.4 * (self.rng.next_u32() % 100) as f32 / 100.0;
            self.particles.push(Particle {
                pos: at,
                vel: Point::new(a.cos() * v, a.sin() * v),
                life,
                life_max: life,
                size: 2.0 + 3.0 * (self.rng.next_u32() % 100) as f32 / 100.0,
                color,
            });
        }
    }

    /// Advance the simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        for p in self.particles.iter_mut() {
            p.life -= dt;
            p.pos.x += p.vel.x * dt;
            p.pos.y += p.vel.y * dt;
            // A little gravity, so the particles arc.
            p.vel.y += 180.0 * dt;
            p.vel.x *= 0.995;
        }
        self.particles.retain(|p| p.life > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_creates_particles() {
        let mut e = EffectSystem::new(7);
        assert!(e.is_empty());
        e.burst(Point::new(10.0, 10.0), Color::WHITE, 12, 60.0);
        assert_eq!(e.particles().len(), 12);
    }

    #[test]
    fn the_burst_is_bounded() {
        let mut e = EffectSystem::new(7);
        e.burst(Point::new(0.0, 0.0), Color::WHITE, 500, 60.0);
        assert!(e.particles().len() <= 48);
    }

    #[test]
    fn the_burst_is_reproducible() {
        let mut a = EffectSystem::new(99);
        let mut b = EffectSystem::new(99);
        a.burst(Point::new(1.0, 2.0), Color::WHITE, 10, 50.0);
        b.burst(Point::new(1.0, 2.0), Color::WHITE, 10, 50.0);
        assert_eq!(a.particles(), b.particles());
    }

    #[test]
    fn particles_die() {
        let mut e = EffectSystem::new(7);
        e.burst(Point::new(0.0, 0.0), Color::WHITE, 8, 10.0);
        for _ in 0..100 {
            e.update(0.05);
        }
        assert!(e.is_empty());
    }

    #[test]
    fn the_opacity_fades_out() {
        let mut e = EffectSystem::new(7);
        e.burst(Point::new(0.0, 0.0), Color::WHITE, 1, 10.0);
        let p = e.particles()[0];
        assert!((p.opacity() - 1.0).abs() < 1e-5);
        e.update(0.1);
        assert!(e.particles()[0].opacity() < 1.0);
    }

    #[test]
    fn a_huge_dt_cannot_teleport_the_particles() {
        let mut e = EffectSystem::new(7);
        e.burst(Point::new(0.0, 0.0), Color::WHITE, 4, 100.0);
        let before = e.particles()[0].pos;
        e.update(10.0);
        let after = e.particles()[0].pos;
        assert!((after.x - before.x).abs() < 10.0);
    }
}
