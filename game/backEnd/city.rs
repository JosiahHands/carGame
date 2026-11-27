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

    /// A car on an open-world highway (see land.rs).
    #[derive(Clone, Debug)]
    struct RoadCar {
        target: usize,
        road: usize,
        along: f64,
        /// +1 driving from the road's start to its end, -1 the other way.
        way: f64,
        speed: f64,
        cruise: f64,
        heading: f64,
        stunned: f64,
        waiting: f64,
        next_honk: f64,
        near_miss_at: f64,
        /// Out of the way, waiting for a road near the player.
        idle: bool,
    }

    #[derive(Default)]
    pub struct Traffic {
        cars: Vec<TrafficCar>,
        road_cars: Vec<RoadCar>,
        crossings: HashMap<(i64, i64), usize>,
        clock: f64,
        /// The city's own traffic is put away while you're far from the city.
        city_hidden: bool,
        /// Honks to play since last asked.
        pub honks: u32,
    }

    fn key(x: f64, z: f64) -> (i64, i64) {
        (x.round() as i64, z.round() as i64)
    }

    impl Traffic {
        /// Puts `count` cars on the streets (city only), and half as many again
        /// on the open world's highways. Removes any from before.
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
            if world.land.is_some() {
                for _ in 0..count / 2 {
                    let target = world.add_traffic_car(b, 0.0, -1e5, 0.0);
                    world.targets[target].home = crate::world::Spot {
                        x: 0.0,
                        z: -1e5,
                        heading: 0.0,
                    };
                    self.road_cars.push(RoadCar {
                        target,
                        road: 0,
                        along: 0.0,
                        way: 1.0,
                        speed: 0.0,
                        cruise: 22.0 + rng::random() * 7.0,
                        heading: 0.0,
                        stunned: 0.0,
                        waiting: 0.0,
                        next_honk: 0.0,
                        near_miss_at: -10.0,
                        idle: true,
                    });
                }
                for i in 0..self.road_cars.len() {
                    self.place_on_road(i, px, pz, world, 0.0);
                }
            }
        }

        pub fn clear(&mut self, world: &mut World) {
            for t in self.cars.iter().map(|c| c.target).chain(self.road_cars.iter().map(|c| c.target)) {
                if t < world.targets.len() && world.targets[t].traffic {
                    world.remove_car(t);
                }
            }
            self.cars.clear();
            self.road_cars.clear();
            self.crossings.clear();
            self.city_hidden = false;
        }

        pub fn count(&self) -> usize {
            self.cars.len() + self.road_cars.len()
        }

        pub fn owns(&self, target: usize) -> bool {
            self.cars.iter().any(|c| c.target == target) || self.road_cars.iter().any(|c| c.target == target)
        }

        /// The player took this car: it isn't traffic any more.
        pub fn forget(&mut self, target: usize) {
            if let Some(i) = self.cars.iter().position(|c| c.target == target) {
                self.release(i);
                self.cars.remove(i);
                // Crossings are held by index: let go of them all and let the cars take them again.
                self.crossings.clear();
            }
            self.road_cars.retain(|c| c.target != target);
        }

        /// The player rammed a traffic car: it stops for a moment, and lets you know.
        pub fn hit(&mut self, target: usize) {
            let clock = self.clock;
            let mut honk = false;
            if let Some(car) = self.cars.iter_mut().find(|c| c.target == target) {
                car.speed = 0.0;
                car.stunned = 2.5;
                if clock > car.next_honk {
                    car.next_honk = clock + 3.0;
                    honk = true;
                }
            }
            if let Some(car) = self.road_cars.iter_mut().find(|c| c.target == target) {
                car.speed = 0.0;
                car.stunned = 2.5;
                if clock > car.next_honk {
                    car.next_honk = clock + 3.0;
                    honk = true;
                }
            }
            if honk {
                self.honks += 1;
            }
        }

        /// Puts a highway car somewhere on a road near the player, 220-650 m
        /// away (or out of the way if there's no road near). `along_bias`
        /// prefers spots ahead of the player's direction (unused when 0).
        fn place_on_road(&mut self, i: usize, px: f64, pz: f64, world: &mut World, _along_bias: f64) {
            let Some(land) = world.land.as_deref() else { return };
            let roads = land.roads_near(px, pz, 650.0);
            let mut spot = None;
            for _ in 0..12 {
                if roads.is_empty() {
                    break;
                }
                let road = rng::pick(&roads);
                let r = &land.roads[road];
                let along = rng::random() * r.length;
                let (x, z, _) = r.pose(along, 0.0);
                let d = (x - px).hypot(z - pz);
                if !(220.0..=650.0).contains(&d) {
                    continue;
                }
                let way = if rng::random() < 0.5 { 1.0 } else { -1.0 };
                let crowded = self
                    .road_cars
                    .iter()
                    .enumerate()
                    .any(|(j, c)| j != i && !c.idle && c.road == road && c.way == way && (c.along - along).abs() < 40.0);
                if !crowded {
                    spot = Some((road, along, way));
                    break;
                }
            }
            let c = &mut self.road_cars[i];
            match spot {
                Some((road, along, way)) => {
                    c.road = road;
                    c.along = along;
                    c.way = way;
                    c.speed = c.cruise * 0.8;
                    c.idle = false;
                    c.stunned = 0.0;
                    c.waiting = 0.0;
                    let (x, z, heading) = lane_pose(&land.roads[road], along, way);
                    c.heading = heading;
                    let t = c.target;
                    world.targets[t].home = crate::world::Spot { x, z, heading };
                    world.move_car(t, x, z, heading);
                }
                None => {
                    c.idle = true;
                    let t = c.target;
                    world.move_car(t, 0.0, -1e5, 0.0);
                }
            }
        }

        /// Drives the highway cars, and moves the ones left far behind to
        /// roads near the player. Returns near-miss points.
        fn drive_roads(&mut self, dt: f64, player: TrafficPlayer, world: &mut World) -> Vec<f64> {
            let mut misses = Vec::new();
            for i in 0..self.road_cars.len() {
                let t = self.road_cars[i].target;
                if !world.targets[t].alive {
                    // Wrecked: it comes back (world.update) at its home, then gets going again.
                    self.road_cars[i].speed = 0.0;
                    continue;
                }
                let far = {
                    let tg = &world.targets[t];
                    (tg.x - player.x).hypot(tg.z - player.z) > 800.0
                };
                if self.road_cars[i].idle || far {
                    // Now and then, look for a road near the player again.
                    if self.road_cars[i].idle && rng::random() > dt * 2.0 {
                        continue;
                    }
                    self.place_on_road(i, player.x, player.z, world, 0.0);
                    continue;
                }
                let Some(land) = world.land.as_deref() else { return misses };
                let c = self.road_cars[i].clone();
                let road = &land.roads[c.road];
                let (cx, cz) = (world.targets[t].x, world.targets[t].z);
                let (fx, fz) = (c.heading.sin(), c.heading.cos());
                // The nearest car ahead in the same lane, or the player.
                let mut gap = f64::INFINITY;
                let mut player_ahead = false;
                for (j, o) in self.road_cars.iter().enumerate() {
                    // A wreck has flown off its lane: it's a box in the world, not a car to follow.
                    if j == i || o.idle || o.road != c.road || o.way != c.way || !world.targets[o.target].alive {
                        continue;
                    }
                    let ahead = (o.along - c.along) * c.way;
                    if ahead > 0.0 && ahead < gap {
                        gap = ahead;
                    }
                }
                let (dx, dz) = (player.x - cx, player.z - cz);
                let ahead = dx * fx + dz * fz;
                if ahead > 0.0 && ahead < 40.0 && (dx * fz - dz * fx).abs() < 1.6 + player.width / 2.0 && ahead < gap {
                    gap = ahead;
                    player_ahead = true;
                }
                let in_town = land.town_at(cx, cz, 0.0).is_some();
                let c = &mut self.road_cars[i];
                c.stunned = (c.stunned - dt).max(0.0);
                let mut want = if c.stunned > 0.0 {
                    0.0
                } else if in_town {
                    c.cruise.min(12.0)
                } else {
                    c.cruise
                };
                if gap < f64::INFINITY {
                    want = want.min(((gap - 9.0) * 1.1).max(0.0));
                }
                let change = want - c.speed;
                c.speed = (c.speed + change.clamp(-14.0 * dt, 4.0 * dt)).max(0.0);
                c.waiting = if c.speed < 0.3 { c.waiting + dt } else { 0.0 };
                if player_ahead && c.waiting > 1.2 && self.clock > c.next_honk && gap < 20.0 {
                    c.next_honk = self.clock + 3.0 + rng::random() * 3.0;
                    self.honks += 1;
                }
                c.along += c.way * c.speed * dt;
                // At the end of the road: on to another road from the same place.
                if c.along > road.length || c.along < 0.0 {
                    let over = if c.along > road.length { c.along - road.length } else { -c.along };
                    let node = if c.way > 0.0 { road.to } else { road.from };
                    let town = &land.towns[node];
                    let choices: Vec<usize> = town.roads.iter().copied().filter(|&r| r != c.road).collect();
                    if town.size != crate::land::Size::City && !choices.is_empty() {
                        let next = rng::pick(&choices);
                        let r2 = &land.roads[next];
                        c.road = next;
                        if r2.from == node {
                            c.way = 1.0;
                            c.along = over.min(r2.length);
                        } else {
                            c.way = -1.0;
                            c.along = (r2.length - over).max(0.0);
                        }
                    } else if (cx - player.x).hypot(cz - player.z) > 200.0 {
                        self.place_on_road(i, player.x, player.z, world, 0.0);
                        continue;
                    } else {
                        // A dead end in sight: turn round.
                        c.way = -c.way;
                        c.along = c.along.clamp(0.0, road.length);
                    }
                }
                let Some(land) = world.land.as_deref() else { return misses };
                let c = &mut self.road_cars[i];
                let (x, z, heading) = lane_pose(&land.roads[c.road], c.along, c.way);
                let turn = (heading - c.heading + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                c.heading += turn * (dt * 8.0).min(1.0);
                let (h, target, speed) = (c.heading, c.target, c.speed);
                world.move_car(target, x, z, h);
                let miss = self.road_near_miss(i, speed, player, world);
                if miss > 0.0 {
                    misses.push(miss);
                }
            }
            misses
        }

        fn road_near_miss(&mut self, i: usize, car_speed: f64, player: TrafficPlayer, world: &World) -> f64 {
            let clock = self.clock;
            let car = &mut self.road_cars[i];
            let t = &world.targets[car.target];
            let Some(rig) = &t.rig else { return 0.0 };
            let speed = player.vx.hypot(player.vz);
            if speed < 18.0 || clock - car.near_miss_at < 2.5 || player.y > t.pos.y + rig.height + 0.5 {
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
            let closing = (player.vx - fx * car_speed).hypot(player.vz - fz * car_speed);
            if closing < 4.0 {
                return 0.0;
            }
            car.near_miss_at = clock;
            100.0 + speed * 3.0
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
                .chain(
                    self.road_cars
                        .iter()
                        .filter(|c| !c.idle)
                        .map(|c| (c.target, c.speed, 0.0, c.speed < c.cruise * 0.6)),
                )
                .collect()
        }

        /// Drives every car. Returns the points for near misses the player had with traffic.
        pub fn update(&mut self, dt: f64, player: TrafficPlayer, world: &mut World) -> Vec<f64> {
            if self.cars.is_empty() && self.road_cars.is_empty() {
                return Vec::new();
            }
            self.clock += dt;
            let mut misses = self.drive_roads(dt, player, world);
            // Far out in the country, the city's own traffic waits out of sight.
            let away = world.land.is_some() && player.x.abs().max(player.z.abs()) > 1100.0;
            if away != self.city_hidden {
                self.city_hidden = away;
                for c in &self.cars {
                    if let Some(rig) = &world.targets[c.target].rig {
                        world.changes.push(crate::world::Change::Show(rig.root, !away));
                    }
                }
            }
            if away {
                return misses;
            }
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

    /// Where a highway car `along` meters down `road` is, in its lane: (x, z, heading).
    /// The lanes meet in the middle at each end of the road, where it joins the next.
    fn lane_pose(road: &crate::land::Road, along: f64, way: f64) -> (f64, f64, f64) {
        let to_end = along.min(road.length - along);
        let lane = crate::land::LANE * crate::land::smoothstep(0.0, 24.0, to_end);
        let (x, z, heading) = road.pose(along, way * lane);
        (x, z, if way > 0.0 { heading } else { heading + std::f64::consts::PI })
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
    //! People walking around the sidewalks of the city and the towns in free
    //! roam. They stroll around the blocks, leap out of the way of cars (always
    //! in time: nobody gets run over), run from explosions and fights, and
    //! cheer big jumps. On foot you can fight them: they're knocked out, never
    //! killed, and get back up a while later. Some fight back.
    //!
    //! Everyone (and the player on foot, and the police) is drawn as one mesh
    //! rebuilt each frame (see `draw_person`).
    use bevy::math::{DVec2, DVec3};
    use bevy::prelude::*;

    use crate::meshes::{MeshData, linear};
    use crate::rng;
    use crate::world::{ROAD_LINES, World};

    const PER_BLOCK: usize = 3;
    const LOOP: f64 = 32.0;
    const SIDE: f64 = LOOP * 2.0;
    const PERIMETER: f64 = SIDE * 4.0;
    const GRAVITY: f64 = 18.0;
    /// People in the city, and in the towns as well.
    const MAX: usize = 120;
    const MAX_ALL: usize = 900;
    /// People further than this from the player stand still and aren't drawn.
    const AWAKE: f64 = 260.0;
    pub const HEALTH: f64 = 100.0;

    pub const SKIN: [u32; 6] = [0xf1c27d, 0xe0ac69, 0xc68642, 0x8d5524, 0xffdbac, 0x6b4226];
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
        hp: f64,
        /// Seconds left lying knocked out.
        down: f64,
        /// Which way they fell.
        fell: f64,
        /// Seconds left fighting the player.
        fighting: f64,
        punch: f64,
        /// Where they face while fighting or fleeing off their loop.
        facing: Option<f64>,
    }

    #[derive(Default)]
    pub struct Pedestrians {
        people: Vec<Person>,
        clock: f64,
        pub visible: bool,
        /// Where the player is, for waking people and drawing them.
        near: DVec2,
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

    /// Someone knocked out or hit, for the score and the police.
    #[derive(Clone, Copy, Debug)]
    pub struct Hit {
        pub at: DVec3,
        pub knocked_out: bool,
    }

    /// How to draw someone: where they are and what they're doing.
    #[derive(Clone, Copy, Debug)]
    pub struct PersonPose {
        /// Where their feet are.
        pub at: Vec3,
        pub heading: f32,
        /// How far through their stride they are (radians), and how big it is (0..1).
        pub step: f32,
        pub stride: f32,
        /// 0 standing up, 1 flat on their back.
        pub lying: f32,
        /// Sideways lean, for diving out of the way.
        pub dive: f32,
        pub arms: Arms,
        /// Skin, shirt, trousers (linear).
        pub colors: [[f32; 4]; 3],
        pub pistol: bool,
        pub scale: f32,
        /// A hat (police), in this color.
        pub cap: Option<[f32; 4]>,
        /// A little bounce in their step.
        pub bounce: f32,
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    pub enum Arms {
        Swing,
        /// Both up, this far (radians): cheering or panicking.
        Up(f32),
        /// The right arm straight out in front (aiming).
        Aim,
        /// A punch with the right arm, 0..1 of the way out.
        Punch(f32),
        /// Fists up, ready to fight.
        Guard,
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
                        self.people.push(Self::person(bx, bz));
                    }
                }
            }
        }

        fn person(bx: f64, bz: f64) -> Person {
            Person {
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
                hp: HEALTH,
                down: 0.0,
                fell: 0.0,
                fighting: 0.0,
                punch: 0.0,
                facing: None,
            }
        }

        /// People for a town's blocks (the middle of each block).
        pub fn add_town(&mut self, blocks: &[DVec2]) {
            for b in blocks {
                for _ in 0..PER_BLOCK {
                    if self.people.len() >= MAX_ALL {
                        return;
                    }
                    self.people.push(Self::person(b.x, b.y));
                }
            }
        }

        /// Someone climbs out of a car (its driver, when you take it) and runs off.
        pub fn bail_out(&mut self, x: f64, z: f64, away: f64) {
            // Their loop is the block nearest; they start off it and run back to it.
            let (bx, bz) = ((x / 80.0).floor() * 80.0 + 40.0, (z / 80.0).floor() * 80.0 + 40.0);
            let mut p = Self::person(bx, bz);
            let at = loop_point(&p);
            p.off = DVec3::new(x - at.0, 0.0, z - at.1);
            p.fleeing = 6.0;
            p.facing = Some(away);
            self.people.push(p);
        }

        pub fn is_empty(&self) -> bool {
            self.people.is_empty()
        }

        fn at(p: &Person) -> DVec3 {
            let lp = loop_point(p);
            DVec3::new(lp.0 + p.off.x, p.off.y.max(0.0), lp.1 + p.off.z)
        }

        /// Everyone near an explosion runs away from it.
        pub fn panic(&mut self, x: f64, z: f64, radius: f64) {
            for p in &mut self.people {
                if p.down > 0.0 {
                    continue;
                }
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
                if p.dodging > 0.0 || p.fleeing > 0.0 || p.down > 0.0 || p.fighting > 0.0 {
                    continue;
                }
                let at = loop_point(p);
                if (at.0 - x).hypot(at.1 - z) < radius {
                    p.cheering = 1.5 + rng::random() * 1.5;
                }
            }
        }

        /// A fight breaks out at (x, z): people near it run, and the bold
        /// ones within `bold` meters come to fight the player.
        pub fn alarm(&mut self, x: f64, z: f64, radius: f64, bold: f64) {
            for p in &mut self.people {
                if p.down > 0.0 || p.fighting > 0.0 {
                    continue;
                }
                let at = Self::at(p);
                let d = (at.x - x).hypot(at.z - z);
                if d > radius {
                    continue;
                }
                if d < bold && rng::random() < 0.18 {
                    p.fighting = 10.0 + rng::random() * 6.0;
                    p.cheering = 0.0;
                } else {
                    p.fleeing = 5.0 + rng::random() * 4.0;
                    p.facing = Some((at.x - x).atan2(at.z - z));
                    p.cheering = 0.0;
                }
            }
        }

        /// A punch from `at` facing `dir` (length 1): everyone within `reach`
        /// in front takes `damage`. Returns who was hit.
        pub fn strike(&mut self, at: DVec3, dir: DVec2, reach: f64, damage: f64) -> Vec<Hit> {
            let mut hits = Vec::new();
            for i in 0..self.people.len() {
                let p = &self.people[i];
                if p.down > 0.0 {
                    continue;
                }
                let me = Self::at(p);
                let to = DVec2::new(me.x - at.x, me.z - at.z);
                let d = to.length();
                if d > reach || (d > 0.3 && to.dot(dir) / d < 0.35) || (me.y - (at.y - 0.3)).abs() > 1.5 {
                    continue;
                }
                let out = self.hurt(i, damage, DVec2::new(at.x, at.z));
                hits.push(Hit { at: me, knocked_out: out });
            }
            hits
        }

        /// Where the first person a shot from `origin` in `dir` (length 1)
        /// hits is, within `max`: (who, distance).
        pub fn shoot(&self, origin: DVec3, dir: DVec3, max: f64) -> Option<(usize, f64)> {
            let mut best: Option<(usize, f64)> = None;
            for (i, p) in self.people.iter().enumerate() {
                if p.down > 0.0 {
                    continue;
                }
                let me = Self::at(p);
                if (me.x - origin.x).abs() > max || (me.z - origin.z).abs() > max {
                    continue;
                }
                if let Some(t) = person_hit(origin, dir, me, 1.0)
                    && t <= max
                    && best.is_none_or(|b| t < b.1)
                {
                    best = Some((i, t));
                }
            }
            best
        }

        /// Hurts someone (hit from `from`). True if that knocked them out.
        pub fn hurt(&mut self, i: usize, damage: f64, from: DVec2) -> bool {
            let p = &mut self.people[i];
            if p.down > 0.0 {
                return false;
            }
            let lp = loop_point(p);
            let me = DVec2::new(lp.0 + p.off.x, lp.1 + p.off.z);
            p.hp -= damage;
            p.cheering = 0.0;
            if p.hp <= 0.0 {
                p.down = 25.0 + rng::random() * 25.0;
                p.fell = (me.x - from.x).atan2(me.y - from.y);
                p.fighting = 0.0;
                p.fleeing = 0.0;
                return true;
            }
            if p.fighting <= 0.0 && rng::random() < 0.3 {
                p.fighting = 12.0;
            } else if p.fighting <= 0.0 {
                p.fleeing = 6.0 + rng::random() * 3.0;
                p.facing = Some((me.x - from.x).atan2(me.y - from.y));
            }
            false
        }

        /// Where the person at index `i` is.
        pub fn position(&self, i: usize) -> DVec3 {
            Self::at(&self.people[i])
        }

        /// The nearest person (standing) within `range` and `cone` (cosine) of
        /// straight ahead from `at` in `dir`: where their chest is.
        pub fn aim_at(&self, at: DVec3, dir: DVec2, range: f64, cone: f64) -> Option<DVec3> {
            let mut best: Option<(f64, DVec3)> = None;
            for p in &self.people {
                if p.down > 0.0 {
                    continue;
                }
                let me = Self::at(p);
                let to = DVec2::new(me.x - at.x, me.z - at.z);
                let d = to.length();
                if d < 0.5 || d > range || to.dot(dir) / d < cone {
                    continue;
                }
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, me + DVec3::Y * 1.25));
                }
            }
            best.map(|b| b.1)
        }

        /// Moves everyone. `player` is the player on foot, if they are. Returns
        /// the damage people fighting the player did to them.
        pub fn update(&mut self, dt: f64, car: CarNear, player: Option<DVec3>) -> f64 {
            self.near = DVec2::new(car.x, car.z);
            if self.people.is_empty() || !self.visible {
                return 0.0;
            }
            self.clock += dt;
            let speed = car.vx.hypot(car.vz);
            let mut damage = 0.0;
            for p in &mut self.people {
                let lp0 = loop_point(p);
                let (wx, wz) = (lp0.0 + p.off.x, lp0.1 + p.off.z);
                if (wx - car.x).abs() > AWAKE || (wz - car.z).abs() > AWAKE {
                    p.down = (p.down - dt).max(0.0);
                    if p.down <= 0.0 && p.hp <= 0.0 {
                        p.hp = HEALTH;
                    }
                    continue;
                }
                p.dodging = (p.dodging - dt).max(0.0);
                p.fleeing = (p.fleeing - dt).max(0.0);
                p.cheering = (p.cheering - dt).max(0.0);
                p.punch = (p.punch - dt).max(0.0);
                if p.down > 0.0 {
                    p.down -= dt;
                    if p.down <= 0.0 {
                        p.hp = HEALTH;
                        p.fleeing = 4.0;
                    }
                    continue;
                }
                // Fighting the player on foot: come at them, and punch.
                if p.fighting > 0.0 {
                    p.fighting -= dt;
                    if let Some(me) = player {
                        let to = DVec2::new(me.x - wx, me.z - wz);
                        let d = to.length();
                        p.facing = Some(to.x.atan2(to.y));
                        if d > 1.0 {
                            let go = to / d * 3.6 * dt;
                            p.off.x += go.x;
                            p.off.z += go.y;
                            p.step += 3.6 * dt * 4.2;
                        } else if p.punch <= 0.0 {
                            p.punch = 1.1 + rng::random() * 0.5;
                            damage += 6.0 + rng::random() * 4.0;
                        }
                        if d > 40.0 {
                            p.fighting = 0.0;
                        }
                        continue;
                    }
                    p.fighting = 0.0;
                }
                let lp = loop_point(p);
                let blocked = speed <= 3.0 && (lp.0 + p.off.x - car.x).hypot(lp.1 + p.off.z - car.z) < car.reach + 2.5 && player.is_none();
                let walking = p.dodging <= 0.0 && p.cheering <= 0.0 && !blocked;
                let pace = if p.fleeing > 0.0 { 4.5 } else { p.pace };
                if walking {
                    if p.fleeing > 0.0
                        && let Some(f) = p.facing
                    {
                        // Running away across anything, then back to the sidewalk.
                        p.off.x += f.sin() * pace * dt;
                        p.off.z += f.cos() * pace * dt;
                    } else {
                        p.along = (p.along + p.way * pace * dt + PERIMETER).rem_euclid(PERIMETER);
                    }
                    p.step += pace * dt * 4.2;
                }
                if p.fleeing <= 0.0 {
                    p.facing = None;
                }
                let at = loop_point(p);
                let (x, z) = (at.0 + p.off.x, at.1 + p.off.z);
                let grounded = p.off.y <= 0.0 && p.vel.y <= 0.0;
                let (dx, dz) = (x - car.x, z - car.z);
                let near = dx.hypot(dz);
                if player.is_some() {
                    // The player is on foot: just step aside if they walk into you.
                    if near < 0.7 && grounded {
                        let k = (0.7 - near) / near.max(0.05);
                        p.off.x += dx * k;
                        p.off.z += dz * k;
                    }
                } else if grounded && speed <= 3.0 {
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
                } else if p.dodging <= 0.0 && !blocked && p.fleeing <= 0.0 {
                    p.off *= (-dt * 1.5).exp();
                }
            }
            damage
        }

        /// Everyone near the player, and `extra` people (the player, the police), as one mesh.
        pub fn mesh(&self, extra: &[PersonPose]) -> MeshData {
            let mut m = MeshData::default();
            let parts = Parts::new();
            for (i, p) in self.people.iter().enumerate() {
                let at = loop_point(p);
                let (x, z) = (at.0 + p.off.x, at.1 + p.off.z);
                if (x - self.near.x).abs() > AWAKE || (z - self.near.y).abs() > AWAKE {
                    continue;
                }
                let walking_heading = (at.2 * p.way).atan2(at.3 * p.way);
                let heading = if p.down > 0.0 {
                    p.fell + std::f64::consts::PI
                } else {
                    p.facing.unwrap_or(walking_heading)
                };
                let dive = if p.dodging > 0.0 { p.lean * p.dodging.min(1.0) * 1.1 } else { 0.0 };
                let moving = p.dodging <= 0.0 && p.cheering <= 0.0 && p.down <= 0.0;
                let arms = if p.down > 0.0 {
                    Arms::Up(std::f32::consts::PI * 0.9)
                } else if p.fighting > 0.0 {
                    if p.punch > 0.8 {
                        Arms::Punch(((1.1 - p.punch) / 0.3).clamp(0.0, 1.0) as f32)
                    } else {
                        Arms::Guard
                    }
                } else if p.cheering > 0.0 {
                    Arms::Up(std::f32::consts::PI * (0.85 + ((self.clock * 12.0 + i as f64).sin() * 0.1) as f32))
                } else if p.dodging > 0.0 || p.fleeing > 0.0 {
                    Arms::Up(std::f32::consts::PI * 0.8)
                } else {
                    Arms::Swing
                };
                let lying = if p.down > 0.0 { 1.0 } else { 0.0 };
                draw_person(
                    &mut m,
                    &parts,
                    &PersonPose {
                        at: Vec3::new(x as f32, p.off.y.max(0.0) as f32, z as f32),
                        heading: heading as f32,
                        step: p.step as f32,
                        stride: match (moving, p.fleeing > 0.0 || p.fighting > 0.0) {
                            (true, true) => 0.9,
                            (true, false) => 0.5,
                            _ => 0.0,
                        },
                        lying,
                        dive: dive as f32,
                        arms,
                        colors: p.colors,
                        pistol: false,
                        scale: 1.0,
                        cap: None,
                        bounce: if moving { 0.04 } else { 0.0 },
                    },
                );
            }
            for pose in extra {
                draw_person(&mut m, &parts, pose);
            }
            m
        }
    }

    /// The distance along a shot (from `origin`, direction `dir`) at which it
    /// hits someone standing at `feet`, `scale` times normal size.
    pub fn person_hit(origin: DVec3, dir: DVec3, feet: DVec3, scale: f64) -> Option<f64> {
        let chest = feet + DVec3::Y * 1.1 * scale;
        let t = (chest - origin).dot(dir);
        if t < 0.0 {
            return None;
        }
        let closest = origin + dir * t;
        let off = closest - chest;
        let side = DVec2::new(off.x, off.z).length();
        (side < 0.38 * scale && off.y.abs() < 0.85 * scale).then_some(t)
    }

    /// The shapes people are made of.
    pub struct Parts {
        leg: Mesh,
        arm: Mesh,
        body: Mesh,
        head: Mesh,
        gun: Mesh,
        cap: Mesh,
    }

    impl Parts {
        pub fn new() -> Self {
            Parts {
                leg: bevy::shape::Cuboid::new(0.17, 0.86, 0.19).mesh().build(),
                arm: bevy::shape::Cuboid::new(0.12, 0.62, 0.13).mesh().build(),
                body: bevy::shape::Cuboid::new(0.44, 0.62, 0.25).mesh().build(),
                head: bevy::shape::Sphere::new(0.14).mesh().uv(10, 8),
                gun: bevy::shape::Cuboid::new(0.06, 0.2, 0.12).mesh().build(),
                cap: bevy::shape::Cylinder::new(0.16, 0.1).mesh().resolution(10).build(),
            }
        }
    }

    impl Default for Parts {
        fn default() -> Self {
            Self::new()
        }
    }

    /// Adds one person to a mesh.
    pub fn draw_person(m: &mut MeshData, parts: &Parts, pose: &PersonPose) {
        use std::f32::consts::{FRAC_PI_2, PI};
        let swing = pose.step.sin() * pose.stride;
        let bounce = if pose.stride > 0.0 {
            pose.step.sin().abs() * pose.bounce
        } else {
            0.0
        };
        // Lying down: tipped onto the back, resting on the ground.
        let lift = pose.lying * 0.13;
        let base = Transform::from_translation(pose.at + Vec3::Y * (bounce + lift))
            .with_rotation(Quat::from_euler(EulerRot::YXZ, pose.heading, -FRAC_PI_2 * pose.lying, pose.dive))
            .with_scale(Vec3::splat(pose.scale));
        let part = |px: f32, py: f32, rx: f32, rz: f32, hang: f32| {
            base * Transform::from_xyz(px, py, 0.0).with_rotation(Quat::from_euler(EulerRot::XYZ, rx, 0.0, rz))
                * Transform::from_xyz(0.0, hang, 0.0)
        };
        let [skin, shirt, trousers] = pose.colors;
        m.add_mesh(&parts.leg, &part(-0.1, 0.9, swing, 0.0, -0.43), trousers);
        m.add_mesh(&parts.leg, &part(0.1, 0.9, -swing, 0.0, -0.43), trousers);
        m.add_mesh(&parts.body, &part(0.0, 0.88, 0.0, 0.0, 0.31), shirt);
        // Arms hang from the shoulders; x rotation swings them forward (negative) and back.
        let (left, right) = match pose.arms {
            Arms::Swing => ((-swing, 0.0), (swing, 0.0)),
            Arms::Up(up) => ((0.0, -up), (0.0, up)),
            Arms::Aim => ((-swing * 0.5, 0.0), (-FRAC_PI_2 - 0.05, 0.0)),
            Arms::Punch(k) => ((-1.1, 0.0), (-1.1 - k * 0.55, 0.0)),
            Arms::Guard => ((-1.2, 0.15), (-1.2, -0.15)),
        };
        m.add_mesh(
            &parts.arm,
            &part(
                -0.29,
                1.46,
                left.0 * if matches!(pose.arms, Arms::Up(_)) { 0.0 } else { 1.0 },
                left.1,
                -0.31,
            ),
            shirt,
        );
        m.add_mesh(&parts.arm, &part(0.29, 1.46, right.0, right.1, -0.31), shirt);
        m.add_mesh(&parts.head, &part(0.0, 1.66, 0.0, 0.0, 0.0), skin);
        if let Some(cap) = pose.cap {
            m.add_mesh(&parts.cap, &part(0.0, 1.8, 0.0, 0.0, 0.0), cap);
        }
        if pose.pistol {
            // In the right hand, pointing the way the arm does.
            m.add_mesh(&parts.gun, &part(0.29, 1.46, right.0, right.1, -0.66), linear(0x1c1c1e));
        }
        let _ = PI;
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

pub mod walk {
    //! You, on foot: walking and running about (W and S, A and D to turn,
    //! Shift to run, Space to jump), getting in and out of cars (F), and, if
    //! fighting is on (Garage → Maps), your fists and a pistol.
    use bevy::math::{DVec2, DVec3, Vec3};

    use crate::meshes::linear;
    use crate::pedestrians::{Arms, PersonPose};
    use crate::physics::CarInput;
    use crate::track::WALL_THICKNESS;
    use crate::world::World;

    pub const RADIUS: f64 = 0.35;
    pub const HEALTH: f64 = 100.0;
    const WALK: f64 = 2.4;
    const RUN: f64 = 7.0;
    const BACK: f64 = 1.8;
    const TURN: f64 = 3.0;
    const JUMP: f64 = 4.8;
    const GRAVITY: f64 = 18.0;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Weapon {
        Fists,
        Pistol,
    }

    impl Weapon {
        pub fn name(self) -> &'static str {
            match self {
                Weapon::Fists => "Fists",
                Weapon::Pistol => "Pistol",
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct OnFoot {
        /// Where your feet are.
        pub pos: DVec3,
        pub heading: f64,
        pub vy: f64,
        /// Forward speed (backwards is negative).
        pub speed: f64,
        pub airborne: bool,
        pub hp: f64,
        pub weapon: Weapon,
        step: f64,
        /// Seconds left of a punch, of showing the pistol held out, and until you can attack again.
        pub punching: f64,
        pub aiming: f64,
        pub cooldown: f64,
        since_hurt: f64,
        /// Seconds left lying knocked out (then you wake up at the hospital).
        pub knocked: f64,
    }

    impl OnFoot {
        pub fn new(pos: DVec3, heading: f64) -> Self {
            OnFoot {
                pos,
                heading,
                vy: 0.0,
                speed: 0.0,
                airborne: false,
                hp: HEALTH,
                weapon: Weapon::Fists,
                step: 0.0,
                punching: 0.0,
                aiming: 0.0,
                cooldown: 0.0,
                since_hurt: 10.0,
                knocked: 0.0,
            }
        }

        pub fn forward(&self) -> DVec2 {
            DVec2::new(self.heading.sin(), self.heading.cos())
        }

        /// Velocity on the ground (x, z).
        pub fn velocity(&self) -> DVec2 {
            self.forward() * self.speed
        }

        /// Takes a hit. True if it knocked you out.
        pub fn hurt(&mut self, damage: f64) -> bool {
            if self.knocked > 0.0 || damage <= 0.0 {
                return false;
            }
            self.hp -= damage;
            self.since_hurt = 0.0;
            if self.hp <= 0.0 {
                self.hp = 0.0;
                self.knocked = 3.5;
                self.speed = 0.0;
                return true;
            }
            false
        }

        /// One step of walking: turning, moving, jumping, and staying out of things.
        pub fn step(&mut self, dt: f64, input: &CarInput, world: &World) {
            self.punching = (self.punching - dt).max(0.0);
            self.aiming = (self.aiming - dt).max(0.0);
            self.cooldown = (self.cooldown - dt).max(0.0);
            self.since_hurt += dt;
            if self.since_hurt > 5.0 && self.knocked <= 0.0 {
                self.hp = (self.hp + 8.0 * dt).min(HEALTH);
            }
            let ground = world.height_at(self.pos.x, self.pos.z);
            if self.knocked > 0.0 {
                self.knocked -= dt;
                self.speed = 0.0;
                self.pos.y = ground;
                return;
            }
            self.heading += input.steer * TURN * dt;
            let run = input.boost;
            let want = input.throttle * if run { RUN } else { WALK } - input.brake * BACK;
            let accel = if self.airborne { 4.0 } else { 22.0 };
            self.speed += (want - self.speed).clamp(-accel * dt, accel * dt);
            let v = self.forward() * self.speed;
            self.pos.x += v.x * dt;
            self.pos.z += v.y * dt;
            if (input.handbrake || input.jump) && !self.airborne {
                self.vy = JUMP;
                self.airborne = true;
                self.pos.y += 0.05;
            }
            let ground = world.height_at(self.pos.x, self.pos.z);
            if self.airborne || self.pos.y > ground + 0.45 {
                self.airborne = true;
                self.vy -= GRAVITY * dt;
                self.pos.y += self.vy * dt;
                if self.pos.y <= ground {
                    self.pos.y = ground;
                    self.vy = 0.0;
                    self.airborne = false;
                }
            } else {
                self.pos.y = ground;
            }
            self.collide(world);
            self.step += self.speed.abs() * dt * 2.7;
        }

        /// Pushes you out of buildings, cars, posts and walls.
        fn collide(&mut self, world: &World) {
            let (x, z) = (self.pos.x, self.pos.z);
            let (circles, walls) = world.nearby(x, z, 1.5);
            for i in world.boxes_near(x, z, 1.5) {
                let b = world.boxes[i];
                if !b.solid || self.pos.y >= b.base + b.height - 0.05 || self.pos.y + 1.7 < b.base {
                    continue;
                }
                let px = self.pos.x.clamp(b.min_x, b.max_x);
                let pz = self.pos.z.clamp(b.min_z, b.max_z);
                self.push_from(px, pz, RADIUS);
            }
            for i in circles {
                let c = world.circles[i];
                if !c.solid || self.pos.y > c.base + c.height || self.pos.y + 1.7 < c.base {
                    continue;
                }
                let (dx, dz) = (self.pos.x - c.x, self.pos.z - c.z);
                let d = dx.hypot(dz).max(1e-4);
                if d < c.r + RADIUS {
                    self.pos.x = c.x + dx / d * (c.r + RADIUS);
                    self.pos.z = c.z + dz / d * (c.r + RADIUS);
                }
            }
            for i in walls {
                let w = world.walls[i];
                if self.pos.y > w.top {
                    continue;
                }
                let (sx, sz) = (w.bx - w.ax, w.bz - w.az);
                let len2 = (sx * sx + sz * sz).max(1e-9);
                let t = (((self.pos.x - w.ax) * sx + (self.pos.z - w.az) * sz) / len2).clamp(0.0, 1.0);
                self.push_from(w.ax + sx * t, w.az + sz * t, RADIUS + WALL_THICKNESS / 2.0);
            }
        }

        fn push_from(&mut self, px: f64, pz: f64, r: f64) {
            let (dx, dz) = (self.pos.x - px, self.pos.z - pz);
            let d = dx.hypot(dz);
            if d >= r {
                return;
            }
            let (nx, nz) = if d > 1e-4 {
                (dx / d, dz / d)
            } else {
                (-self.heading.sin(), -self.heading.cos())
            };
            self.pos.x = px + nx * r;
            self.pos.z = pz + nz * r;
        }

        /// How to draw you.
        pub fn pose(&self) -> PersonPose {
            let arms = if self.knocked > 0.0 {
                Arms::Up(std::f32::consts::PI * 0.9)
            } else if self.punching > 0.0 {
                Arms::Punch((1.0 - (self.punching / 0.3 - 0.5).abs() * 2.0).clamp(0.0, 1.0) as f32)
            } else if self.weapon == Weapon::Pistol && self.aiming > 0.0 {
                Arms::Aim
            } else {
                Arms::Swing
            };
            let moving = self.speed.abs() > 0.2 && !self.airborne;
            PersonPose {
                at: Vec3::new(self.pos.x as f32, self.pos.y as f32, self.pos.z as f32),
                heading: self.heading as f32,
                step: self.step as f32,
                stride: if moving {
                    (self.speed.abs() / RUN).clamp(0.35, 1.0) as f32
                } else {
                    0.0
                },
                lying: if self.knocked > 0.0 { 1.0 } else { 0.0 },
                dive: 0.0,
                arms,
                colors: [linear(0xe0ac69), linear(0x2f6fb5), linear(0x26303f)],
                pistol: self.weapon == Weapon::Pistol && self.knocked <= 0.0,
                scale: 1.0,
                cap: None,
                bounce: 0.05,
            }
        }
    }
}

pub mod police {
    //! The police, in free roam without the Living City pack (which brings
    //! its own officer). Fighting people, firing a gun near them and pulling
    //! drivers out of their cars make you wanted, one to five stars. A
    //! patrol car comes; its officers get out and chase you on foot, and at
    //! three stars they fire back. Stand still next to one and you're
    //! busted (and fined). Get far enough away, or out of sight long
    //! enough, and you lose them.
    use bevy::math::{DVec2, DVec3};

    use crate::meshes::linear;
    use crate::pedestrians::{Arms, PersonPose, SKIN, person_hit};
    use crate::physics::CarInput;
    use crate::rng;
    use crate::walk::OnFoot;
    use crate::world::World;

    #[derive(Clone, Debug)]
    pub struct Unit {
        /// The patrol car (a car in the world: you can ram it or blow it up).
        pub target: usize,
        pub heading: f64,
        pub speed: f64,
        siren: f64,
        /// Stopped, with the officers out.
        pub parked: bool,
    }

    #[derive(Clone, Debug)]
    pub struct Officer {
        /// They walk and run like you do, and can't go through walls.
        pub body: OnFoot,
        pub hp: f64,
        /// Seconds left knocked out.
        pub down: f64,
        shoot: f64,
        punch: f64,
        skin: u32,
    }

    impl Officer {
        pub fn at(&self) -> DVec3 {
            self.body.pos
        }
    }

    /// Whether nothing (a building, a hill) is between two points.
    fn clear_line(world: &World, from: DVec3, to: DVec3) -> bool {
        let ray = to - from;
        let d = ray.length();
        d < 0.5 || world.raycast(from, ray / d, d).is_none_or(|h| h.distance >= d - 0.6)
    }

    /// What the police did this frame.
    #[derive(Clone, Debug, PartialEq)]
    pub enum Report {
        /// A patrol car is wanted at this spot, heading this way.
        CallCar(f64, f64, f64),
        Busted(f64),
        Lost,
        Siren,
        Shot,
    }

    #[derive(Default)]
    pub struct Police {
        /// Stars, 0 to 5.
        pub wanted: f64,
        pub unit: Option<Unit>,
        pub officers: Vec<Officer>,
        /// Seconds since the police last saw you.
        unseen: f64,
        next_call: f64,
        arrest: f64,
        clock: f64,
        /// Damage the officers' shots and punches did to you since last asked.
        pub damage: f64,
    }

    impl Police {
        /// You did something the police care about: this many more stars.
        pub fn crime(&mut self, stars: f64) {
            if self.wanted < 0.5 {
                self.next_call = self.clock + 3.0;
            }
            self.wanted = (self.wanted + stars).min(5.0);
            self.unseen = 0.0;
        }

        pub fn stars(&self) -> usize {
            self.wanted.ceil() as usize
        }

        /// Takes the patrol car and officers away (a new map, an event).
        pub fn clear(&mut self, world: &mut World) {
            if let Some(u) = self.unit.take()
                && u.target < world.targets.len()
            {
                world.remove_car(u.target);
            }
            self.officers.clear();
            self.wanted = 0.0;
            self.arrest = 0.0;
        }

        /// A patrol car arrives (made by the game at the spot it asked for).
        pub fn set_unit(&mut self, target: usize, heading: f64) {
            self.unit = Some(Unit {
                target,
                heading,
                speed: 0.0,
                siren: 0.0,
                parked: false,
            });
        }

        /// Hurts officers a punch or shot reaches. True for each knocked out.
        pub fn strike(&mut self, at: DVec3, dir: DVec2, reach: f64, damage: f64) -> Vec<(DVec3, bool)> {
            let mut out = Vec::new();
            for o in &mut self.officers {
                if o.down > 0.0 {
                    continue;
                }
                let to = DVec2::new(o.body.pos.x - at.x, o.body.pos.z - at.z);
                let d = to.length();
                if d > reach || (d > 0.3 && to.dot(dir) / d < 0.35) {
                    continue;
                }
                out.push((o.body.pos, Self::hurt_officer(o, damage)));
            }
            out
        }

        fn hurt_officer(o: &mut Officer, damage: f64) -> bool {
            o.hp -= damage;
            if o.hp <= 0.0 {
                o.down = 40.0;
                return true;
            }
            false
        }

        /// The first officer a shot hits, within `max`: (index, distance).
        pub fn shoot(&self, origin: DVec3, dir: DVec3, max: f64) -> Option<(usize, f64)> {
            self.officers
                .iter()
                .enumerate()
                .filter(|(_, o)| o.down <= 0.0)
                .filter_map(|(i, o)| person_hit(origin, dir, o.body.pos, 1.0).filter(|&t| t <= max).map(|t| (i, t)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
        }

        /// A shot hit officer `i`. True if they're knocked out.
        pub fn shot(&mut self, i: usize, damage: f64) -> bool {
            Self::hurt_officer(&mut self.officers[i], damage)
        }

        /// The nearest standing officer in front, for the pistol to aim at.
        pub fn aim_at(&self, at: DVec3, dir: DVec2, range: f64, cone: f64) -> Option<DVec3> {
            self.officers
                .iter()
                .filter(|o| o.down <= 0.0)
                .filter_map(|o| {
                    let to = DVec2::new(o.body.pos.x - at.x, o.body.pos.z - at.z);
                    let d = to.length();
                    (d > 0.5 && d < range && to.dot(dir) / d >= cone).then_some((d, o.body.pos + DVec3::Y * 1.25))
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .map(|b| b.1)
        }

        /// Runs the police for `dt` seconds. `player` is where you are,
        /// `on_foot` whether you're out of the car, `speed` how fast you're going.
        pub fn update(&mut self, dt: f64, player: DVec3, on_foot: bool, speed: f64, world: &mut World) -> Vec<Report> {
            self.clock += dt;
            let mut out = Vec::new();
            for o in &mut self.officers {
                o.down = (o.down - dt).max(0.0);
                o.shoot = (o.shoot - dt).max(0.0);
                o.punch = (o.punch - dt).max(0.0);
            }
            if self.wanted <= 0.0 {
                // Everyone goes home.
                if let Some(u) = &self.unit
                    && world
                        .targets
                        .get(u.target)
                        .is_none_or(|t| (t.x - player.x).hypot(t.z - player.z) > 90.0)
                {
                    let u = self.unit.take().unwrap();
                    world.remove_car(u.target);
                    self.officers.clear();
                }
                return out;
            }
            let Some(unit) = &mut self.unit else {
                if self.clock >= self.next_call {
                    self.next_call = self.clock + 8.0;
                    let a = rng::random() * std::f64::consts::TAU;
                    let (x, z) = (player.x + a.sin() * 150.0, player.z + a.cos() * 150.0);
                    out.push(Report::CallCar(x, z, (player.x - x).atan2(player.z - z)));
                }
                self.cool(dt, &mut out);
                return out;
            };
            let Some(car) = world.targets.get(unit.target) else {
                self.unit = None;
                return out;
            };
            if !car.alive || car.rig.is_none() {
                // Wrecked: the officers get out where it is.
                if self.officers.is_empty() && car.rig.is_some() {
                    self.spawn_officers(DVec3::new(car.x, car.pos.y, car.z), 1);
                }
                if car.rig.is_none() {
                    self.unit = None;
                }
            }
            let Some(unit) = &mut self.unit else {
                self.cool(dt, &mut out);
                return out;
            };
            let car = &world.targets[unit.target];
            let car_at = DVec2::new(car.x, car.z);
            let me = DVec2::new(player.x, player.z);
            let distance = car_at.distance(me);
            let seen = distance < 140.0
                || self
                    .officers
                    .iter()
                    .any(|o| o.down <= 0.0 && DVec2::new(o.body.pos.x, o.body.pos.z).distance(me) < 60.0);
            if seen {
                self.unseen = 0.0;
            }
            unit.siren -= dt;
            if unit.siren <= 0.0 && distance < 160.0 {
                unit.siren = 1.6;
                out.push(Report::Siren);
            }
            let alive = car.alive;
            if alive && !unit.parked {
                // Drive at the player; stop beside them if they're on foot or slow.
                let to = me - car_at;
                let goal = to.x.atan2(to.y);
                let turn = (goal - unit.heading + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                unit.heading += turn.clamp(-2.2 * dt, 2.2 * dt);
                // Something in the way just ahead (a parked car, a building): stop there.
                let ahead = car_at + DVec2::new(unit.heading.sin(), unit.heading.cos()) * (3.5 + unit.speed * 0.25);
                let target = unit.target;
                let blocked = world.boxes_near(ahead.x, ahead.y, 3.0).into_iter().any(|i| {
                    let b = &world.boxes[i];
                    b.solid
                        && b.target != Some(target)
                        && ahead.x > b.min_x - 1.2
                        && ahead.x < b.max_x + 1.2
                        && ahead.y > b.min_z - 1.2
                        && ahead.y < b.max_z + 1.2
                });
                let desired = if distance < 12.0 || blocked {
                    0.0
                } else {
                    (distance * 0.9).clamp(6.0, 34.0) * (1.0 - turn.abs().min(1.2) * 0.5)
                };
                if blocked {
                    unit.speed = unit.speed.min(2.0);
                }
                unit.speed += (desired - unit.speed).clamp(-16.0 * dt, 8.0 * dt);
                let step = DVec2::new(unit.heading.sin(), unit.heading.cos()) * unit.speed * dt;
                let (x, z, h) = (car_at.x + step.x, car_at.y + step.y, unit.heading);
                world.move_car(unit.target, x, z, h);
                if (distance < 14.0 || (blocked && distance < 60.0)) && (on_foot || speed < 3.0) && unit.speed < 4.0 {
                    unit.parked = true;
                    let n = if self.wanted >= 3.0 { 2 } else { 1 };
                    if self.officers.is_empty() {
                        let side = DVec2::new(h.cos(), -h.sin()) * 1.8;
                        self.spawn_officers(DVec3::new(x + side.x, player.y, z + side.y), n);
                    }
                }
            }
            if let Some(unit) = &mut self.unit
                && unit.parked
                && !on_foot
                && speed > 8.0
                && self
                    .officers
                    .iter()
                    .all(|o| o.down > 0.0 || DVec2::new(o.body.pos.x, o.body.pos.z).distance(car_at) < 3.0)
            {
                // You drove off: back in the car, after you.
                unit.parked = false;
                self.officers.clear();
            }
            // The officers on foot.
            let mut arrested = false;
            let armed = self.wanted >= 3.0;
            for o in &mut self.officers {
                if o.down > 0.0 {
                    continue;
                }
                let to = DVec2::new(player.x - o.body.pos.x, player.z - o.body.pos.z);
                let d = to.length();
                let sees = clear_line(world, o.body.pos + DVec3::Y * 1.4, player + DVec3::Y * 1.2);
                let goal = to.x.atan2(to.y);
                let turn = (goal - o.body.heading + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                let shooting = armed && sees && (4.0..28.0).contains(&d) && on_foot;
                let input = CarInput {
                    throttle: if d > 1.1 && !shooting { 1.0 } else { 0.0 },
                    boost: d > 6.0,
                    steer: (turn * 3.0).clamp(-1.0, 1.0),
                    ..Default::default()
                };
                o.body.step(dt, &input, world);
                if shooting && o.shoot <= 0.0 {
                    o.shoot = 1.3 + rng::random() * 0.6;
                    o.body.aiming = 1.5;
                    out.push(Report::Shot);
                    if rng::random() < 0.55 {
                        self.damage += 7.0;
                    }
                } else if d <= 1.4 && sees {
                    arrested = true;
                }
            }
            self.arrest = if arrested && speed < 2.5 {
                self.arrest + dt
            } else {
                (self.arrest - dt).max(0.0)
            };
            if self.arrest > 1.6 {
                let fine = (self.wanted * 120.0).round();
                self.wanted = 0.0;
                self.arrest = 0.0;
                out.push(Report::Busted(fine));
                return out;
            }
            if distance > 320.0 {
                self.unseen += dt * 3.0;
            } else if !seen {
                self.unseen += dt;
            }
            self.cool(dt, &mut out);
            out
        }

        /// Out of sight, the stars fade, and in the end the police give up.
        fn cool(&mut self, dt: f64, out: &mut Vec<Report>) {
            if self.unseen > 12.0 {
                self.wanted -= dt * 0.25;
                if self.wanted <= 0.0 {
                    self.wanted = 0.0;
                    out.push(Report::Lost);
                }
            }
            self.unseen += if self.unit.is_none() { dt } else { 0.0 };
        }

        fn spawn_officers(&mut self, at: DVec3, n: usize) {
            for k in 0..n {
                self.officers.push(Officer {
                    body: OnFoot::new(at + DVec3::new(k as f64 * 0.9, 0.0, 0.0), 0.0),
                    hp: 100.0,
                    down: 0.0,
                    shoot: 1.0,
                    punch: 0.0,
                    skin: rng::pick(&SKIN),
                });
            }
        }

        /// How to draw the officers.
        pub fn poses(&self) -> Vec<PersonPose> {
            self.officers
                .iter()
                .map(|o| {
                    let mut pose = o.body.pose();
                    if o.down > 0.0 {
                        pose.lying = 1.0;
                        pose.stride = 0.0;
                        pose.arms = Arms::Up(2.8);
                    } else if self.wanted >= 3.0 && o.body.aiming > 0.0 {
                        pose.arms = Arms::Aim;
                    }
                    pose.colors = [linear(o.skin), linear(0x1d2c4a), linear(0x141c2b)];
                    pose.pistol = self.wanted >= 3.0 && o.down <= 0.0;
                    pose.cap = Some(linear(0x16233a));
                    pose
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::{DVec2, DVec3};

    use crate::pedestrians::{CarNear, Pedestrians};
    use crate::police::{Police, Report};
    use crate::walk::OnFoot;
    use crate::world::{BoxShape, World};

    fn still_car(x: f64, z: f64) -> CarNear {
        CarNear {
            x,
            z,
            vx: 0.0,
            vz: 0.0,
            reach: 0.4,
        }
    }

    #[test]
    fn punches_knock_people_out_and_they_get_back_up() {
        let mut people = Pedestrians::default();
        people.visible = true;
        people.add_town(&[DVec2::new(0.0, 0.0)]);
        let target = people.position(0);
        let from = target - DVec3::new(0.0, 0.0, 1.0);
        let mut out = false;
        for _ in 0..3 {
            for h in people.strike(from + DVec3::Y * 1.2, DVec2::new(0.0, 1.0), 1.5, 34.0) {
                out |= h.knocked_out;
            }
        }
        assert!(out, "three punches knock someone out");
        // Knocked out: punches and shots go past them.
        assert!(
            people
                .strike(from + DVec3::Y * 1.2, DVec2::new(0.0, 1.0), 1.5, 34.0)
                .iter()
                .all(|h| (h.at - target).length() > 0.5)
        );
        assert!(people.shoot(from + DVec3::Y * 1.4, DVec3::Z, 10.0).is_none_or(|(i, _)| i != 0));
        // A minute later, they're up again.
        for _ in 0..600 {
            people.update(0.1, still_car(from.x, from.z), Some(from));
        }
        assert!(people.aim_at(from - DVec3::Z * 5.0, DVec2::new(0.0, 1.0), 200.0, -1.0).is_some());
    }

    #[test]
    fn a_shot_hits_the_first_person_in_line() {
        let mut people = Pedestrians::default();
        people.visible = true;
        people.add_town(&[DVec2::new(0.0, 0.0)]);
        let target = people.position(1);
        let origin = target + DVec3::new(-8.0, 1.4, 0.0);
        let hit = people.shoot(origin, DVec3::X, 30.0);
        assert!(hit.is_some_and(|(_, t)| (t - 8.0).abs() < 0.6), "{hit:?}");
        assert!(people.shoot(origin, -DVec3::X, 30.0).is_none_or(|(_, t)| t > 9.0));
    }

    #[test]
    fn the_police_come_and_arrest_you_standing_still() {
        let mut world = World::empty();
        let mut police = Police::default();
        police.crime(1.0);
        let me = DVec3::new(0.0, 0.0, 0.0);
        let mut called = None;
        for _ in 0..100 {
            for r in police.update(0.1, me, true, 0.0, &mut world) {
                if let Report::CallCar(x, z, h) = r {
                    called = Some((x, z, h));
                }
            }
            if called.is_some() {
                break;
            }
        }
        let (x, z, h) = called.expect("a patrol car is called");
        assert!((x.hypot(z) - 150.0).abs() < 1.0);
        // The car (a box in the world) drives up, the officer gets out and arrests you.
        world.boxes.push(BoxShape {
            min_x: x,
            max_x: x,
            min_z: z,
            max_z: z,
            height: 1.4,
            solid: true,
            target: Some(0),
            base: 0.0,
        });
        world.targets.push(crate::world::Target::new_public(
            crate::world::TargetKind::Car,
            x,
            z,
            70.0,
            1000.0,
            0,
        ));
        world.targets[0].rig = None;
        police.set_unit(0, h);
        // Without a model the car is treated as wrecked: the officers come on foot.
        let mut busted = false;
        for _ in 0..600 {
            if police
                .update(0.1, me, true, 0.0, &mut world)
                .iter()
                .any(|r| matches!(r, Report::Busted(_)))
            {
                busted = true;
                break;
            }
        }
        assert!(busted || police.officers.is_empty(), "officers on foot arrest you");
    }

    #[test]
    fn walking_stops_at_walls() {
        let mut world = World::empty();
        world.boxes.push(BoxShape {
            min_x: -5.0,
            max_x: 5.0,
            min_z: 3.0,
            max_z: 8.0,
            height: 10.0,
            solid: true,
            target: None,
            base: 0.0,
        });
        world.index_new();
        let mut me = OnFoot::new(DVec3::ZERO, 0.0);
        let walk = crate::physics::CarInput {
            throttle: 1.0,
            boost: true,
            ..Default::default()
        };
        for _ in 0..120 {
            me.step(1.0 / 60.0, &walk, &world);
        }
        assert!(me.pos.z < 3.0 - crate::walk::RADIUS + 0.01, "stopped at the wall: {}", me.pos.z);
        assert!(me.pos.z > 2.0, "walked up to it: {}", me.pos.z);
        // Three hard hits knock you out; you get up after a few seconds.
        assert!(!me.hurt(40.0) && !me.hurt(40.0) && me.hurt(40.0));
        for _ in 0..300 {
            me.step(1.0 / 60.0, &walk, &world);
        }
        assert!(me.knocked <= 0.0);
    }
}
