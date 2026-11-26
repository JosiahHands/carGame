//! Chase, far-chase and hood cameras for driving; an orbit camera in the
//! garage and in photo mode (drag to look around, scroll to zoom).
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Chase,
    Far,
    Hood,
}

impl CameraMode {
    pub fn label(self) -> &'static str {
        match self {
            CameraMode::Chase => "Chase camera",
            CameraMode::Far => "Far camera",
            CameraMode::Hood => "Hood camera",
        }
    }
}

/// An orbit around a point: the camera's direction and distance from it.
#[derive(Clone, Copy, Debug)]
pub struct Orbit {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub auto_rotate: bool,
    pub pan: bool,
    /// Smoothed changes still to apply (like OrbitControls' damping).
    spin: Vec2,
    zoom: f32,
}

#[derive(Resource)]
pub struct CameraRig {
    pub mode: CameraMode,
    pub eye_height: f32,
    /// How big the car is compared to a normal one.
    pub size: f32,
    pub base_fov: f32,
    pub fov: f32,
    pub garage: bool,
    pub orbit: Orbit,
    pub position: Vec3,
    pub look_at: Vec3,
    desired: Vec3,
}

impl Default for CameraRig {
    fn default() -> Self {
        CameraRig {
            mode: CameraMode::Chase,
            eye_height: 1.1,
            size: 1.0,
            base_fov: 62.0,
            fov: 62.0,
            garage: false,
            orbit: Orbit {
                target: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.3,
                distance: 7.0,
                min_distance: 4.0,
                max_distance: 14.0,
                auto_rotate: true,
                pan: false,
                spin: Vec2::ZERO,
                zoom: 0.0,
            },
            position: Vec3::new(0.0, 3.0, -7.0),
            look_at: Vec3::ZERO,
            desired: Vec3::ZERO,
        }
    }
}

impl CameraRig {
    pub fn next_mode(&mut self) -> CameraMode {
        self.mode = match self.mode {
            CameraMode::Chase => CameraMode::Far,
            CameraMode::Far => CameraMode::Hood,
            CameraMode::Hood => CameraMode::Chase,
        };
        self.mode
    }

    /// Switches the orbit camera on (garage, photo mode) or back to following the car.
    pub fn set_garage(&mut self, on: bool, car: Vec3, heading: f32, photo: bool) {
        self.garage = on;
        let s = self.size;
        let o = &mut self.orbit;
        o.auto_rotate = !photo;
        o.pan = photo;
        o.min_distance = if photo { 2.0 } else { 4.0 } * s;
        o.max_distance = if photo { 40.0 } else { 14.0 } * s;
        if on {
            o.target = Vec3::new(car.x, car.y + 0.7 * s, car.z);
            if !photo {
                let a = heading + std::f32::consts::PI * 0.8;
                self.position = Vec3::new(car.x + a.sin() * 7.0 * s, car.y + 2.2 * s, car.z + a.cos() * 7.0 * s);
            }
            // Read the orbit back from where the camera is.
            let off = self.position - o.target;
            o.distance = off.length().clamp(o.min_distance, o.max_distance);
            o.yaw = off.x.atan2(off.z);
            o.pitch = (off.y / off.length().max(0.001)).clamp(-1.0, 1.0).asin();
            o.spin = Vec2::ZERO;
            o.zoom = 0.0;
        } else {
            self.snap(car, heading);
        }
    }

    /// Jumps straight to the follow position.
    pub fn snap(&mut self, car: Vec3, heading: f32) {
        self.target(car, heading);
        self.position = self.desired;
    }

    fn target(&mut self, car: Vec3, heading: f32) {
        let (fx, fz) = (heading.sin(), heading.cos());
        let s = self.size;
        let (dist, height, ahead) = match self.mode {
            CameraMode::Far => (11.0 * s, 4.2 * s, 4.0 * s),
            CameraMode::Hood => (-0.3 * s, self.eye_height, 12.0 * s),
            CameraMode::Chase => (6.5 * s, 2.4 * s, 3.0 * s),
        };
        self.desired = Vec3::new(car.x - fx * dist, car.y + height, car.z - fz * dist);
        let look_y = if self.mode == CameraMode::Hood {
            self.eye_height - 0.2 * s
        } else {
            1.0 * s
        };
        self.look_at = Vec3::new(car.x + fx * ahead, car.y + look_y, car.z + fz * ahead);
    }

    /// Mouse drag (pixels) and scroll for the orbit camera.
    pub fn drag(&mut self, delta: Vec2, scroll: f32, panning: bool) {
        if !self.garage {
            return;
        }
        if panning && self.orbit.pan {
            let right = Vec3::new(self.orbit.yaw.cos(), 0.0, -self.orbit.yaw.sin());
            let up = Vec3::Y;
            let k = self.orbit.distance * 0.0015;
            self.orbit.target += (-right * delta.x + up * delta.y) * k;
        } else {
            self.orbit.spin += Vec2::new(-delta.x, delta.y) * 0.005;
        }
        self.orbit.zoom += -scroll * 0.1;
        if delta != Vec2::ZERO {
            self.orbit.auto_rotate = false;
        }
    }

    pub fn update(&mut self, dt: f32, car: Vec3, heading: f32, speed: f32, boosting: bool) {
        if self.garage {
            let o = &mut self.orbit;
            if o.auto_rotate {
                // OrbitControls' autoRotateSpeed 0.8: a turn every 75 seconds.
                o.yaw += std::f32::consts::TAU / 75.0 * dt;
            }
            let k = 1.0 - (-dt * 12.0).exp();
            o.yaw += o.spin.x * k;
            o.pitch = (o.pitch + o.spin.y * k).clamp(0.05, std::f32::consts::FRAC_PI_2 - 0.05);
            o.spin *= 1.0 - k;
            o.distance = (o.distance * (1.0 + o.zoom * k)).clamp(o.min_distance, o.max_distance);
            o.zoom *= 1.0 - k;
            let dir = Vec3::new(o.pitch.cos() * o.yaw.sin(), o.pitch.sin(), o.pitch.cos() * o.yaw.cos());
            self.position = o.target + dir * o.distance;
            self.look_at = o.target;
            self.fov += (self.base_fov - self.fov) * (dt * 6.0).min(1.0);
            return;
        }
        self.target(car, heading);
        if self.mode == CameraMode::Hood {
            self.position = self.desired;
        } else {
            let stiffness = 6.0 * (speed / 30.0).max(1.0);
            self.position = self.position.lerp(self.desired, 1.0 - (-dt * stiffness).exp());
            self.position.y = self.position.y.max(car.y + 0.6 * self.size);
        }
        let fov = self.base_fov + (speed * 0.22).min(14.0) + if boosting { 8.0 } else { 0.0 };
        self.fov += (fov - self.fov) * (dt * 3.0).min(1.0);
    }
}
