#![allow(dead_code)]
//! Small deterministic ambient population on the decoded pedestrian graph.
//! Route following/avoidance are prototype behaviours, not the original AI.
use crate::collision::{CollisionQuery, CollisionScene, CollisionWorld, NativeCollider};
use crate::driving::Car;
use crate::paths::PathNetwork;
use crate::player::Walker;
use crate::render::Camera;

pub struct Pedestrian {
    pub fighter: crate::combat::Fighter,
    aggression: f32,
    pub walker: Walker,
    pub heading: f32,
    pub model: usize,
    pub animation_time: f32,
    pub clip: &'static str,
    previous: usize,
    target: usize,
    blocked_time: f32,
    pub panic_timer: f32,
    pub panic_origin: Option<[f32; 3]>,
}

pub struct Population {
    pub pedestrians: Vec<Pedestrian>,
    refresh_time: f32,
}

impl Population {
    pub fn colliders<'a>(
        &self,
        models: &'a [crate::character::Character],
    ) -> Vec<crate::collision::CollisionInstance<'a>> {
        self.pedestrians
            .iter()
            .filter(|p| p.fighter.alive())
            .map(|p| models[p.model].collision.at(p.walker.feet, p.heading))
            .collect()
    }
    pub fn snapshot(&self, models: &[crate::character::Character]) -> serde_json::Value {
        serde_json::json!(self
            .pedestrians
            .iter()
            .map(|p| serde_json::json!({
                "model_id": models[p.model].stats.model_id, "position": p.walker.feet,
                "heading": p.heading, "grounded": p.walker.grounded, "animation": p.clip,
                "animation_time": p.animation_time, "path_target": p.target,
                "combat": p.fighter.snapshot(),
            }))
            .collect::<Vec<_>>())
    }
    pub fn new() -> Self {
        Self {
            pedestrians: Vec::new(),
            refresh_time: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.pedestrians.clear();
        self.refresh_time = 0.0;
    }

    #[allow(dead_code)]
    pub fn trigger_panic(&mut self, origin: [f32; 3], radius: f32) {
        for ped in &mut self.pedestrians {
            if horizontal_distance(ped.walker.feet, origin) < radius {
                ped.panic_timer = 5.0;
                ped.panic_origin = Some(origin);
            }
        }
    }

    pub fn refresh(
        &mut self,
        world: &impl CollisionQuery,
        paths: &PathNetwork,
        focus: [f32; 3],
        model_count: usize,
    ) {
        if model_count == 0 {
            return;
        }
        self.pedestrians.retain(|p| {
            horizontal_distance(p.walker.feet, focus) < 150.0
                && (p.walker.feet[2] - focus[2]).abs() < 12.0
        });
        for node_id in paths.nearest_pedestrians(focus, 80.0) {
            if self.pedestrians.len() >= 6 {
                break;
            }
            let node = &paths.nodes[node_id];
            if horizontal_distance(node.position, focus) < 7.0
                || self
                    .pedestrians
                    .iter()
                    .any(|p| horizontal_distance(p.walker.feet, node.position) < 9.0)
            {
                continue;
            }
            let Some(target) = node
                .links
                .iter()
                .find(|link| link.target >= paths.car_nodes)
                .map(|l| l.target)
            else {
                continue;
            };
            let camera = Camera {
                pos: [node.position[0], node.position[1], node.position[2] + 2.5],
                yaw: 0.0,
                pitch: 0.0,
            };
            let Some(walker) = Walker::from_camera(world, &camera) else {
                continue;
            };
            // Coarse graph Z must agree with a real walkable surface. This also
            // prevents spawning beneath bridges or on an unrelated interior.
            if (walker.feet[2] - node.position[2]).abs() > 2.0 || !walker.has_clear_body(world) {
                continue;
            }
            let dest = paths.nodes[target].position;
            let heading = (dest[0] - walker.feet[0]).atan2(dest[1] - walker.feet[1]);
            self.pedestrians.push(Pedestrian {
                fighter: crate::combat::Fighter::default(),
                aggression: 0.,
                walker,
                heading,
                model: node_id % model_count,
                animation_time: (node_id % 11) as f32 * 0.13,
                clip: "idle",
                previous: node_id,
                target,
                blocked_time: 0.0,
                panic_timer: 0.0,
                panic_origin: None,
            });
        }
    }

    pub fn step(
        &mut self,
        world: &CollisionWorld,
        paths: &PathNetwork,
        cars: &[Car],
        collider: &NativeCollider,
        focus: [f32; 3],
        player: Option<[f32; 3]>,
        model_count: usize,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.1);
        let instances: Vec<_> = cars
            .iter()
            .map(|car| collider.at(car.position, car.heading))
            .collect();
        let scene = CollisionScene {
            world,
            instances: &instances,
        };
        self.refresh_time -= dt;
        if self.refresh_time <= 0.0 {
            self.refresh(&scene, paths, focus, model_count);
            self.refresh_time = 2.0;
        }
        let positions: Vec<_> = self.pedestrians.iter().map(|p| p.walker.feet).collect();
        for (index, ped) in self.pedestrians.iter_mut().enumerate() {
            if !ped.fighter.alive() || ped.aggression > 0. {
                continue;
            }
            let position = ped.walker.feet;
            let node = &paths.nodes[ped.target];
            if ped.blocked_time > 2.0 {
                std::mem::swap(&mut ped.previous, &mut ped.target);
                ped.blocked_time = 0.0;
            } else if horizontal_distance(position, node.position) < 0.7 {
                let next = node
                    .links
                    .iter()
                    .filter(|l| l.target >= paths.car_nodes && l.target != ped.previous)
                    .min_by_key(|l| l.target.wrapping_add(index * 7919) % 997)
                    .or_else(|| node.links.iter().find(|l| l.target >= paths.car_nodes));
                if let Some(link) = next {
                    ped.previous = ped.target;
                    ped.target = link.target;
                }
                ped.blocked_time = 0.0;
            }
            let target = paths.nodes[ped.target].position;
            let mut direction = [target[0] - position[0], target[1] - position[1]];
            let mut speed = 1.35;
            let danger = cars
                .iter()
                .filter(|car| {
                    car.speed.abs() > 2.0
                        && (car.position[2] - position[2]).abs() < 2.0
                        && horizontal_distance(position, car.position) < 6.0
                })
                .min_by(|a, b| {
                    horizontal_distance(position, a.position)
                        .total_cmp(&horizontal_distance(position, b.position))
                });
            if ped.panic_timer > 0.0 {
                ped.panic_timer -= dt;
                if let Some(origin) = ped.panic_origin {
                    let flee_x = position[0] - origin[0];
                    let flee_y = position[1] - origin[1];
                    let flee_len = flee_x.hypot(flee_y).max(0.001);
                    direction = [flee_x / flee_len, flee_y / flee_len];
                    speed = 4.8;
                }
            } else if let Some(car) = danger {
                direction = [position[0] - car.position[0], position[1] - car.position[1]];
                speed = 4.0;
            } else if player.is_some_and(|p| {
                horizontal_distance(p, position) < 1.1 && (p[2] - position[2]).abs() < 1.0
            }) {
                speed = 0.0;
            }
            let length = direction[0].hypot(direction[1]).max(0.001);
            direction = [direction[0] / length, direction[1] / length];
            let ahead = [
                position[0] + direction[0] * 0.65,
                position[1] + direction[1] * 0.65,
                position[2],
            ];
            if positions.iter().enumerate().any(|(j, p)| {
                j != index
                    && horizontal_distance(*p, ahead) < 0.65
                    && (p[2] - position[2]).abs() < 1.0
            }) {
                speed = 0.0;
            }
            ped.walker.step(&scene, direction, speed, false, dt);
            let moved = horizontal_distance(position, ped.walker.feet);
            if moved > 0.002 {
                ped.heading = direction[0].atan2(direction[1]);
                ped.blocked_time = 0.0;
            } else {
                ped.blocked_time += dt;
            }
            ped.clip = if moved < 0.002 {
                "idle"
            } else if speed > 2.0 {
                "run"
            } else {
                "walk"
            };
            ped.animation_time += dt;
        }
    }

    /// Deterministic first response to an assault. Normal pedestrians remain
    /// peaceful until struck. All combat uses the same solid scene as walking.
    pub fn combat_step(
        &mut self,
        world: &impl CollisionQuery,
        profile: &crate::combat::Profile,
        player: Option<[f32; 3]>,
        heading: f32,
        fighter: &mut crate::combat::Fighter,
        attack: bool,
        guard: bool,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let dt = dt.min(0.1);
        if player.is_some() && fighter.alive() {
            fighter.stance(guard || attack);
            if attack {
                fighter.attack();
            }
        } else {
            fighter.cancel();
        }
        let strike = fighter.tick(profile, dt);
        if let Some(position) = player.filter(|_| strike) {
            let ids: Vec<_> = self
                .pedestrians
                .iter()
                .enumerate()
                .filter(|(_, p)| p.fighter.alive())
                .map(|(i, _)| i)
                .collect();
            let positions: Vec<_> = ids
                .iter()
                .map(|&i| self.pedestrians[i].walker.feet)
                .collect();
            if let Some(t) =
                crate::combat::target(world, position, heading, &positions, crate::combat::REACH)
            {
                let ped = &mut self.pedestrians[ids[t]];
                ped.fighter
                    .damage(crate::combat::DAMAGE, fighter.hits as usize);
                ped.aggression = 12.;
                fighter.hits += 1;
            }
        }
        let positions: Vec<_> = self.pedestrians.iter().map(|p| p.walker.feet).collect();
        for (index, ped) in self.pedestrians.iter_mut().enumerate() {
            ped.aggression = (ped.aggression - dt).max(0.);
            let target = player.filter(|p| {
                fighter.alive()
                    && ped.fighter.alive()
                    && ped.aggression > 0.
                    && horizontal_distance(*p, ped.walker.feet) < 20.
                    && (p[2] - ped.walker.feet[2]).abs() < 1.0
            });
            if let Some(p) = target {
                let delta = [p[0] - ped.walker.feet[0], p[1] - ped.walker.feet[1]];
                let distance = delta[0].hypot(delta[1]);
                if !ped.fighter.busy() {
                    ped.heading = delta[0].atan2(delta[1]);
                    if distance > 1.25 {
                        ped.fighter.stance(false);
                        let crowded = positions.iter().enumerate().any(|(i, q)| {
                            i != index && horizontal_distance(*q, ped.walker.feet) < 0.7
                        });
                        ped.walker
                            .step(world, delta, if crowded { 0. } else { 1.6 }, false, dt);
                        ped.clip = "walk";
                        ped.animation_time += dt;
                    } else {
                        ped.fighter.stance(true);
                        ped.fighter.attack();
                    }
                }
            } else if ped.fighter.alive() && !ped.fighter.busy() {
                ped.fighter.stance(false);
            }
            if ped.fighter.tick(profile, dt) {
                if let Some(p) = target {
                    if crate::combat::target(
                        world,
                        ped.walker.feet,
                        ped.heading,
                        &[p],
                        crate::combat::REACH,
                    )
                    .is_some()
                    {
                        fighter.damage(crate::combat::DAMAGE, ped.fighter.hits as usize);
                        ped.fighter.hits += 1;
                    }
                }
            }
        }
    }
}

fn horizontal_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{test_box_collider, Triangle};
    use crate::paths::{PathLink, PathNode};

    fn fixture() -> (CollisionWorld, PathNetwork, NativeCollider, Car) {
        let world = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [
                    [-100.0, -100.0, 0.0],
                    [100.0, -100.0, 0.0],
                    [100.0, 100.0, 0.0],
                ],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [
                    [-100.0, -100.0, 0.0],
                    [100.0, 100.0, 0.0],
                    [-100.0, 100.0, 0.0],
                ],
                surface: 0,
                model_id: 1,
            },
        ]);
        let paths = PathNetwork {
            car_nodes: 0,
            nodes: vec![
                PathNode {
                    position: [10.0, 0.0, 0.0],
                    links: vec![PathLink {
                        target: 1,
                        crosses_road: false,
                        lanes: Some([1, 1]),
                    }],
                    flags: 0,
                },
                PathNode {
                    position: [10.0, 20.0, 0.0],
                    links: vec![PathLink {
                        target: 0,
                        crosses_road: false,
                        lanes: Some([1, 1]),
                    }],
                    flags: 0,
                },
            ],
        };
        let shape = test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]);
        (world, paths, shape, Car::new([40.0, 40.0, 0.0], 0.0))
    }

    #[test]
    fn population_spawns_on_ground_and_walks_the_graph() {
        let (world, paths, shape, car) = fixture();
        let mut population = Population::new();
        population.refresh(&world, &paths, [0.0; 3], 3);
        assert_eq!(population.pedestrians.len(), 2);
        for _ in 0..120 {
            population.step(
                &world,
                &paths,
                &[car],
                &shape,
                [0.0; 3],
                None,
                3,
                1.0 / 60.0,
            );
        }
        let ped = &population.pedestrians[0];
        assert!(
            (ped.walker.feet[1] - 2.7).abs() < 0.02,
            "{:?}",
            ped.walker.feet
        );
        assert!(ped.walker.grounded);
        assert_eq!(ped.clip, "walk");
    }

    #[test]
    fn assault_triggers_reaction_retaliation_and_player_knockout() {
        let (world, paths, _, _) = fixture();
        let mut population = Population::new();
        population.refresh(&world, &paths, [0.; 3], 3);
        let mut fighter = crate::combat::Fighter::default();
        let profile = crate::combat::Profile::fixture();
        let player = Some([10., -1.1, 0.02]);
        for _ in 0..120 {
            population.combat_step(
                &world,
                &profile,
                player,
                0.,
                &mut fighter,
                false,
                false,
                1. / 120.,
            );
        }
        assert_eq!(fighter.health, 100);
        assert_eq!(population.pedestrians[0].fighter.health, 100);
        for i in 0..1500 {
            population.combat_step(
                &world,
                &profile,
                player,
                0.,
                &mut fighter,
                i < 36,
                false,
                1. / 120.,
            );
        }
        assert!(fighter.hits > 0 && fighter.hits_taken > 0);
        assert_eq!(fighter.health, 0);
        assert_eq!(fighter.pose().unwrap().clip, "KO_skid_back");
        assert!(population.pedestrians[0].fighter.health < 100);
    }

    #[test]
    fn pedestrians_stop_for_player_and_flee_moving_car() {
        let (world, paths, shape, mut car) = fixture();
        let mut population = Population::new();
        population.refresh(&world, &paths, [0.0; 3], 3);
        let start = population.pedestrians[0].walker.feet;
        population.step(
            &world,
            &paths,
            &[car],
            &shape,
            [0.0; 3],
            Some([10.0, 0.8, 0.02]),
            3,
            0.1,
        );
        assert_eq!(population.pedestrians[0].walker.feet, start);
        assert_eq!(population.pedestrians[0].clip, "idle");
        car.position = [10.0, -5.0, 0.02];
        car.speed = 5.0;
        population.step(&world, &paths, &[car], &shape, [0.0; 3], None, 3, 0.1);
        assert_eq!(population.pedestrians[0].clip, "run");
        assert!(population.pedestrians[0].walker.feet[1] > 0.3);
    }

    #[test]
    fn population_rejects_other_floor_and_clears_on_reset() {
        let (world, mut paths, _, _) = fixture();
        let mut population = Population::new();
        paths.nodes[0].position[2] = 10.0;
        paths.nodes[1].position[2] = 10.0;
        population.refresh(&world, &paths, [0.0, 0.0, 10.0], 3);
        assert!(population.pedestrians.is_empty());
        paths.nodes[0].position[2] = 0.0;
        population.refresh(&world, &paths, [0.0; 3], 3);
        assert_eq!(population.pedestrians.len(), 1);
        population.reset();
        assert!(population.pedestrians.is_empty());
    }

    #[test]
    fn population_panic_reaction_flees_threat_origin() {
        let (world, paths, shape, car) = fixture();
        let mut population = Population::new();
        population.refresh(&world, &paths, [0.0; 3], 3);
        assert!(!population.pedestrians.is_empty());
        let initial_y = population.pedestrians[0].walker.feet[1];
        population.trigger_panic([10.0, -5.0, 0.0], 25.0);
        assert!(population.pedestrians[0].panic_timer > 0.0);
        population.step(&world, &paths, &[car], &shape, [0.0; 3], None, 3, 0.1);
        assert!(population.pedestrians[0].walker.feet[1] > initial_y);
    }
}
