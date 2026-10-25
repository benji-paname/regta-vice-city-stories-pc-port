//! Native low-car clips and root motion, with swept walking to a clear door.
//! Door angle and approach steering are PC controller approximations.
use crate::{
    character::{Character, Pose},
    collision::{CollisionQuery, CollisionScene},
    driving::Car,
    iso::Result,
    player::Walker,
    render::Camera,
    vehicle::VehicleModel,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Approach,
    Align,
    Open,
    GetIn,
    ShutInside,
    GetOut,
    ShutOutside,
    Complete,
}
pub struct Access {
    phase: Phase,
    clock: f32,
    elapsed: f32,
    pub side: f32,
    pub anchor: [f32; 3],
    pub entering: bool,
    path: Vec<[f32; 3]>,
    next: usize,
    exit: Walker,
}
#[derive(PartialEq, Debug)]
pub enum Outcome {
    Running,
    Entered,
    Exited,
    Cancelled,
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn point(car: &Car, x: f32, y: f32, z: f32) -> [f32; 3] {
    let (s, c) = car.heading.sin_cos();
    [
        car.position[0] + c * x + s * y,
        car.position[1] - s * x + c * y,
        z,
    ]
}
impl Access {
    pub fn seated_position(actor: &Character, model: &VehicleModel, car: &Car) -> [f32; 3] {
        let side = -1.;
        let door = model.doors.iter().find(|d| d.side == side).unwrap();
        let y = model.triangles[door.triangle_range.clone()]
            .iter()
            .flat_map(|t| &t.vertices)
            .map(|v| v.p[1])
            .fold(f32::INFINITY, f32::min)
            + 0.05;
        let root = actor.root_translation("CAR_closedoorL_RHS", 99.).unwrap();
        point(
            car,
            model.bounds[0][0] - 0.45 - root[0],
            y + root[1],
            car.position[2] + root[2] + 0.005,
        )
    }
    pub fn validate(actor: &Character) -> Result<()> {
        for clip in [
            "CAR_align_RHS",
            "CAR_open_RHS",
            "CAR_getinL_RHS",
            "CAR_closedoorL_RHS",
            "CAR_getoutL_RHS",
            "CAR_close_RHS",
        ] {
            if actor.duration(clip).is_none() || actor.root_translation(clip, 0.).is_none() {
                return Err(format!("Missing vehicle animation {clip}").into());
            }
        }
        Ok(())
    }
    pub fn begin(
        world: &impl CollisionQuery,
        car: &Car,
        model: &VehicleModel,
        walker: Option<&Walker>,
    ) -> Option<Self> {
        if car.speed.abs() >= 1. || !car.grounded {
            return None;
        }
        let instances = [model.collision.at(car.position, car.heading)];
        let solid = CollisionScene {
            world,
            instances: &instances,
        };
        // Driver side first; allow the other door only when the first is blocked.
        for side in [-1.0f32, 1.] {
            let edge = if side < 0. {
                -model.bounds[0][0]
            } else {
                model.bounds[1][0]
            };
            let x = side * (edge + 0.45);
            // Native low-car root motion starts beside the rear edge of the
            // front door, then moves forward into the seat. Align against that
            // mesh edge instead of the vehicle centre (which is by the dash).
            let door = model.doors.iter().find(|d| d.side == side)?;
            let door_y = model.triangles[door.triangle_range.clone()]
                .iter()
                .flat_map(|t| &t.vertices)
                .map(|v| v.p[1])
                .fold(f32::INFINITY, f32::min)
                + 0.05;
            let feet = point(car, x, door_y, car.position[2] + 2.2);
            let Some(exit) = Walker::from_camera(
                &solid,
                &Camera {
                    pos: feet,
                    yaw: car.heading,
                    pitch: 0.,
                },
            ) else {
                continue;
            };
            if (exit.feet[2] - car.position[2]).abs() > 0.65 || !exit.has_clear_body(&solid) {
                continue;
            }
            let (s, c) = car.heading.sin_cos();
            if [0.4, 0.9, 1.3].iter().any(|z| {
                world
                    .actor_raycast(
                        point(car, 0., door_y, exit.feet[2] + z),
                        [side * c, -side * s, 0.],
                        x.abs(),
                    )
                    .is_some()
            }) {
                continue;
            }
            let path = if let Some(walker) = walker {
                let mut paths = vec![vec![exit.feet]];
                // Try both ends of the car; validate every segment with the real
                // walker rather than teleporting across the body or nearby walls.
                let other_x = -side
                    * (if side < 0. {
                        model.bounds[1][0]
                    } else {
                        -model.bounds[0][0]
                    } + 0.6);
                for y in [model.bounds[0][1] - 0.65, model.bounds[1][1] + 0.65] {
                    paths.push(vec![
                        point(car, other_x, y, exit.feet[2]),
                        point(car, x, y, exit.feet[2]),
                        exit.feet,
                    ]);
                    paths.push(vec![point(car, x, y, exit.feet[2]), exit.feet]);
                }
                paths.sort_by(|a, b| {
                    let length = |p: &Vec<[f32; 3]>| {
                        let mut prev = walker.feet;
                        p.iter()
                            .map(|&v| {
                                let d = distance(prev, v);
                                prev = v;
                                d
                            })
                            .sum::<f32>()
                    };
                    length(a).total_cmp(&length(b))
                });
                let Some(path) = paths.into_iter().find(|path| {
                    let mut probe = walker.clone();
                    for &goal in path {
                        let mut reached = false;
                        for _ in 0..480 {
                            let d = distance(probe.feet, goal);
                            if d < 0.04 {
                                reached = true;
                                break;
                            }
                            let before = probe.feet;
                            probe.step(
                                &solid,
                                [(goal[0] - before[0]) / d, (goal[1] - before[1]) / d],
                                (d * 60.).min(2.2),
                                false,
                                1. / 60.,
                            );
                            if !probe.grounded
                                || distance(before, probe.feet) < 0.0001
                                || (probe.feet[2] - goal[2]).abs() > 0.5
                            {
                                break;
                            }
                        }
                        if !reached {
                            return false;
                        }
                    }
                    true
                }) else {
                    continue;
                };
                path
            } else {
                vec![]
            };
            return Some(Self {
                phase: if walker.is_some() {
                    Phase::Approach
                } else {
                    Phase::GetOut
                },
                clock: 0.,
                elapsed: 0.,
                side,
                anchor: exit.feet,
                entering: walker.is_some(),
                path,
                next: 0,
                exit,
            });
        }
        None
    }
    pub fn approaching(&self) -> bool {
        self.phase == Phase::Approach
    }
    pub fn clip(&self) -> &'static str {
        match self.phase {
            Phase::Approach => "WALK_player",
            Phase::Align => "CAR_align_RHS",
            Phase::Open => "CAR_open_RHS",
            Phase::GetIn => "CAR_getinL_RHS",
            Phase::ShutInside => "CAR_closedoorL_RHS",
            Phase::GetOut => "CAR_getoutL_RHS",
            Phase::ShutOutside | Phase::Complete => "CAR_close_RHS",
        }
    }
    pub fn pose(&self) -> Pose {
        Pose {
            clip: self.clip(),
            time: self.clock,
            looping: self.approaching(),
            root_height: !self.approaching(),
        }
    }
    pub fn opening(&self, actor: &Character) -> f32 {
        let t = (self.clock / actor.duration(self.clip()).unwrap_or(1.)).clamp(0., 1.);
        match self.phase {
            Phase::Open => (t * 1.5 - 0.5).clamp(0., 1.),
            Phase::GetIn => 1.,
            Phase::ShutInside => 1. - t,
            Phase::GetOut => (t * 4.).min(1.),
            Phase::ShutOutside => 1. - t,
            _ => 0.,
        }
    }
    pub fn position(&self, actor: &Character, car: &Car, walker: Option<&Walker>) -> [f32; 3] {
        if self.approaching() {
            return walker.map_or(self.anchor, |w| w.feet);
        }
        let root = actor
            .root_translation(self.clip(), self.clock)
            .unwrap_or([0.; 3]);
        let (s, c) = car.heading.sin_cos();
        // Root Z is already in the skinned pose. Mirror RHS for the left door.
        [
            self.anchor[0] + c * root[0] * self.side + s * root[1],
            self.anchor[1] - s * root[0] * self.side + c * root[1],
            self.anchor[2],
        ]
    }
    pub fn tick(
        &mut self,
        world: &impl CollisionQuery,
        actor: &Character,
        car: &Car,
        model: &VehicleModel,
        walker: &mut Option<Walker>,
        heading: &mut f32,
        dt: f32,
    ) -> Outcome {
        if !dt.is_finite() || dt <= 0. {
            return Outcome::Running;
        }
        let dt = dt.min(0.1);
        self.elapsed += dt;
        self.clock += dt;
        if self.approaching() {
            let Some(w) = walker else {
                return Outcome::Cancelled;
            };
            if self.elapsed > 12. {
                return Outcome::Cancelled;
            }
            let goal = self.path[self.next];
            let d = distance(w.feet, goal);
            if d < 0.04 {
                self.next += 1;
                if self.next == self.path.len() {
                    self.anchor = w.feet;
                    self.phase = Phase::Align;
                    self.clock = 0.;
                    *heading = car.heading;
                }
            } else {
                let axis = [(goal[0] - w.feet[0]) / d, (goal[1] - w.feet[1]) / d];
                *heading = axis[0].atan2(axis[1]);
                let instances = [model.collision.at(car.position, car.heading)];
                w.step(
                    &CollisionScene {
                        world,
                        instances: &instances,
                    },
                    axis,
                    (d / dt).min(2.2),
                    false,
                    dt,
                );
            }
            return Outcome::Running;
        }
        *heading = car.heading;
        if self.clock >= actor.duration(self.clip()).unwrap_or(1.) {
            self.clock = 0.;
            self.phase = match self.phase {
                Phase::Align => Phase::Open,
                Phase::Open => Phase::GetIn,
                Phase::GetIn => Phase::ShutInside,
                Phase::ShutInside | Phase::ShutOutside => Phase::Complete,
                Phase::GetOut => Phase::ShutOutside,
                p => p,
            };
            if self.phase == Phase::Complete {
                if self.entering {
                    *walker = None;
                    return Outcome::Entered;
                }
                // Recheck dynamic obstacles at completion; never materialize in
                // a pedestrian/traffic car that arrived while the door opened.
                let instances = [model.collision.at(car.position, car.heading)];
                if !self.exit.has_clear_body(&CollisionScene {
                    world,
                    instances: &instances,
                }) {
                    return Outcome::Cancelled;
                }
                *walker = Some(self.exit.clone());
                return Outcome::Exited;
            }
        }
        Outcome::Running
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"phase":format!("{:?}",self.phase),"clip":self.clip(),"time":self.clock,"side":self.side,"anchor":self.anchor,"entering":self.entering})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{test_box_collider, CollisionWorld, Triangle};
    #[test]
    #[ignore = "Requires the user's local ISO"]
    fn native_vehicle_access_walks_around_body_animates_doors_and_exits_safely() {
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let actor = Character::load(&mut disc, &game).unwrap();
        let model = VehicleModel::load_banshee(&mut disc, &game).unwrap();
        Access::validate(&actor).unwrap();
        println!("native vehicle frames {:?}", model.stats.frame_positions);
        for name in [
            "CAR_open_RHS",
            "CAR_getinL_RHS",
            "CAR_closedoorL_RHS",
            "CAR_getoutL_RHS",
            "CAR_close_RHS",
            "CAR_Lsit",
        ] {
            println!(
                "{name}: start {:?} end {:?}",
                actor.root_translation(name, 0.),
                actor.root_translation(name, 99.)
            );
        }
        assert_eq!(model.doors.len(), 2);
        for door in &model.doors {
            let mut mesh = model.triangles.clone();
            model.animate_door(&mut mesh, door.side, 1.);
            let mut displacement = 0.0f32;
            for (i, (a, b)) in mesh.iter().zip(&model.triangles).enumerate() {
                for (v, u) in a.vertices.iter().zip(b.vertices) {
                    if door.triangle_range.contains(&i) {
                        displacement = displacement.max(distance(v.p, u.p));
                    } else {
                        assert_eq!(
                            v.p, u.p,
                            "opening one door must not move the body/other door"
                        );
                    }
                }
            }
            assert!(displacement > 0.5);
            println!(
                "door {} hinge {:?} range {:?}",
                door.side, door.hinge, door.triangle_range
            );
        }
        let world = CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-30., -30., 0.], [30., -30., 0.], [30., 30., 0.]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-30., -30., 0.], [30., 30., 0.], [-30., 30., 0.]],
                surface: 0,
                model_id: 1,
            },
        ]);
        let mut car = Car::new([0., 0., 2.], 0.);
        car.settle_on_ground(&world, true, 0.);
        for start in [[-3., 0., 2.], [3., 0., 2.], [0., -5., 2.]] {
            let mut w = Some(
                Walker::from_camera(
                    &world,
                    &Camera {
                        pos: start,
                        yaw: 0.,
                        pitch: 0.,
                    },
                )
                .unwrap(),
            );
            let mut a = Access::begin(&world, &car, &model, w.as_ref())
                .expect("clear route around native car");
            let mut h = 0.;
            let mut outcome = Outcome::Running;
            let mut phases = std::collections::HashSet::new();
            for _ in 0..1800 {
                phases.insert(a.clip());
                outcome = a.tick(&world, &actor, &car, &model, &mut w, &mut h, 1. / 120.);
                if outcome != Outcome::Running {
                    break;
                }
                if a.approaching() {
                    let instances = [model.collision.at(car.position, car.heading)];
                    assert!(w.as_ref().unwrap().body_clearance(
                        &CollisionScene {
                            world: &world,
                            instances: &instances
                        },
                        0.29
                    ));
                }
            }
            assert_eq!(outcome, Outcome::Entered, "start={start:?}");
            assert!(w.is_none());
            assert!(
                phases.contains("CAR_getinL_RHS")
                    && phases.contains("CAR_open_RHS")
                    && phases.contains("CAR_closedoorL_RHS")
            );
            let mut a = Access::begin(&world, &car, &model, None).unwrap();
            for _ in 0..300 {
                outcome = a.tick(&world, &actor, &car, &model, &mut w, &mut h, 1. / 120.);
                if outcome != Outcome::Running {
                    break;
                }
            }
            assert_eq!(outcome, Outcome::Exited);
            assert!(w.as_ref().unwrap().grounded);
        }
        let wall = test_box_collider([[-5., -4., 0.], [-1.25, 4., 4.]]);
        let opposite = test_box_collider([[1.25, -4., 0.], [5., 4., 4.]]);
        let shapes = [wall.at([0.; 3], 0.), opposite.at([0.; 3], 0.)];
        let one = CollisionScene {
            world: &world,
            instances: &shapes[..1],
        };
        assert_eq!(Access::begin(&one, &car, &model, None).unwrap().side, 1.);
        let blocked = CollisionScene {
            world: &world,
            instances: &shapes,
        };
        assert!(Access::begin(&blocked, &car, &model, None).is_none());
        for moving in [false, true] {
            let pose = Pose {
                clip: "WEAPON_crouch",
                time: 1.,
                looping: false,
                root_height: true,
            };
            for gait in [0., 0.2, 0.5, 0.8] {
                let mesh = actor.crouch_triangles(pose, gait, moving);
                let z: Vec<_> = mesh
                    .iter()
                    .flat_map(|t| t.vertices.iter().map(|v| v.p[2]))
                    .collect();
                let min = z.iter().copied().fold(f32::INFINITY, f32::min);
                let max = z.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                println!("crouch moving={moving} gait={gait} z={min}..{max}");
                assert!(min.abs() < 1e-4 && max < 1.45);
            }
        }
    }
}
