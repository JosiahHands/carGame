//! Explosions and what they leave behind: fire, smoke, sparks, flying debris,
//! fireballs, shockwave rings, scorch marks and bullet tracers.
//!
//! Everything lives in fixed-size pools that are reused, and each kind is
//! drawn as one mesh rebuilt every frame (see effects_render in world_render.rs),
//! so even a big explosion is only a handful of draws.
use bevy::math::DVec3;

use crate::rng::{self, random};

/// One particle: a soft round blob that moves, grows and fades.
#[derive(Clone, Copy, Debug, Default)]
pub struct Particle {
    pub pos: DVec3,
    pub velocity: DVec3,
    pub age: f64,
    pub life: f64,
    pub size: [f64; 2],
    pub start_color: [f32; 3],
    pub end_color: [f32; 3],
    pub opacity: f64,
    pub rise: f64,
    pub drag: f64,
    pub alive: bool,
}

/// What a new particle looks like.
#[derive(Clone, Copy, Debug)]
pub struct ParticleOptions {
    pub at: DVec3,
    pub velocity: DVec3,
    pub life: f64,
    pub size: [f64; 2],
    pub color: [f32; 3],
    pub end_color: Option<[f32; 3]>,
    pub opacity: f64,
    pub rise: f64,
    pub drag: f64,
}

impl Default for ParticleOptions {
    fn default() -> Self {
        ParticleOptions {
            at: DVec3::ZERO,
            velocity: DVec3::ZERO,
            life: 1.0,
            size: [1.0, 1.0],
            color: [1.0; 3],
            end_color: None,
            opacity: 1.0,
            rise: 0.0,
            drag: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Debris {
    pub pos: DVec3,
    pub rot: DVec3,
    pub scale: DVec3,
    pub color: [f32; 3],
    pub velocity: DVec3,
    pub spin: DVec3,
    pub age: f64,
    pub life: f64,
    pub resting: bool,
    pub alive: bool,
}

/// A short animation: fireball, ring, tracer or scorch mark.
#[derive(Clone, Copy, Debug, Default)]
pub struct Flash {
    pub pos: DVec3,
    pub age: f64,
    pub life: f64,
    pub size: f64,
    pub turn: f64,
    pub from: DVec3,
    pub to: DVec3,
    pub alive: bool,
}

const GLOW_COUNT: usize = 300;
const SMOKE_COUNT: usize = 300;
const DEBRIS_COUNT: usize = 150;

pub struct Effects {
    /// 0..1: how many particles each effect makes (the Effects setting).
    pub amount: f64,
    pub glow: Vec<Particle>,
    pub smoke: Vec<Particle>,
    next_glow: usize,
    next_smoke: usize,
    pub debris: Vec<Debris>,
    next_debris: usize,
    pub fireballs: Vec<Flash>,
    pub rings: Vec<Flash>,
    pub tracers: Vec<Flash>,
    pub scorch_marks: Vec<Flash>,
    next_scorch: usize,
    /// The flash of light from explosions: position, color and brightness.
    pub light_pos: DVec3,
    pub light_color: [f32; 3],
    pub light_intensity: f64,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            amount: 1.0,
            glow: vec![Particle::default(); GLOW_COUNT],
            smoke: vec![Particle::default(); SMOKE_COUNT],
            next_glow: 0,
            next_smoke: 0,
            debris: vec![Debris::default(); DEBRIS_COUNT],
            next_debris: 0,
            fireballs: vec![Flash::default(); 8],
            rings: vec![Flash::default(); 3],
            tracers: vec![Flash::default(); 40],
            scorch_marks: vec![Flash::default(); 30],
            next_scorch: 0,
            light_pos: DVec3::ZERO,
            light_color: rgb(0xffa040),
            light_intensity: 0.0,
        }
    }
}

/// An sRGB hex color as 0..1 floats.
pub fn rgb(hex: u32) -> [f32; 3] {
    [
        ((hex >> 16) & 255) as f32 / 255.0,
        ((hex >> 8) & 255) as f32 / 255.0,
        (hex & 255) as f32 / 255.0,
    ]
}

fn spread(range: f64) -> f64 {
    (random() - 0.5) * range
}

fn random_direction() -> DVec3 {
    DVec3::new(spread(2.0), spread(2.0), spread(2.0)).normalize_or_zero()
}

impl Effects {
    fn count(&self, n: usize) -> usize {
        ((n as f64 * self.amount).round() as usize).max(1)
    }

    pub fn add_glow(&mut self, o: ParticleOptions) {
        start(&mut self.glow[self.next_glow], o);
        self.next_glow = (self.next_glow + 1) % GLOW_COUNT;
    }

    pub fn add_smoke(&mut self, o: ParticleOptions) {
        start(&mut self.smoke[self.next_smoke], o);
        self.next_smoke = (self.next_smoke + 1) % SMOKE_COUNT;
    }

    pub fn add_debris(&mut self, at: DVec3, velocity: DVec3, size: DVec3, color: u32, spin: f64) {
        let d = &mut self.debris[self.next_debris];
        self.next_debris = (self.next_debris + 1) % DEBRIS_COUNT;
        *d = Debris {
            pos: at,
            rot: DVec3::new(random() * 6.0, random() * 6.0, random() * 6.0),
            scale: size,
            color: rgb(color),
            velocity,
            spin: DVec3::new(spread(spin), spread(spin), spread(spin)),
            age: 0.0,
            life: 8.0 + random() * 4.0,
            resting: false,
            alive: true,
        };
    }

    /// A firework: a ball of colored sparks that fall and fade.
    pub fn firework(&mut self, at: DVec3, color: u32, size: f64) {
        for _ in 0..self.count(60) {
            let y = random() * 2.0 - 1.0;
            let r = (1.0 - y * y).sqrt();
            let a = random() * std::f64::consts::TAU;
            let v = DVec3::new(a.cos() * r, y, a.sin() * r) * (14.0 * size * (0.8 + random() * 0.4));
            self.add_glow(ParticleOptions {
                at,
                velocity: v,
                life: 1.2 + random() * 0.5,
                size: [0.6 * size, 0.1 * size],
                color: rgb(color),
                end_color: Some([1.0; 3]),
                rise: -5.0,
                drag: 1.2,
                ..Default::default()
            });
        }
    }

    /// Confetti: little paper squares thrown up around a point.
    pub fn confetti(&mut self, at: DVec3, count: usize) {
        let colors = [0xff3b3b, 0xffd23f, 0x3bb273, 0x1b98e0, 0xc62f9e, 0xffffff];
        for i in 0..self.count(count) {
            let a = random() * std::f64::consts::TAU;
            let s = 2.0 + random() * 5.0;
            self.add_debris(
                DVec3::new(at.x + spread(1.0), at.y + 1.0, at.z + spread(1.0)),
                DVec3::new(a.cos() * s, 6.0 + random() * 8.0, a.sin() * s),
                DVec3::new(0.18, 0.02, 0.12),
                colors[i % colors.len()],
                14.0,
            );
        }
    }

    /// A burst of debris flying out from a point, in the given colors.
    pub fn shatter(&mut self, center: DVec3, count: usize, colors: &[u32], size: f64, speed: f64) {
        for _ in 0..self.count(count) {
            let angle = random() * std::f64::consts::TAU;
            let s = speed * (0.4 + random() * 0.8);
            let k = size * (0.4 + random() * 0.9);
            self.add_debris(
                DVec3::new(center.x + spread(size), center.y + random() * size, center.z + spread(size)),
                DVec3::new(angle.cos() * s, 2.0 + random() * 6.0, angle.sin() * s),
                DVec3::new(k, k * (0.4 + random() * 0.7), k * (0.6 + random() * 0.6)),
                rng::pick(colors),
                8.0,
            );
        }
    }

    /// A fiery explosion. `size` is roughly its radius in meters.
    pub fn explosion(&mut self, at: DVec3, size: f64) {
        play(&mut self.fireballs, at, size * 0.8, 0.55);
        self.light_color = rgb(0xffa040);
        self.light_pos = DVec3::new(at.x, at.y + 2.0, at.z);
        self.light_intensity = self.light_intensity.max(40.0 * size);
        for _ in 0..self.count(16) {
            let mut v = random_direction() * (size * (3.0 + random() * 4.0));
            v.y = 2.0 + random() * size * 4.0;
            self.add_glow(ParticleOptions {
                at,
                velocity: v,
                life: 0.35 + random() * 0.4,
                size: [size * 1.6, size * 0.3],
                color: rgb(0xffd966),
                end_color: Some(rgb(0xff4010)),
                drag: 3.5,
                rise: 2.0,
                ..Default::default()
            });
        }
        for _ in 0..self.count(12) {
            let mut v = random_direction() * (18.0 + random() * 18.0);
            v.y = 4.0 + random() * 12.0;
            self.add_glow(ParticleOptions {
                at,
                velocity: v,
                life: 0.6 + random() * 0.6,
                size: [0.3, 0.1],
                color: rgb(0xffe6a0),
                end_color: Some(rgb(0xff6020)),
                rise: -14.0,
                drag: 0.6,
                ..Default::default()
            });
        }
        for _ in 0..self.count(14) {
            let mut v = random_direction() * (size * 1.5);
            v.y = 1.5 + random() * 2.5;
            self.add_smoke(ParticleOptions {
                at: at + random_direction() * size,
                velocity: v,
                life: 2.5 + random() * 2.5,
                size: [size * 2.0, size * 5.0],
                color: rgb(0x303030),
                end_color: Some(rgb(0x606060)),
                opacity: 0.75,
                rise: 0.6,
                drag: 0.8,
            });
        }
        self.scorch(at.x, at.z, size * 2.6);
    }

    /// A flat blue ring rushing outwards, for the shockwave weapon.
    pub fn shockwave(&mut self, at: DVec3, radius: f64) {
        play(&mut self.rings, DVec3::new(at.x, 0.3, at.z), radius, 0.6);
        self.light_color = rgb(0x9fdcff);
        self.light_pos = DVec3::new(at.x, 2.0, at.z);
        self.light_intensity = 120.0;
        self.dust(at.x, at.z, 1.5, 1.5, 40, 0.5);
    }

    /// Sparks (and a puff of dust on walls and the road) where a bullet hits.
    pub fn bullet_hit(&mut self, at: DVec3, dusty: bool) {
        for _ in 0..self.count(5) {
            let mut v = random_direction() * (5.0 + random() * 8.0);
            v.y = 2.0 + random() * 5.0;
            self.add_glow(ParticleOptions {
                at,
                velocity: v,
                life: 0.15 + random() * 0.25,
                size: [0.2, 0.05],
                color: rgb(0xffd98a),
                end_color: Some(rgb(0xff6a1a)),
                rise: -12.0,
                ..Default::default()
            });
        }
        if dusty {
            self.add_smoke(ParticleOptions {
                at,
                velocity: DVec3::new(0.0, 0.8, 0.0),
                life: 1.0,
                size: [0.4, 1.6],
                color: rgb(0x9a948a),
                opacity: 0.6,
                drag: 1.5,
                ..Default::default()
            });
        }
    }

    /// A quick flash at the end of a gun barrel.
    pub fn muzzle_flash(&mut self, at: DVec3, direction: DVec3) {
        self.add_glow(ParticleOptions {
            at,
            velocity: direction * 4.0,
            life: 0.07,
            size: [0.5, 0.15],
            color: rgb(0xffd98a),
            ..Default::default()
        });
    }

    /// Fire and smoke rising from something burning. Call every frame.
    pub fn burn(&mut self, at: DVec3, strength: f64, dt: f64) {
        let per_second = 20.0 * strength * self.amount;
        if random() > per_second * dt {
            return;
        }
        let spot = DVec3::new(at.x + spread(strength), at.y, at.z + spread(strength));
        self.add_glow(ParticleOptions {
            at: spot,
            velocity: DVec3::new(0.0, 2.0 + random() * 2.0, 0.0),
            life: 0.5,
            size: [strength, 0.2],
            color: rgb(0xffb340),
            end_color: Some(rgb(0xff3005)),
            drag: 1.0,
            ..Default::default()
        });
        self.add_smoke(ParticleOptions {
            at: DVec3::new(spot.x, at.y + 0.6, spot.z),
            velocity: DVec3::new(0.6, 2.0 + random() * 1.5, 0.3),
            life: 3.0 + random() * 2.0,
            size: [strength, 4.0 * strength],
            color: rgb(0x202020),
            end_color: Some(rgb(0x505052)),
            opacity: 0.6,
            drag: 0.4,
            ..Default::default()
        });
    }

    /// Dust billowing out from the edges of a rectangle on the ground.
    pub fn dust(&mut self, x: f64, z: f64, half_width: f64, half_depth: f64, count: usize, scale: f64) {
        for _ in 0..self.count(count) {
            let along = random() * 2.0 - 1.0;
            let sides = [
                (x + along * half_width, z + half_depth, 0.0, 1.0),
                (x + along * half_width, z - half_depth, 0.0, -1.0),
                (x + half_width, z + along * half_depth, 1.0, 0.0),
                (x - half_width, z + along * half_depth, -1.0, 0.0),
            ];
            let (sx, sz, ox, oz) = sides[rng::index(4)];
            let speed = (3.0 + random() * 5.0) * scale;
            self.add_smoke(ParticleOptions {
                at: DVec3::new(sx + ox, 0.5 + random() * 2.0 * scale, sz + oz),
                velocity: DVec3::new(ox * speed + spread(2.0), 0.5 + random() * 2.0, oz * speed + spread(2.0)),
                life: 3.0 + random() * 3.0,
                size: [3.0 * scale, 9.0 * scale],
                color: rgb(0x8c857a),
                opacity: 0.8,
                drag: 0.9,
                ..Default::default()
            });
        }
    }

    /// A glowing streak flying from the gun to where the bullet hit.
    pub fn tracer(&mut self, from: DVec3, to: DVec3) {
        let i = self.tracers.iter().position(|t| !t.alive).unwrap_or(0);
        self.tracers[i] = Flash {
            from,
            to,
            age: 0.0,
            life: (from.distance(to) / 400.0).max(0.04),
            alive: true,
            ..Default::default()
        };
    }

    /// A dark burn mark on the ground.
    pub fn scorch(&mut self, x: f64, z: f64, size: f64) {
        let s = &mut self.scorch_marks[self.next_scorch];
        self.next_scorch = (self.next_scorch + 1) % 30;
        *s = Flash {
            pos: DVec3::new(x, 0.045, z),
            size,
            turn: random() * std::f64::consts::PI,
            age: 0.0,
            life: 35.0,
            alive: true,
            ..Default::default()
        };
    }

    /// Moves everything on by `dt` seconds.
    pub fn update(&mut self, dt: f64) {
        for p in self.glow.iter_mut().chain(self.smoke.iter_mut()) {
            if !p.alive {
                continue;
            }
            p.age += dt;
            if p.age >= p.life {
                p.alive = false;
                continue;
            }
            p.velocity *= (1.0 - p.drag * dt).max(0.0);
            p.velocity.y += p.rise * dt;
            p.pos += p.velocity * dt;
            p.pos.y = p.pos.y.max(0.05);
        }
        for d in &mut self.debris {
            if !d.alive {
                continue;
            }
            d.age += dt;
            if d.age >= d.life {
                d.alive = false;
                continue;
            }
            if !d.resting {
                d.velocity.y -= 16.0 * dt;
                d.pos += d.velocity * dt;
                d.rot += d.spin * dt;
                let half_height = d.scale.min_element() / 2.0;
                if d.pos.y < half_height {
                    d.pos.y = half_height;
                    d.velocity = DVec3::new(d.velocity.x * 0.5, d.velocity.y.abs() * 0.3, d.velocity.z * 0.5);
                    d.spin *= 0.5;
                    if d.velocity.length() < 1.0 {
                        d.resting = true;
                        d.rot = DVec3::new(0.0, d.rot.y, 0.0);
                        d.pos.y = d.scale.y / 2.0;
                    }
                }
            }
            let left = d.life - d.age;
            if left < 1.0 {
                d.scale *= (1.0 - dt / left).max(0.0);
            }
        }
        for f in self
            .fireballs
            .iter_mut()
            .chain(self.rings.iter_mut())
            .chain(self.scorch_marks.iter_mut())
            .chain(self.tracers.iter_mut())
        {
            if f.alive {
                f.age += dt;
                if f.age >= f.life {
                    f.alive = false;
                }
            }
        }
        self.light_intensity *= (-dt * 9.0).exp();
        if self.light_intensity < 0.5 {
            self.light_intensity = 0.0;
        }
    }

    /// Removes every effect at once.
    pub fn clear(&mut self) {
        for p in self.glow.iter_mut().chain(self.smoke.iter_mut()) {
            p.alive = false;
        }
        for d in &mut self.debris {
            d.alive = false;
        }
        for f in self
            .fireballs
            .iter_mut()
            .chain(self.rings.iter_mut())
            .chain(self.tracers.iter_mut())
            .chain(self.scorch_marks.iter_mut())
        {
            f.alive = false;
        }
    }
}

fn start(p: &mut Particle, o: ParticleOptions) {
    *p = Particle {
        pos: o.at,
        velocity: o.velocity,
        age: 0.0,
        life: o.life,
        size: o.size,
        start_color: o.color,
        end_color: o.end_color.unwrap_or(o.color),
        opacity: o.opacity,
        rise: o.rise,
        drag: o.drag,
        alive: true,
    };
}

fn play(pool: &mut [Flash], at: DVec3, size: f64, life: f64) {
    let i = pool.iter().position(|f| !f.alive).unwrap_or(0);
    pool[i] = Flash {
        pos: at,
        size,
        age: 0.0,
        life,
        alive: true,
        ..Default::default()
    };
}
