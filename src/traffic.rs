#![allow(dead_code)]
//! First ambient traffic controller over the original road graph.
//! Lane offset, speed and steering are provisional. Native lane counts restrict
//! the first pass to two-way roads; traffic lights remain unimplemented. Reuses the validated Banshee model.
use crate::collision::{
    CollisionInstance, CollisionQuery, CollisionScene, CollisionWorld, NativeCollider,
};
use crate::driving::Car;
use crate::paths::PathNetwork;

const LANE_OFFSET: f32 = 1.8;
const CRUISE_SPEED: f32 = 7.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficLightState {
    Green,
    Amber,
    Red,
}

#[derive(Clone, Debug)]
pub struct TrafficLight {
    pub position: [f32; 3],
    pub heading: f32,
    pub cycle_offset: f32,
}

impl TrafficLight {
    pub fn heading(&self) -> f32 {
        self.heading
    }

    pub fn state_at(&self, time: f32) -> TrafficLightState {
        let t = (time + self.cycle_offset).rem_euclid(32.0);
        if t < 14.0 {
            TrafficLightState::Green
        } else if t < 17.0 {
            TrafficLightState::Amber
        } else {
            TrafficLightState::Red
        }
    }
}

pub struct TrafficCar {
    pub car: Car,
    parked: bool,
    from: usize,
    to: usize,
    serial: usize,
}
pub struct Traffic {
    pub cars: Vec<TrafficCar>,
    refresh_time: f32,
    next_serial: usize,
    pub lights: Vec<TrafficLight>,
    pub siren_active: bool,
    pub siren_pos: [f32; 3],
    pub global_time: f32,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn lane_segment(paths: &PathNetwork, from: usize, to: usize) -> ([f32; 3], [f32; 3]) {
    let mut a = paths.nodes[from].position;
    let mut b = paths.nodes[to].position;
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = dx.hypot(dy).max(0.001);
    for p in [&mut a, &mut b] {
        p[0] += dy / length * LANE_OFFSET;
        p[1] -= dx / length * LANE_OFFSET;
    }
    (a, b)
}
fn clear_body(world: &impl CollisionQuery, car: &Car) -> bool {
    let (s, c) = car.heading.sin_cos();
    [-0.94f32, 0.0, 0.94].into_iter().all(|x| {
        [-2.72f32, 0.0, 2.72].into_iter().all(|y| {
            let direction = [c * x + s * y, -s * x + c * y, 0.0];
            [0.35, 0.85].into_iter().all(|z| {
                world
                    .raycast(
                        [car.position[0], car.position[1], car.position[2] + z],
                        direction,
                        x.hypot(y) + 0.08,
                    )
                    .is_none()
            })
        })
    })
}

impl Traffic {
    pub fn new() -> Self {
        Self {
            cars: Vec::new(),
            refresh_time: 0.0,
            next_serial: 0,
            lights: Self::default_lights(),
            siren_active: false,
            siren_pos: [0.0; 3],
            global_time: 0.0,
        }
    }

    pub fn default_lights() -> Vec<TrafficLight> {
        vec![
            TrafficLight {
                position: [504.0, -496.0, 10.5],
                heading: 0.0,
                cycle_offset: 0.0,
            },
            TrafficLight {
                position: [350.0, -600.0, 10.0],
                heading: 1.57,
                cycle_offset: 16.0,
            },
            TrafficLight {
                position: [-1005.0, -495.0, 11.0],
                heading: 0.0,
                cycle_offset: 8.0,
            },
        ]
    }
    pub fn reset(&mut self) {
        self.cars.clear();
        self.refresh_time = 0.0;
    }
    /// E uses the same access checks for the player's car and ambient cars.
    /// An exchanged car stays parked, rendered and collidable at its old pose.
    pub fn enter_nearest(
        &mut self,
        world: &CollisionWorld,
        shape: &NativeCollider,
        pedestrians: &[CollisionInstance<'_>],
        feet: [f32; 3],
        player_car: &mut Car,
    ) -> bool {
        let candidates: Vec<_> = std::iter::once(*player_car)
            .chain(self.cars.iter().map(|v| v.car))
            .collect();
        let mut order: Vec<_> = (0..candidates.len()).collect();
        order.sort_by(|&a, &b| {
            distance(feet, candidates[a].position)
                .total_cmp(&distance(feet, candidates[b].position))
        });
        for index in order {
            let candidate = candidates[index];
            let horizontal = distance(feet, candidate.position);
            if horizontal >= 6.75
                || (feet[2] - candidate.position[2]).abs() >= 1.5
                || candidate.speed.abs() >= 1.0
                || !candidate.grounded
            {
                continue;
            }
            let mut obstacles = pedestrians.to_vec();
            obstacles.extend(
                candidates
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, car)| shape.at(car.position, car.heading)),
            );
            let scene = CollisionScene {
                world,
                instances: &obstacles,
            };
            let direction = std::array::from_fn(|a| candidate.position[a] - feet[a]);
            let length = direction.iter().map(|v| v * v).sum::<f32>().sqrt();
            if scene
                .raycast([feet[0], feet[1], feet[2] + 0.8], direction, length)
                .is_some()
            {
                continue;
            }
            if index > 0 {
                let selected = &mut self.cars[index - 1];
                std::mem::swap(player_car, &mut selected.car);
                selected.car.speed = 0.0;
                selected.parked = true;
            }
            player_car.speed = 0.0;
            return true;
        }
        false
    }
    pub fn colliders<'a>(&self, shape: &'a NativeCollider) -> Vec<CollisionInstance<'a>> {
        self.cars
            .iter()
            .map(|v| shape.at(v.car.position, v.car.heading))
            .collect()
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!(self
            .cars
            .iter()
            .map(|v| serde_json::json!({
                "model_id":175,"serial":v.serial,"position":v.car.position,"heading":v.car.heading,
                "speed":v.car.speed,"grounded":v.car.grounded,"parked":v.parked,"path_from":v.from,"path_to":v.to,
            }))
            .collect::<Vec<_>>())
    }
    pub fn refresh(
        &mut self,
        world: &CollisionWorld,
        paths: &PathNetwork,
        focus: [f32; 3],
        obstacles: &[CollisionInstance<'_>],
    ) {
        self.cars.retain(|v| {
            distance(v.car.position, focus) < 190.0 && (v.car.position[2] - focus[2]).abs() < 12.0
        });
        // Keep abandoned cars nearby; limit older parked cars only once the
        // player is far enough away that they cannot disappear beside them.
        let mut parked_count = self.cars.iter().filter(|v| v.parked).count();
        self.cars.retain(|v| {
            if v.parked && parked_count > 12 && distance(v.car.position, focus) > 40.0 {
                parked_count -= 1;
                false
            } else {
                true
            }
        });
        let scene = CollisionScene {
            world,
            instances: obstacles,
        };
        for from in paths.nearest_roads(focus, 120.0) {
            if self.cars.iter().filter(|v| !v.parked).count() >= 4 {
                break;
            }
            if distance(paths.nodes[from].position, focus) < 28.0 {
                continue;
            }
            let links = &paths.nodes[from].links;
            let Some(to) = links
                .iter()
                .filter(|l| l.target < paths.car_nodes && l.two_way())
                .min_by_key(|l| (l.target + self.next_serial * 17) % 997)
                .map(|l| l.target)
            else {
                continue;
            };
            let (a, b) = lane_segment(paths, from, to);
            if distance(a, b) < 4.0 || (a[2] - b[2]).abs() > 4.0 {
                continue;
            }
            let mut car = Car::new([a[0], a[1], a[2] + 1.5], (b[0] - a[0]).atan2(b[1] - a[1]));
            car.settle_on_ground(world, true, 0.0);
            if !car.grounded
                || (car.position[2] - a[2]).abs() > 1.8
                || !clear_body(&scene, &car)
                || self
                    .cars
                    .iter()
                    .any(|v| distance(v.car.position, car.position) < 24.0)
            {
                continue;
            }
            self.cars.push(TrafficCar {
                car,
                parked: false,
                from,
                to,
                serial: self.next_serial,
            });
            self.next_serial += 1;
        }
    }
    pub fn step(
        &mut self,
        world: &CollisionWorld,
        paths: &PathNetwork,
        shape: &NativeCollider,
        focus: [f32; 3],
        external: &[CollisionInstance<'_>],
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.1);
        self.global_time += dt;
        self.refresh_time -= dt;
        if self.refresh_time <= 0.0 {
            self.refresh(world, paths, focus, external);
            self.refresh_time = 2.0;
        }
        let poses: Vec<_> = self
            .cars
            .iter()
            .map(|v| (v.car.position, v.car.heading))
            .collect();
        for (index, vehicle) in self.cars.iter_mut().enumerate() {
            if vehicle.parked {
                continue;
            }
            let (a, b) = lane_segment(paths, vehicle.from, vehicle.to);
            let d = [b[0] - a[0], b[1] - a[1]];
            let passed = (vehicle.car.position[0] - b[0]) * d[0]
                + (vehicle.car.position[1] - b[1]) * d[1]
                >= 0.0;
            if distance(vehicle.car.position, b) < 3.5 || passed {
                let current = vehicle.to;
                let next = paths.nodes[current]
                    .links
                    .iter()
                    .filter(|l| {
                        l.target < paths.car_nodes && l.target != vehicle.from && l.two_way()
                    })
                    .filter(|l| {
                        let p = paths.nodes[l.target].position;
                        (p[0] - b[0]) * d[0] + (p[1] - b[1]) * d[1]
                            > -0.25 * distance(p, b) * d[0].hypot(d[1])
                    })
                    .min_by_key(|l| (l.target + vehicle.serial * 31) % 997)
                    .map(|l| l.target);
                if let Some(next) = next {
                    vehicle.from = current;
                    vehicle.to = next;
                } else {
                    vehicle.car.speed = 0.0;
                    continue;
                }
            }
            let (a, b) = lane_segment(paths, vehicle.from, vehicle.to);
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0].hypot(d[1]).max(0.001);
            let progress = ((vehicle.car.position[0] - a[0]) * d[0]
                + (vehicle.car.position[1] - a[1]) * d[1])
                / length;
            let lookahead = (4.0 + vehicle.car.speed * 0.5).max(4.0);
            let t = ((progress + lookahead) / length).clamp(0.0, 1.0);
            let dx = a[0] + d[0] * t - vehicle.car.position[0];
            let dy = a[1] + d[1] * t - vehicle.car.position[1];
            let error = (dx.atan2(dy) - vehicle.car.heading + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let wanted_angle = (2.0 * 2.75 * error.sin() / dx.hypot(dy).max(2.0)).atan();
            let lock = 34f32.to_radians() * (1.0 - 0.72 * vehicle.car.speed.abs() / 58.0);
            let steer = wanted_angle.signum() * (wanted_angle.abs() / lock).sqrt().min(1.0);
            let mut instances: Vec<_> = external.to_vec();
            instances.extend(
                poses
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != index)
                    .map(|(_, (p, h))| shape.at(*p, *h)),
            );
            let scene = CollisionScene {
                world,
                instances: &instances,
            };
            let (s, c) = vehicle.car.heading.sin_cos();
            let stopping = 3.2 + vehicle.car.speed.abs() * 0.65 + vehicle.car.speed.powi(2) / 16.0;
            let blocked = [-0.8f32, 0.0, 0.8].into_iter().any(|x| {
                scene
                    .actor_raycast(
                        [
                            vehicle.car.position[0] + c * x,
                            vehicle.car.position[1] - s * x,
                            vehicle.car.position[2] + 0.75,
                        ],
                        [s, c, 0.0],
                        stopping,
                    )
                    .is_some()
            });
            let light_stop = self.lights.iter().any(|light| {
                let state = light.state_at(self.global_time);
                if state == TrafficLightState::Green {
                    return false;
                }
                let dx = light.position[0] - vehicle.car.position[0];
                let dy = light.position[1] - vehicle.car.position[1];
                let dist = dx.hypot(dy);
                if dist < 16.0 && (light.position[2] - vehicle.car.position[2]).abs() < 3.0 {
                    let forward = dx * s + dy * c;
                    let lateral = dx * c - dy * s;
                    forward > 1.0 && forward < 15.0 && lateral.abs() < 4.5
                } else {
                    false
                }
            });
            let blocked = blocked || light_stop;

            let mut target_speed = CRUISE_SPEED / (1.0 + error.abs() * 2.0);
            let mut yield_steer = 0.0;
            if self.siren_active {
                let s_dx = self.siren_pos[0] - vehicle.car.position[0];
                let s_dy = self.siren_pos[1] - vehicle.car.position[1];
                if s_dx.hypot(s_dy) < 40.0 {
                    yield_steer = 0.35;
                    target_speed = target_speed.min(2.5);
                }
            }
            let steer = (steer + yield_steer).clamp(-1.0, 1.0);
            let throttle = if !blocked && vehicle.car.speed < target_speed {
                0.28
            } else {
                0.0
            };
            let brake = if blocked {
                1.0
            } else if vehicle.car.speed > target_speed + 0.3 {
                0.2
            } else {
                0.0
            };
            vehicle.car.step(&scene, throttle, brake, steer, dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{test_box_collider, Triangle};
    use crate::paths::{PathLink, PathNode};
    fn fixture() -> (CollisionWorld, PathNetwork, NativeCollider) {
        let world = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [
                    [-200.0, -200.0, 0.0],
                    [200.0, -200.0, 0.0],
                    [200.0, 200.0, 0.0],
                ],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [
                    [-200.0, -200.0, 0.0],
                    [200.0, 200.0, 0.0],
                    [-200.0, 200.0, 0.0],
                ],
                surface: 0,
                model_id: 1,
            },
        ]);
        let positions = [
            [0.0, -50.0, 0.0],
            [0.0, 50.0, 0.0],
            [50.0, 50.0, 0.0],
            [50.0, -50.0, 0.0],
        ];
        let paths = PathNetwork {
            car_nodes: 4,
            nodes: positions
                .into_iter()
                .enumerate()
                .map(|(i, position)| PathNode {
                    position,
                    flags: 0,
                    links: vec![PathLink {
                        target: (i + 1) % 4,
                        crosses_road: false,
                        lanes: Some([1, 1]),
                    }],
                })
                .collect(),
        };
        (
            world,
            paths,
            test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]),
        )
    }
    #[test]
    fn entering_traffic_preserves_both_cars_and_parks_the_previous_one() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        let target = traffic.cars[0].car;
        let mut player = Car::new([-20.0, -40.0, 0.0], 0.4);
        player.settle_on_ground(&world, true, 0.0);
        let old = player.position;
        let feet = [
            target.position[0] + 3.0,
            target.position[1],
            target.position[2],
        ];
        assert!(traffic.enter_nearest(&world, &shape, &[], feet, &mut player));
        assert_eq!(player.position, target.position);
        assert_eq!(player.heading, target.heading);
        assert_eq!(traffic.cars[0].car.position, old);
        assert!(traffic.cars[0].parked);
        assert_eq!(traffic.colliders(&shape).len(), 4);
        for _ in 0..120 {
            let obstacles = [shape.at(player.position, player.heading)];
            traffic.step(
                &world,
                &paths,
                &shape,
                player.position,
                &obstacles,
                1.0 / 60.0,
            );
        }
        assert_eq!(traffic.cars[0].car.position, old);
        let feet = [old[0] + 3.0, old[1], old[2]];
        assert!(traffic.enter_nearest(&world, &shape, &[], feet, &mut player));
        assert_eq!(player.position, old, "the abandoned car remains usable");
        assert_eq!(traffic.cars[0].car.position, target.position);
    }

    #[test]
    fn entry_rejects_moving_cars_other_floors_and_obstacles() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        traffic.cars.truncate(1);
        let p = traffic.cars[0].car.position;
        let mut player = Car::new([-100.0, -100.0, 0.0], 0.0);
        let feet = [p[0] + 3.0, p[1], p[2]];
        assert!(!traffic.enter_nearest(
            &world,
            &shape,
            &[],
            [feet[0], feet[1], feet[2] + 3.0],
            &mut player
        ));
        traffic.cars[0].car.speed = 4.0;
        assert!(!traffic.enter_nearest(&world, &shape, &[], feet, &mut player));
        traffic.cars[0].car.speed = 0.0;
        let blocker = shape.at([p[0] + 1.6, p[1], p[2]], 0.0);
        assert!(!traffic.enter_nearest(&world, &shape, &[blocker], feet, &mut player));
        let wall = CollisionWorld::from_triangles(vec![Triangle {
            vertices: [
                [p[0] + 1.5, p[1] - 5.0, 0.0],
                [p[0] + 1.5, p[1] + 5.0, 0.0],
                [p[0] + 1.5, p[1], 5.0],
            ],
            surface: 0,
            model_id: 9,
        }]);
        assert!(!traffic.enter_nearest(&wall, &shape, &[], feet, &mut player));
        assert!(traffic.enter_nearest(&world, &shape, &[], feet, &mut player));
    }

    #[test]
    fn traffic_spawns_on_roads_moves_and_stays_grounded() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        assert_eq!(traffic.cars.len(), 4);
        let start = traffic.cars[0].car.position;
        for _ in 0..240 {
            traffic.step(&world, &paths, &shape, [0.0; 3], &[], 1.0 / 60.0);
        }
        assert!(distance(start, traffic.cars[0].car.position) > 15.0);
        assert!(traffic.cars.iter().all(|v| v.car.grounded));
    }
    #[test]
    fn traffic_brakes_for_vehicle_and_other_floor_does_not_spawn() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0, 0.0, 10.0], &[]);
        assert!(traffic.cars.is_empty());
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        traffic.cars.truncate(1);
        let start = traffic.cars[0].car.position;
        let obstacles = [shape.at([start[0], start[1] + 12.0, start[2]], 0.0)];
        for _ in 0..240 {
            traffic.step(&world, &paths, &shape, [0.0; 3], &obstacles, 1.0 / 60.0);
        }
        assert!(traffic.cars[0].car.position[1] < start[1] + 6.5);
        assert!(traffic.cars[0].car.speed < 0.2);
    }

    #[test]
    fn traffic_light_cycles_green_amber_red() {
        let light = TrafficLight {
            position: [0.0, 0.0, 0.0],
            heading: 0.0,
            cycle_offset: 0.0,
        };
        assert_eq!(light.state_at(5.0), TrafficLightState::Green);
        assert_eq!(light.state_at(15.0), TrafficLightState::Amber);
        assert_eq!(light.state_at(22.0), TrafficLightState::Red);
        assert_eq!(light.state_at(33.0), TrafficLightState::Green);
    }

    #[test]
    fn traffic_car_stops_at_red_light() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        traffic.cars.truncate(1);
        let start = traffic.cars[0].car.position;
        traffic.lights = vec![TrafficLight {
            position: [start[0], start[1] + 10.0, start[2]],
            heading: 0.0,
            cycle_offset: 20.0, // Red light
        }];
        for _ in 0..120 {
            traffic.step(&world, &paths, &shape, [0.0; 3], &[], 1.0 / 60.0);
        }
        assert!(traffic.cars[0].car.position[1] < start[1] + 10.5);
    }

    #[test]
    fn traffic_car_yields_to_emergency_siren() {
        let (world, paths, shape) = fixture();
        let mut traffic = Traffic::new();
        traffic.refresh(&world, &paths, [0.0; 3], &[]);
        traffic.cars.truncate(1);
        traffic.siren_active = true;
        traffic.siren_pos = traffic.cars[0].car.position;
        traffic.step(&world, &paths, &shape, [0.0; 3], &[], 1.0 / 60.0);
        // Under active siren, traffic car steering is biased towards the curb
        assert!(traffic.cars[0].car.speed <= 2.5);
    }
}
