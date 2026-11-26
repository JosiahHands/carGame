//! People walking around the city's sidewalks in free roam. They stroll
//! around the blocks, leap out of the way of cars (always in time: nobody
//! gets hurt), run from explosions, and cheer big jumps. Everyone is drawn as
//! one mesh rebuilt each frame.
use bevy::math::DVec3;
use bevy::prelude::*;

use crate::meshes::{MeshData, linear};
use crate::rng;
use crate::world::{ROAD_LINES, World};

const PER_BLOCK: usize = 3;
const LOOP: f64 = 32.0;
const SIDE: f64 = LOOP * 2.0;
const PERIMETER: f64 = SIDE * 4.0;
const GRAVITY: f64 = 18.0;
const MAX: usize = 120;

const SKIN: [u32; 6] = [0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0xffdbac, 0x6b4226];
const SHIRTS: [u32; 10] = [
    0xd7263d, 0x1b98e0, 0x3bb273, 0xf46036, 0xffd23f, 0x7b2cbf, 0xffffff, 0x2e2e2e, 0xff70a6, 0x2ec4b6,
];
const TROUSERS: [u32; 6] = [0x2b3a55, 0x3a3a3a, 0x6b4f3a, 0x1d3557, 0x8a8f99, 0x4a5d23];

#[derive(Clone, Debug)]
struct Person {
    bx: f64,
    bz: f64,
    along: f64,
    way: f64,
    pace: f64,
    step: f64,
    off: DVec3,
    vel: DVec3,
    dodging: f64,
    fleeing: f64,
    cheering: f64,
    lean: f64,
    colors: [[f32; 4]; 3],
}

#[derive(Default)]
pub struct Pedestrians {
    people: Vec<Person>,
    clock: f64,
    pub visible: bool,
}

/// The car, for people to get out of the way of.
#[derive(Clone, Copy, Debug)]
pub struct CarNear {
    pub x: f64,
    pub z: f64,
    pub vx: f64,
    pub vz: f64,
    pub reach: f64,
}

impl Pedestrians {
    pub fn load(&mut self, world: &World, on: bool) {
        self.people.clear();
        self.visible = true;
        if !on || world.map != "city" {
            return;
        }
        let centers: Vec<f64> = ROAD_LINES[..6].iter().map(|c| c + 40.0).collect();
        for &bx in &centers {
            for &bz in &centers {
                for _ in 0..PER_BLOCK {
                    if self.people.len() >= MAX {
                        break;
                    }
                    self.people.push(Person {
                        bx,
                        bz,
                        along: rng::random() * PERIMETER,
                        way: if rng::random() < 0.5 { 1.0 } else { -1.0 },
                        pace: 1.0 + rng::random() * 0.6,
                        step: rng::random() * 10.0,
                        off: DVec3::ZERO,
                        vel: DVec3::ZERO,
                        dodging: 0.0,
                        fleeing: 0.0,
                        cheering: 0.0,
                        lean: 0.0,
                        colors: [linear(rng::pick(&SKIN)), linear(rng::pick(&SHIRTS)), linear(rng::pick(&TROUSERS))],
                    });
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.people.is_empty()
    }

    /// Everyone near an explosion runs away from it.
    pub fn panic(&mut self, x: f64, z: f64, radius: f64) {
        for p in &mut self.people {
            let at = loop_point(p);
            if (at.0 + p.off.x - x).hypot(at.1 + p.off.z - z) > radius {
                continue;
            }
            p.fleeing = 3.0 + rng::random() * 2.0;
            p.way = if at.2 * (at.0 - x) + at.3 * (at.1 - z) >= 0.0 { 1.0 } else { -1.0 };
        }
    }

    /// Everyone near a big jump or a fast pass cheers.
    pub fn cheer(&mut self, x: f64, z: f64, radius: f64) {
        for p in &mut self.people {
            if p.dodging > 0.0 || p.fleeing > 0.0 {
                continue;
            }
            let at = loop_point(p);
            if (at.0 - x).hypot(at.1 - z) < radius {
                p.cheering = 1.5 + rng::random() * 1.5;
            }
        }
    }

    pub fn update(&mut self, dt: f64, car: CarNear) {
        if self.people.is_empty() || !self.visible {
            return;
        }
        self.clock += dt;
        let speed = car.vx.hypot(car.vz);
        for p in &mut self.people {
            p.dodging = (p.dodging - dt).max(0.0);
            p.fleeing = (p.fleeing - dt).max(0.0);
            p.cheering = (p.cheering - dt).max(0.0);
            let lp = loop_point(p);
            let blocked = speed <= 3.0 && (lp.0 + p.off.x - car.x).hypot(lp.1 + p.off.z - car.z) < car.reach + 2.5;
            let walking = p.dodging <= 0.0 && p.cheering <= 0.0 && !blocked;
            let pace = if p.fleeing > 0.0 { 4.5 } else { p.pace };
            if walking {
                p.along = (p.along + p.way * pace * dt + PERIMETER).rem_euclid(PERIMETER);
                p.step += pace * dt * 4.2;
            }
            let at = loop_point(p);
            let (x, z) = (at.0 + p.off.x, at.1 + p.off.z);
            let grounded = p.off.y <= 0.0 && p.vel.y <= 0.0;
            let (dx, dz) = (x - car.x, z - car.z);
            let near = dx.hypot(dz);
            if grounded && speed <= 3.0 {
                if near < car.reach + 0.8 {
                    let (ax, az) = if near > 0.01 { (dx / near, dz / near) } else { (1.0, 0.0) };
                    let up = 3.0;
                    let push = (car.reach + 1.5 - near) / (2.0 * up / GRAVITY);
                    p.vel = DVec3::new(ax * push, up, az * push);
                    p.dodging = 0.8;
                    p.lean = 0.0;
                }
            } else if grounded {
                let soon = ((dx * car.vx + dz * car.vz) / (speed * speed)).clamp(0.0, 1.0);
                let miss = (dx - car.vx * soon).hypot(dz - car.vz * soon);
                if (miss < car.reach + 1.5 && near < speed * 1.2 + car.reach) || near < car.reach + 0.8 {
                    let (sx, sz) = (-car.vz / speed, car.vx / speed);
                    let away = if dx * sx + dz * sz >= 0.0 { 1.0 } else { -1.0 };
                    let up = if near < car.reach + 2.0 { 7.0 } else { 4.0 };
                    let go = (car.reach + 1.5 - miss).max(1.5);
                    let push = go / (2.0 * up / GRAVITY);
                    p.vel = DVec3::new(sx * away * push, up, sz * away * push);
                    p.dodging = 1.6;
                    p.lean = away;
                }
            }
            if p.off.y > 0.0 || p.vel.y > 0.0 {
                p.vel.y -= GRAVITY * dt;
                p.off += p.vel * dt;
                if p.off.y <= 0.0 {
                    p.off.y = 0.0;
                    p.vel = DVec3::ZERO;
                }
            } else if p.dodging <= 0.0 && !blocked {
                p.off *= (-dt * 1.5).exp();
            }
        }
    }

    /// Everyone, as one mesh.
    pub fn mesh(&self) -> MeshData {
        let mut m = MeshData::default();
        let leg = bevy::shape::Cuboid::new(0.17, 0.86, 0.19).mesh().build();
        let arm = bevy::shape::Cuboid::new(0.12, 0.62, 0.13).mesh().build();
        let body = bevy::shape::Cuboid::new(0.44, 0.62, 0.25).mesh().build();
        let head = bevy::shape::Sphere::new(0.14).mesh().uv(10, 8);
        for (i, p) in self.people.iter().enumerate() {
            let at = loop_point(p);
            let (x, z) = (at.0 + p.off.x, at.1 + p.off.z);
            let heading = (at.2 * p.way).atan2(at.3 * p.way);
            let dive = if p.dodging > 0.0 { p.lean * p.dodging.min(1.0) * 1.1 } else { 0.0 };
            let bounce = if p.dodging > 0.0 || p.cheering > 0.0 {
                0.0
            } else {
                p.step.sin().abs() * 0.04
            };
            let base = Transform::from_xyz(x as f32, (p.off.y + bounce) as f32, z as f32).with_rotation(Quat::from_euler(
                EulerRot::YXZ,
                heading as f32,
                0.0,
                dive as f32,
            ));
            let swing = if p.dodging > 0.0 {
                0.6
            } else {
                p.step.sin() * if p.fleeing > 0.0 { 0.9 } else { 0.5 }
            } as f32;
            // A part hangs from its joint (hip or shoulder), so it swings from there.
            let part = |px: f32, py: f32, rx: f32, rz: f32, hang: f32| {
                base * Transform::from_xyz(px, py, 0.0).with_rotation(Quat::from_euler(EulerRot::XYZ, rx, 0.0, rz))
                    * Transform::from_xyz(0.0, hang, 0.0)
            };
            let [skin, shirt, trousers] = p.colors;
            m.add_mesh(&leg, &part(-0.1, 0.9, swing, 0.0, -0.43), trousers);
            m.add_mesh(&leg, &part(0.1, 0.9, -swing, 0.0, -0.43), trousers);
            m.add_mesh(&body, &part(0.0, 0.88, 0.0, 0.0, 0.31), shirt);
            let up = if p.cheering > 0.0 {
                std::f32::consts::PI * (0.85 + ((self.clock * 12.0 + i as f64).sin() * 0.1) as f32)
            } else if p.dodging > 0.0 || p.fleeing > 0.0 {
                std::f32::consts::PI * 0.8
            } else {
                0.0
            };
            let raised = up != 0.0;
            m.add_mesh(
                &arm,
                &part(
                    -0.29,
                    1.46,
                    if raised { 0.0 } else { -swing },
                    if raised { -up } else { 0.0 },
                    -0.31,
                ),
                shirt,
            );
            m.add_mesh(
                &arm,
                &part(0.29, 1.46, if raised { 0.0 } else { swing }, if raised { up } else { 0.0 }, -0.31),
                shirt,
            );
            m.add_mesh(&head, &part(0.0, 1.66, 0.0, 0.0, 0.0), skin);
        }
        m
    }
}

/// Where on the loop around the block, and the loop's direction there: (x, z, dx, dz).
fn loop_point(p: &Person) -> (f64, f64, f64, f64) {
    let side = (p.along / SIDE).floor();
    let t = p.along - side * SIDE - LOOP;
    match side as i32 {
        0 => (p.bx + t, p.bz - LOOP, 1.0, 0.0),
        1 => (p.bx + LOOP, p.bz + t, 0.0, 1.0),
        2 => (p.bx - t, p.bz + LOOP, -1.0, 0.0),
        _ => (p.bx - LOOP, p.bz - t, 0.0, -1.0),
    }
}
