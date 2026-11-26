//! The city's traffic and the people on its sidewalks.

pub mod traffic {
    //! City traffic: cars that drive the streets in free roam. They keep to the
    //! right-hand lane, take crossings one car at a time, queue behind each
    //! other, and stop (and honk) when you're in the way. Each one is a real car
    //! in the world: ram it or blow it up and it wrecks like a parked car.
    use std::collections::HashMap;

    use crate::rng;
    use crate::world::{ROAD_LINES, World};
    use crate::world_build::Build;

    const LANE: f64 = 3.0;
    const STOP: f64 = 9.0;
    const BLOCK: f64 = 80.0;
    const LOW: f64 = ROAD_LINES[0];
    const HIGH: f64 = ROAD_LINES[6];
    const TURN_SPEED: f64 = 7.0;
    const LOOK_AHEAD: f64 = 28.0;

    const FORWARD: [(f64, f64); 4] = [(0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0)];
    const RIGHT: [(f64, f64); 4] = [(-1.0, 0.0), (0.0, 1.0), (1.0, 0.0), (0.0, -1.0)];

    #[derive(Clone, Copy, Debug)]
    struct Segment {
        ax: f64,
        az: f64,
        bx: f64,
        bz: f64,
        c: Option<(f64, f64)>,
        length: f64,
    }

    #[derive(Clone, Debug)]
    struct TrafficCar {
        target: usize,
        dir: usize,
        line: f64,
        next: f64,
        on_crossing: bool,
        seg: Segment,
        at: f64,
        speed: f64,
        cruise: f64,
        exit_dir: usize,
        exit_line: f64,
        waiting: f64,
        stunned: f64,
        wrecked: bool,
        next_honk: f64,
        near_miss_at: f64,
    }

    /// Where the player's car is, for the traffic to avoid and to spot near misses.
    #[derive(Clone, Copy, Debug)]
    pub struct TrafficPlayer {
        pub x: f64,
        pub y: f64,
        pub z: f64,
        pub vx: f64,
        pub vz: f64,
        pub width: f64,
        pub length: f64,
    }

    #[derive(Default)]
    pub struct Traffic {
        cars: Vec<TrafficCar>,
        crossings: HashMap<(i64, i64), usize>,
        clock: f64,
        /// Honks to play since last asked.
        pub honks: u32,
    }

    fn key(x: f64, z: f64) -> (i64, i64) {
        (x.round() as i64, z.round() as i64)
    }

    impl Traffic {
        /// Puts `count` cars on the streets (city only). Removes any from before.
        pub fn load(&mut self, world: &mut World, b: &mut Build, count: usize, px: f64, pz: f64) {
            self.clear(world);
            if world.map != "city" {
                return;
            }
            for _ in 0..count {
                if let Some(start) = self.start(px, pz, None, world) {
                    let p = point_on(&start.seg, 0.0);
                    let target = world.add_traffic_car(b, p.0, p.1, p.2);
                    self.cars.push(TrafficCar { target, ..start });
                }
            }
        }

        pub fn clear(&mut self, world: &mut World) {
            for c in &self.cars {
                if c.target < world.targets.len() && world.targets[c.target].traffic {
                    world.remove_car(c.target);
                }
            }
            self.cars.clear();
            self.crossings.clear();
        }

        pub fn count(&self) -> usize {
            self.cars.len()
        }

        pub fn owns(&self, target: usize) -> bool {
            self.cars.iter().any(|c| c.target == target)
        }

        /// The player rammed a traffic car: it stops for a moment, and lets you know.
        pub fn hit(&mut self, target: usize) {
            let clock = self.clock;
            if let Some(car) = self.cars.iter_mut().find(|c| c.target == target) {
                car.speed = 0.0;
                car.stunned = 2.5;
                if clock > car.next_honk {
                    car.next_honk = clock + 3.0;
                    self.honks += 1;
                }
            }
        }

        /// Each car's target, speed, front wheel turn and whether it's braking, for drawing.
        pub fn visuals(&self) -> Vec<(usize, f64, f64, bool)> {
            self.cars
                .iter()
                .map(|c| {
                    let steer = if c.on_crossing && c.seg.c.is_some() {
                        if c.exit_dir == (c.dir + 1) % 4 { 0.35 } else { -0.35 }
                    } else {
                        0.0
                    };
                    (c.target, c.speed, steer, c.speed < c.cruise * 0.6)
                })
                .collect()
        }

        /// Drives every car. Returns the points for near misses the player had with traffic.
        pub fn update(&mut self, dt: f64, player: TrafficPlayer, world: &mut World) -> Vec<f64> {
            if self.cars.is_empty() {
                return Vec::new();
            }
            self.clock += dt;
            let mut misses = Vec::new();
            for i in 0..self.cars.len() {
                let t = self.cars[i].target;
                if !world.targets[t].alive {
                    self.release(i);
                    self.cars[i].wrecked = true;
                    continue;
                }
                if self.cars[i].wrecked {
                    if let Some(fresh) = self.start(player.x, player.z, Some(t), world) {
                        self.cars[i] = TrafficCar { target: t, ..fresh };
                    }
                    continue;
                }
                self.drive(i, dt, player, world);
                let miss = self.near_miss(i, player, world);
                if miss > 0.0 {
                    misses.push(miss);
                }
            }
            misses
        }

        fn drive(&mut self, i: usize, dt: f64, player: TrafficPlayer, world: &mut World) {
            let t = self.cars[i].target;
            let (tx, tz, heading) = (world.targets[t].x, world.targets[t].z, world.targets[t].rot.y);
            let car = &mut self.cars[i];
            car.stunned = (car.stunned - dt).max(0.0);
            let mut want = if car.stunned > 0.0 {
                0.0
            } else if car.on_crossing && car.seg.c.is_some() {
                TURN_SPEED
            } else {
                car.cruise
            };
            let (fx, fz) = (heading.sin(), heading.cos());
            let mut gap = f64::INFINITY;
            let mut player_ahead = false;
            let mut check = |x: f64, z: f64, room: f64, is_player: bool| {
                let (dx, dz) = (x - tx, z - tz);
                let ahead = dx * fx + dz * fz;
                if ahead <= 0.0 || ahead > LOOK_AHEAD || (dx * fz - dz * fx).abs() > room {
                    return;
                }
                if ahead < gap {
                    gap = ahead;
                    player_ahead = is_player;
                }
            };
            for (j, other) in self.cars.iter().enumerate() {
                if j != i {
                    let o = &world.targets[other.target];
                    check(o.x, o.z, 2.4, false);
                }
            }
            check(player.x, player.z, 1.4 + player.width / 2.0, true);
            if gap < f64::INFINITY {
                want = want.min(((gap - 7.0) * 1.2).max(0.0));
            }
            if !self.cars[i].on_crossing {
                let left = self.cars[i].seg.length - self.cars[i].at;
                let blocked = gap < left + STOP * 2.0;
                if blocked {
                    self.release(i);
                }
                if left < 14.0 && (blocked || !self.reserve(i)) {
                    want = want.min(((left - 0.3) * 1.5).max(0.0));
                }
            }
            let clock = self.clock;
            let car = &mut self.cars[i];
            let change = want - car.speed;
            car.speed = (car.speed + change.clamp(-12.0 * dt, 4.0 * dt)).max(0.0);
            car.waiting = if car.speed < 0.3 { car.waiting + dt } else { 0.0 };
            if player_ahead && car.waiting > 1.2 && clock > car.next_honk && gap < 20.0 {
                car.next_honk = clock + 3.0 + rng::random() * 3.0;
                self.honks += 1;
            }
            if self.cars[i].waiting > 12.0 && (player.x - tx).hypot(player.z - tz) > 80.0 {
                if let Some(fresh) = self.start(player.x, player.z, Some(t), world) {
                    self.release(i);
                    self.cars[i] = TrafficCar { target: t, ..fresh };
                }
                return;
            }
            let car = &mut self.cars[i];
            car.at += car.speed * dt;
            while self.cars[i].at >= self.cars[i].seg.length {
                let len = self.cars[i].seg.length;
                self.cars[i].at -= len;
                if !self.advance(i) {
                    let car = &mut self.cars[i];
                    car.at = car.seg.length;
                    car.speed = 0.0;
                    break;
                }
            }
            let car = &self.cars[i];
            let p = point_on(&car.seg, car.at);
            world.move_car(t, p.0, p.1, p.2);
        }

        fn advance(&mut self, i: usize) -> bool {
            if !self.cars[i].on_crossing {
                if !self.reserve(i) {
                    return false;
                }
                let car = &mut self.cars[i];
                car.on_crossing = true;
                car.seg = crossing_segment(car.dir, car.line, car.next, car.exit_dir, car.exit_line);
                return true;
            }
            self.release(i);
            let car = &mut self.cars[i];
            let straight = car.exit_dir == car.dir;
            let (efx, efz) = FORWARD[car.exit_dir];
            let sign = efx + efz;
            let crossed_at = if straight { car.next } else { car.line };
            car.dir = car.exit_dir;
            car.line = car.exit_line;
            car.next = crossed_at + sign * BLOCK;
            car.on_crossing = false;
            let from = lane_point(car.dir, car.line, crossed_at + sign * STOP);
            let to = lane_point(car.dir, car.line, car.next - sign * STOP);
            car.seg = line(from, to);
            choose_turn(car);
            true
        }

        fn crossing_key(car: &TrafficCar) -> (i64, i64) {
            if car.dir.is_multiple_of(2) {
                key(car.line, car.next)
            } else {
                key(car.next, car.line)
            }
        }

        fn reserve(&mut self, i: usize) -> bool {
            let k = Self::crossing_key(&self.cars[i]);
            match self.crossings.get(&k) {
                Some(&holder) if holder != i => false,
                _ => {
                    self.crossings.insert(k, i);
                    true
                }
            }
        }

        fn release(&mut self, i: usize) {
            let k = Self::crossing_key(&self.cars[i]);
            if self.crossings.get(&k) == Some(&i) {
                self.crossings.remove(&k);
            }
        }

        fn near_miss(&mut self, i: usize, player: TrafficPlayer, world: &World) -> f64 {
            let car = &mut self.cars[i];
            let t = &world.targets[car.target];
            let Some(rig) = &t.rig else { return 0.0 };
            let speed = player.vx.hypot(player.vz);
            if speed < 18.0 || self.clock - car.near_miss_at < 2.5 || player.y > rig.height + 0.5 {
                return 0.0;
            }
            let (fx, fz) = (t.rot.y.sin(), t.rot.y.cos());
            let (dx, dz) = (player.x - t.x, player.z - t.z);
            let along = (dx * fx + dz * fz).abs();
            let side = (dx * fz - dz * fx).abs();
            let gap = side - rig.width / 2.0 - player.width / 2.0;
            if along > (rig.length + player.length) / 2.0 || !(0.15..=1.4).contains(&gap) {
                return 0.0;
            }
            let closing = (player.vx - fx * car.speed).hypot(player.vz - fz * car.speed);
            if closing < 4.0 {
                return 0.0;
            }
            car.near_miss_at = self.clock;
            100.0 + speed * 3.0
        }

        /// A random place to start, at least 50 m from the player and clear of the other cars.
        fn start(&self, px: f64, pz: f64, target: Option<usize>, world: &mut World) -> Option<TrafficCar> {
            for _ in 0..40 {
                let dir = rng::index(4);
                let (fx, fz) = FORWARD[dir];
                let sign = fx + fz;
                let line_at = rng::pick(&ROAD_LINES);
                let choices: Vec<f64> = ROAD_LINES.iter().copied().filter(|c| in_city(c - sign * BLOCK)).collect();
                let next = rng::pick(&choices);
                let along = next - sign * (STOP + 6.0 + rng::random() * (BLOCK - 2.0 * STOP - 12.0));
                let p = lane_point(dir, line_at, along);
                if (p.0 - px).hypot(p.1 - pz) < 50.0 {
                    continue;
                }
                if self
                    .cars
                    .iter()
                    .any(|c| Some(c.target) != target && (world.targets[c.target].x - p.0).hypot(world.targets[c.target].z - p.1) < 14.0)
                {
                    continue;
                }
                let mut car = TrafficCar {
                    target: target.unwrap_or(0),
                    dir,
                    line: line_at,
                    next,
                    on_crossing: false,
                    seg: line(p, lane_point(dir, line_at, next - sign * STOP)),
                    at: 0.0,
                    speed: 0.0,
                    cruise: 11.0 + rng::random() * 6.0,
                    exit_dir: dir,
                    exit_line: line_at,
                    waiting: 0.0,
                    stunned: 0.0,
                    wrecked: false,
                    next_honk: 0.0,
                    near_miss_at: -10.0,
                };
                choose_turn(&mut car);
                if let Some(t) = target {
                    let pose = point_on(&car.seg, 0.0);
                    world.targets[t].home = crate::world::Spot {
                        x: p.0,
                        z: p.1,
                        heading: pose.2,
                    };
                    world.move_car(t, pose.0, pose.1, pose.2);
                }
                return Some(car);
            }
            None
        }
    }

    fn choose_turn(car: &mut TrafficCar) {
        let mut options: Vec<(usize, f64, f64)> = Vec::new();
        let (fx, fz) = FORWARD[car.dir];
        if in_city(car.next + (fx + fz) * BLOCK) {
            options.push((car.dir, car.line, 2.0));
        }
        for turn in [1, 3] {
            let dir = (car.dir + turn) % 4;
            let (tx, tz) = FORWARD[dir];
            if in_city(car.line + (tx + tz) * BLOCK) {
                options.push((dir, car.next, 1.0));
            }
        }
        if options.is_empty() {
            return;
        }
        let mut roll = rng::random() * options.iter().map(|o| o.2).sum::<f64>();
        let mut pick = options[0];
        for o in &options {
            roll -= o.2;
            if roll < 0.0 {
                pick = *o;
                break;
            }
        }
        car.exit_dir = pick.0;
        car.exit_line = pick.1;
    }

    fn in_city(c: f64) -> bool {
        (LOW - 1.0..=HIGH + 1.0).contains(&c)
    }

    fn lane_point(dir: usize, road: f64, along: f64) -> (f64, f64) {
        let (fx, _) = FORWARD[dir];
        let (rx, rz) = RIGHT[dir];
        if fx == 0.0 {
            (road + rx * LANE, along)
        } else {
            (along, road + rz * LANE)
        }
    }

    fn line(a: (f64, f64), b: (f64, f64)) -> Segment {
        Segment {
            ax: a.0,
            az: a.1,
            bx: b.0,
            bz: b.1,
            c: None,
            length: (b.0 - a.0).hypot(b.1 - a.1),
        }
    }

    fn crossing_segment(dir: usize, road: f64, next: f64, exit_dir: usize, exit_line: f64) -> Segment {
        let (fx, fz) = FORWARD[dir];
        let sign = fx + fz;
        let from = lane_point(dir, road, next - sign * STOP);
        if exit_dir == dir {
            return line(from, lane_point(dir, road, next + sign * STOP));
        }
        let (ex, ez) = FORWARD[exit_dir];
        let to = lane_point(exit_dir, exit_line, road + (ex + ez) * STOP);
        let (cx, cz) = if fx == 0.0 { (from.0, to.1) } else { (to.0, from.1) };
        let mut length = 0.0;
        let mut last = from;
        for i in 1..=12 {
            let p = bezier(from, (cx, cz), to, i as f64 / 12.0);
            length += (p.0 - last.0).hypot(p.1 - last.1);
            last = p;
        }
        Segment {
            ax: from.0,
            az: from.1,
            bx: to.0,
            bz: to.1,
            c: Some((cx, cz)),
            length,
        }
    }

    fn bezier(a: (f64, f64), c: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
        let u = 1.0 - t;
        (
            u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0,
            u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1,
        )
    }

    /// Where on a segment, and which way it faces, `at` meters along: (x, z, heading).
    fn point_on(seg: &Segment, at: f64) -> (f64, f64, f64) {
        let t = if seg.length > 0.0 { (at / seg.length).min(1.0) } else { 1.0 };
        match seg.c {
            None => (
                seg.ax + (seg.bx - seg.ax) * t,
                seg.az + (seg.bz - seg.az) * t,
                (seg.bx - seg.ax).atan2(seg.bz - seg.az),
            ),
            Some((cx, cz)) => {
                let p = bezier((seg.ax, seg.az), (cx, cz), (seg.bx, seg.bz), t);
                let u = 1.0 - t;
                let dx = 2.0 * u * (cx - seg.ax) + 2.0 * t * (seg.bx - cx);
                let dz = 2.0 * u * (cz - seg.az) + 2.0 * t * (seg.bz - cz);
                (p.0, p.1, dx.atan2(dz))
            }
        }
    }
}

pub mod pedestrians {
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
}
