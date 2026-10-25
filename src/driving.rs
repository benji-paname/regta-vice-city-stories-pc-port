//! Minimal arcade vehicle motion over the original static collision world.
//! This controller is an approximation; it does not reproduce the PS2 handling model.

use crate::collision::{ActorQuery, CollisionQuery, Hit, Ray};

const SKIN: f32 = 0.015;
const TOP_SPEED: f32 = 58.0; // m/s; provisional, not a decoded PS2 handling value.
const FIXED_STEP: f32 = 1.0 / 120.0;

#[derive(Clone, Copy, Debug)]
pub struct Impact {
    pub contact: Hit,
    pub velocity: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Car {
    pub position: [f32; 3],
    pub heading: f32,
    pub speed: f32,
    pub wheel_angle: f32,
    pub steering_angle: f32,
    pub grounded: bool,
    pub impact: Option<Impact>,
    vertical_speed: f32,
    reverse_hold: f32,
    pub suspension_displacements: [f32; 4],
    pub health: f32,
    pub burning: bool,
    pub burn_timer: f32,
    pub exploded: bool,
    pub is_police: bool,
    pub strobe_timer: f32,
    pub headlights: bool,
}

impl Car {
    pub fn new(position: [f32; 3], heading: f32) -> Self {
        Self {
            position,
            heading,
            speed: 0.0,
            wheel_angle: 0.0,
            steering_angle: 0.0,
            grounded: false,
            impact: None,
            vertical_speed: 0.0,
            reverse_hold: 0.0,
            suspension_displacements: [0.0; 4],
            health: 1000.0,
            burning: false,
            burn_timer: 0.0,
            exploded: false,
            is_police: false,
            strobe_timer: 0.0,
            headlights: true,
        }
    }

    pub fn suspension_displacements(&self) -> [f32; 4] {
        self.suspension_displacements
    }

    pub fn apply_damage(&mut self, amount: f32) {
        if self.exploded {
            return;
        }
        self.health = (self.health - amount).max(0.0);
        if self.health <= 0.0 {
            self.exploded = true;
            self.burning = false;
            self.speed = 0.0;
        } else if self.health < 250.0 && !self.burning {
            self.burning = true;
            self.burn_timer = 5.0;
        }
    }

    #[allow(dead_code)]
    pub fn is_destroyed(&self) -> bool {
        self.exploded
    }

    /// Align the car's wheel plane with the original static road surface.
    pub fn settle_on_ground(&mut self, world: &impl CollisionQuery, snap: bool, dt: f32) {
        let world = &ActorQuery(world);
        if !dt.is_finite() || dt < 0.0 {
            return;
        }
        let dt = dt.min(0.1);
        // Search near the wheels, never from twelve metres above the car:
        // that selected bridge decks/roofs when driving underneath them.
        let rise = 0.35;
        let fall = if snap {
            24.0
        } else {
            (-self.vertical_speed * dt).max(0.0) + 0.2
        };
        let (s, c) = self.heading.sin_cos();
        let mut height_sum = 0.0;
        let mut hit_count = 0;
        // 4 corners: 0: Front-Left, 1: Front-Right, 2: Rear-Left, 3: Rear-Right
        const CORNERS: [(f32, f32); 4] = [
            (1.55, -0.94),
            (1.55, 0.94),
            (-1.55, -0.94),
            (-1.55, 0.94),
        ];

        for (idx, &(longitudinal, lateral)) in CORNERS.iter().enumerate() {
            let x = self.position[0] + c * lateral + s * longitudinal;
            let y = self.position[1] - s * lateral + c * longitudinal;
            if let Some(hit) = world.raycast(
                [x, y, self.position[2] + rise],
                [0.0, 0.0, -1.0],
                rise + fall,
            ) {
                if hit.normal[2] > 0.55 {
                    height_sum += hit.position[2];
                    hit_count += 1;
                    let target_disp = (hit.position[2] - self.position[2]).clamp(-0.16, 0.16);
                    self.suspension_displacements[idx] +=
                        (target_disp - self.suspension_displacements[idx]) * (20.0 * dt).min(1.0);
                } else {
                    self.suspension_displacements[idx] +=
                        (-0.10 - self.suspension_displacements[idx]) * (10.0 * dt).min(1.0);
                }
            } else {
                self.suspension_displacements[idx] +=
                    (-0.10 - self.suspension_displacements[idx]) * (10.0 * dt).min(1.0);
            }
        }
        self.grounded = false;
        if hit_count >= 2 {
            self.position[2] = height_sum / hit_count as f32 + SKIN;
            self.vertical_speed = 0.0;
            self.grounded = true;
        } else if !snap {
            self.vertical_speed = (self.vertical_speed - 18.0 * dt).max(-50.0);
            self.position[2] += self.vertical_speed * dt;
        }
    }

    /// Signed pedal: forward accelerates, backward brakes then reverses.
    /// The separate brake input always stops the car (Space), without reversing.
    pub fn step(
        &mut self,
        world: &impl CollisionQuery,
        throttle: f32,
        brake: f32,
        steer: f32,
        dt: f32,
    ) {
        self.impact = None;
        if !dt.is_finite()
            || dt <= 0.0
            || !throttle.is_finite()
            || !brake.is_finite()
            || !steer.is_finite()
        {
            return;
        }
        let dt = dt.min(0.1);
        let ticks = (dt / FIXED_STEP).ceil().max(1.0) as usize;
        let world = &ActorQuery(world);
        for _ in 0..ticks {
            self.tick(world, throttle, brake, steer, dt / ticks as f32);
        }
    }

    fn tick(
        &mut self,
        world: &impl CollisionQuery,
        throttle: f32,
        brake: f32,
        steer: f32,
        dt: f32,
    ) {
        let throttle = if self.exploded { 0.0 } else { throttle.clamp(-1.0, 1.0) };
        let brake = if self.exploded { 1.0 } else { brake.clamp(0.0, 1.0) };
        let steer = if self.exploded { 0.0 } else { steer.clamp(-1.0, 1.0) };
        if self.burning && !self.exploded {
            self.burn_timer -= dt;
            if self.burn_timer <= 0.0 {
                self.exploded = true;
                self.health = 0.0;
                self.burning = false;
                self.speed = 0.0;
            }
        }
        if self.is_police {
            self.strobe_timer = (self.strobe_timer + dt * 4.0).fract();
        }
        self.settle_on_ground(world, false, dt);
        // Weight transfer: pitch under throttle/braking, roll under steering
        let pitch = (throttle * 0.02 - brake * 0.035).clamp(-0.04, 0.03);
        let roll = (self.steering_angle * (self.speed / TOP_SPEED) * 0.035).clamp(-0.03, 0.03);
        self.suspension_displacements[0] = (self.suspension_displacements[0] - pitch - roll).clamp(-0.16, 0.16);
        self.suspension_displacements[1] = (self.suspension_displacements[1] - pitch + roll).clamp(-0.16, 0.16);
        self.suspension_displacements[2] = (self.suspension_displacements[2] + pitch - roll).clamp(-0.16, 0.16);
        self.suspension_displacements[3] = (self.suspension_displacements[3] + pitch + roll).clamp(-0.16, 0.16);
        if self.grounded {
            if brake > 0.0 {
                self.speed = approach_zero(self.speed, 40.0 * brake * dt);
                self.reverse_hold = 0.0;
            } else if throttle * self.speed < 0.0 {
                self.speed = approach_zero(self.speed, 32.0 * throttle.abs() * dt);
                self.reverse_hold = 0.0;
            } else {
                // A brief stop separates braking from reverse. Holding S is enough.
                if throttle < 0.0 && self.speed == 0.0 {
                    self.reverse_hold += dt;
                } else {
                    self.reverse_hold = 0.0;
                }
                if throttle >= 0.0 || self.speed < 0.0 || self.reverse_hold >= 0.18 {
                    let acceleration = if throttle < 0.0 { 10.0 } else { 21.0 };
                    self.speed += throttle
                        * acceleration
                        * dt
                        * (1.0 - 0.6 * (self.speed.abs() / TOP_SPEED).powi(2));
                }
            }
            // Rolling resistance + aerodynamic drag. The old linear damping
            // prevented the provisional top speed from being approached.
            self.speed = approach_zero(self.speed, (0.35 + 0.0025 * self.speed * self.speed) * dt);
        }
        self.speed = self.speed.clamp(-16.0, TOP_SPEED);
        let speed_fraction = (self.speed.abs() / TOP_SPEED).clamp(0.0, 1.0);
        // Banshee handling +0x68 is 34 degrees. The PS2 code converts this
        // degree value to radians and applies a signed-square steering input.
        // The speed fade remains an arcade approximation until +0x70 is mapped.
        const BANSHEE_STEERING_LOCK_RAD: f32 = 34.0 * std::f32::consts::PI / 180.0;
        let steering_angle = BANSHEE_STEERING_LOCK_RAD * (1.0 - 0.72 * speed_fraction);
        let wanted_steering = steer * steer.abs() * steering_angle;
        self.steering_angle += (wanted_steering - self.steering_angle).clamp(-3.5 * dt, 3.5 * dt);
        let yaw_rate = if self.grounded {
            (self.steering_angle.tan() * self.speed / 2.75).clamp(-2.6, 2.6)
        } else {
            0.0
        };
        let turn = yaw_rate * dt;

        let (s, c) = (self.heading + turn * 0.5).sin_cos();
        let delta = [s * self.speed * dt, c * self.speed * dt];
        let distance = (delta[0] * delta[0] + delta[1] * delta[1]).sqrt();
        if distance <= 1e-6 {
            return;
        }
        let (allowed, contact) = self.motion_fraction(world, delta, turn);
        if let Some(contact) = contact {
            self.impact = Some(Impact {
                contact,
                velocity: [s * self.speed, c * self.speed, 0.],
            });
        }
        self.position[0] += delta[0] * allowed;
        self.position[1] += delta[1] * allowed;
        self.heading = (self.heading + turn * allowed + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        self.wheel_angle = (self.wheel_angle + distance * allowed * self.speed.signum() / 0.35)
            .rem_euclid(std::f32::consts::TAU);
        if allowed < 1.0 {
            if self.speed.abs() > 6.0 {
                let damage = (self.speed.abs() - 6.0) * 22.0;
                self.apply_damage(damage);
            }
            self.speed = 0.0;
        }
    }

    /// Sweep perimeter probes through translation AND rotation, in either gear.
    /// Still an approximate body: very thin obstacles between probes can be missed.
    fn motion_fraction(
        &self,
        world: &impl CollisionQuery,
        delta: [f32; 2],
        turn: f32,
    ) -> (f32, Option<Hit>) {
        let (s0, c0) = self.heading.sin_cos();
        let (s1, c1) = (self.heading + turn).sin_cos();
        let mut allowed: f32 = 1.0;
        let mut contact = None;
        let mut rays = Vec::with_capacity(448);
        let mut lengths = Vec::with_capacity(448);
        // Enclose the decoded Banshee collider: X [-1.1922,1.1951],
        // Y [-2.8298,2.9074]. The old +/-2.72 bumper was inside it.
        for longitudinal in [-2.85f32, 0.0, 2.92] {
            for slice in 0..=52 {
                let lateral = -1.21 + 2.42 * slice as f32 / 52.;
                if longitudinal == 0.0 && lateral.abs() < 1.16 {
                    continue;
                }
                let offset = [
                    c0 * lateral + s0 * longitudinal,
                    -s0 * lateral + c0 * longitudinal,
                ];
                let motion = [
                    delta[0] + c1 * lateral + s1 * longitudinal - offset[0],
                    delta[1] - s1 * lateral + c1 * longitudinal - offset[1],
                    0.0,
                ];
                let length = motion[0].hypot(motion[1]);
                if length < 1e-7 {
                    continue;
                }
                for height in [0.35f32, 0.5, 0.65, 0.85] {
                    let origin = [
                        self.position[0] + offset[0],
                        self.position[1] + offset[1],
                        self.position[2] + height,
                    ];
                    rays.push(Ray {
                        origin,
                        direction: motion,
                        distance: length + 0.04,
                    });
                    lengths.push(length);
                }
            }
        }
        for (hit, length) in world.raycast_batch(&rays).into_iter().zip(lengths) {
            if let Some(hit) = hit {
                if hit.normal[2].abs() < 0.55 {
                    let fraction = ((hit.distance - 0.04) / length).clamp(0., 1.);
                    if fraction < allowed {
                        allowed = fraction;
                        contact = Some(hit);
                    }
                }
            }
        }
        (allowed, contact)
    }
}

fn approach_zero(value: f32, amount: f32) -> f32 {
    value.signum() * (value.abs() - amount).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{CollisionWorld, Triangle};

    fn flat_world(z: f32) -> CollisionWorld {
        let a = [-2000.0, -2000.0, z];
        let b = [2000.0, -2000.0, z];
        let c = [2000.0, 2000.0, z];
        let d = [-2000.0, 2000.0, z];
        CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [a, b, c],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [a, c, d],
                surface: 0,
                model_id: 1,
            },
        ])
    }

    #[test]
    fn settles_all_wheels_on_flat_ground() {
        let mut car = Car::new([0.0, 0.0, 4.0], 0.0);
        car.settle_on_ground(&flat_world(0.0), true, 0.0);
        assert!((car.position[2] - 0.015).abs() < 1e-4);
    }

    #[test]
    fn throttle_moves_forward_and_turns() {
        let mut car = Car::new([0.0, 0.0, 0.015], 0.0);
        let world = flat_world(0.0);
        car.step(&world, 1.0, 0.0, 0.0, 0.1);
        assert!(car.speed > 1.0);
        assert!(car.position[1] > 0.0);
        for _ in 0..8 {
            car.step(&world, 1.0, 0.0, 1.0, 0.1);
        }
        assert!(car.heading > 0.2);
        assert!((car.position[2] - 0.015).abs() < 1e-3);
    }

    #[test]
    fn brake_reduces_speed_and_invalid_time_is_ignored() {
        let mut car = Car::new([0.0, 0.0, 0.0], 0.0);
        let world = flat_world(0.0);
        car.step(&world, 1.0, 0.0, 0.0, 0.1);
        let before = car.speed;
        car.step(&world, 0.0, 1.0, 0.0, 0.1);
        assert!(car.speed < before);
        let state = car;
        car.step(&world, 1.0, 0.0, 1.0, f32::NAN);
        assert_eq!(car.position, state.position);
        assert_eq!(car.heading, state.heading);
        assert_eq!(car.speed, state.speed);
    }

    fn advance(
        car: &mut Car,
        world: &impl CollisionQuery,
        pedal: f32,
        brake: f32,
        steer: f32,
        ticks: usize,
    ) {
        for _ in 0..ticks {
            car.step(world, pedal, brake, steer, FIXED_STEP);
        }
    }

    #[test]
    fn backward_pedal_brakes_then_reverses_and_forward_brakes_reverse() {
        let world = flat_world(0.0);
        let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
        car.speed = 12.0;
        advance(&mut car, &world, -1.0, 0.0, 0.0, 12);
        assert!(car.speed > 0.0 && car.speed < 12.0);
        advance(&mut car, &world, -1.0, 0.0, 0.0, 108);
        assert!(car.speed < -2.0);
        let reverse_position = car.position[1];
        advance(&mut car, &world, -1.0, 0.0, 0.0, 60);
        assert!(car.position[1] < reverse_position);
        advance(&mut car, &world, 1.0, 0.0, 0.0, 120);
        assert!(car.speed > 0.0);
    }

    #[test]
    fn handbrake_stops_and_holds_even_with_accelerator_held() {
        let world = flat_world(0.0);
        let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
        car.speed = 20.0;
        advance(&mut car, &world, 1.0, 1.0, 0.0, 180);
        assert_eq!(car.speed, 0.0);
        let position = car.position;
        advance(&mut car, &world, -1.0, 1.0, 0.0, 120);
        assert_eq!(car.position, position);
    }

    #[test]
    fn bridge_above_does_not_pull_car_off_road() {
        let mut triangles = flat_world(0.0).triangles;
        triangles.extend(flat_world(4.0).triangles);
        let world = CollisionWorld::from_triangles(triangles);
        let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
        car.settle_on_ground(&world, true, 0.0);
        advance(&mut car, &world, 1.0, 0.0, 0.0, 120);
        assert!((car.position[2] - SKIN).abs() < 1e-4);
        assert!(car.grounded);
    }

    #[test]
    fn stationary_car_falls_and_lands_without_pedal_input() {
        let world = flat_world(0.0);
        let mut car = Car::new([0.0, 0.0, 4.0], 0.0);
        advance(&mut car, &world, 0.0, 0.0, 0.0, 30);
        assert!(car.position[2] < 4.0 && !car.grounded);
        advance(&mut car, &world, 0.0, 0.0, 0.0, 150);
        assert!((car.position[2] - SKIN).abs() < 1e-4 && car.grounded);
    }

    #[test]
    fn front_and_rear_center_probes_stop_at_narrow_obstacle() {
        for sign in [-1.0, 1.0] {
            let y = sign * 4.0;
            let mut triangles = flat_world(0.0).triangles;
            triangles.extend([
                Triangle {
                    vertices: [[-0.1, y, 0.0], [0.1, y, 0.0], [0.1, y, 2.0]],
                    surface: 0,
                    model_id: 2,
                },
                Triangle {
                    vertices: [[-0.1, y, 0.0], [0.1, y, 2.0], [-0.1, y, 2.0]],
                    surface: 0,
                    model_id: 2,
                },
            ]);
            let world = CollisionWorld::from_triangles(triangles);
            let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
            car.speed = sign * 16.0;
            car.step(&world, 0.0, 0.0, 0.0, 0.1);
            assert_eq!(car.speed, 0.0);
            assert!(car.position[1].abs() + 2.72 < 4.0);
            advance(&mut car, &world, -sign, 0.0, 0.0, 120);
            assert!(
                car.speed * sign < 0.0,
                "must be able to back away after impact"
            );
        }
    }

    #[test]
    fn rotation_is_limited_before_front_corner_crosses_side_wall() {
        let mut triangles = flat_world(0.0).triangles;
        triangles.extend([
            Triangle {
                vertices: [[1.1, -20.0, 0.0], [1.1, 20.0, 0.0], [1.1, 20.0, 3.0]],
                surface: 0,
                model_id: 2,
            },
            Triangle {
                vertices: [[1.1, -20.0, 0.0], [1.1, 20.0, 3.0], [1.1, -20.0, 3.0]],
                surface: 0,
                model_id: 2,
            },
        ]);
        let world = CollisionWorld::from_triangles(triangles);
        let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
        car.speed = 12.0;
        advance(&mut car, &world, 1.0, 0.0, 1.0, 120);
        let (s, c) = car.heading.sin_cos();
        let front_corner_x = car.position[0] + 0.94 * c + 2.72 * s;
        assert!(front_corner_x < 1.1, "corner at {front_corner_x}");
        assert_eq!(car.speed, 0.0);
    }

    #[test]
    fn reaches_provisional_high_speed_and_frame_substeps_match() {
        let world = flat_world(0.0);
        let mut car = Car::new([0.0, 0.0, SKIN], 0.0);
        advance(&mut car, &world, 1.0, 0.0, 0.0, 1440);
        assert!(
            car.speed > 54.0 && car.speed <= TOP_SPEED,
            "speed {}",
            car.speed
        );
        let mut small = Car::new([0.0, 0.0, SKIN], 0.0);
        let mut large = small;
        advance(&mut small, &world, 1.0, 0.0, 0.4, 120);
        for _ in 0..10 {
            large.step(&world, 1.0, 0.0, 0.4, 0.1);
        }
        assert!((large.speed - small.speed).abs() < 0.001);
        for i in 0..3 {
            assert!((large.position[i] - small.position[i]).abs() < 0.001);
        }
    }

    #[test]
    fn independent_suspension_deflects_and_settles() {
        let world = flat_world(0.0);
        let mut car = Car::new([0.0, 0.0, 1.0], 0.0);
        car.settle_on_ground(&world, true, 0.016);
        assert!(car.grounded);
        let disp = car.suspension_displacements();
        for &d in &disp {
            assert!(d.abs() < 0.15, "Suspension should be within limits on flat ground");
        }
    }

    #[test]
    fn vehicle_damage_triggers_fire_and_explosion() {
        let mut car = Car::new([0.0, 0.0, 0.0], 0.0);
        assert_eq!(car.health, 1000.0);
        assert!(!car.burning);
        assert!(!car.exploded);

        // Light damage
        car.apply_damage(500.0);
        assert_eq!(car.health, 500.0);
        assert!(!car.burning);

        // Heavy damage triggers engine fire
        car.apply_damage(300.0);
        assert_eq!(car.health, 200.0);
        assert!(car.burning);
        assert_eq!(car.burn_timer, 5.0);

        // Step through fire countdown
        let world = flat_world(0.0);
        for _ in 0..60 {
            car.step(&world, 1.0, 0.0, 0.0, 0.1);
        }
        assert!(car.exploded);
        assert_eq!(car.health, 0.0);
        assert_eq!(car.speed, 0.0);
    }

    #[test]
    fn police_strobe_timer_advances_on_police_car() {
        let mut car = Car::new([0.0, 0.0, 0.0], 0.0);
        car.is_police = true;
        let world = flat_world(0.0);
        car.step(&world, 0.0, 0.0, 0.0, 0.25);
        assert!(car.strobe_timer > 0.0);
    }
}
