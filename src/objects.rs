//! Provisional street-object dynamics. Native meshes, placement, collision and
//! mass are preserved. Break thresholds, friction and hinged falling are PC
//! approximations, not decoded VCS equations. No generic building destruction.
//! Tilt collision filtering and distant map-prop recycling are ported from
//! SLES_546.22; see the source addresses beside those rules below.
use crate::collision::{CollisionInstance, CollisionQuery, CollisionWorld};
use crate::driving::Car;
use crate::iso::Result;
use crate::leeds::{Instance, Level};
use std::collections::HashMap;

type Point = [f32; 3];
type Matrix = [f32; 16];
const IDENTITY: Matrix = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];
const STEP: f32 = 1. / 120.;
// SLES 0x35027c and 0x350324 compare object matrix up.z to 0x3f28f5c3.
const ACTOR_COLLISION_UP_Z: f32 = 0.66;
// SLES 0x2b06d0: BOTH current and saved original origins must be >80m away.
const RECYCLE_DISTANCE: f32 = 80.;

#[derive(Clone)]
pub struct Binding {
    pub entity: usize,
    pub level: usize,
    pub instance: usize,
    pub mass: f32,
    pub pole: bool,
    pub pivot: Point,
    pub original_up: Point,
    pub ignore_when_tilted: bool,
    pub restore_radius: f32,
}
struct Body {
    binding: Binding,
    original: Instance,
    origin: Point,
    points: Vec<Point>,
    matrix: Matrix,
    velocity: Point,
    direction: Point,
    angle: f32,
    angular_speed: f32,
    awake: bool,
    sleeping: bool,
    sleep_time: f32,
}
pub struct Objects {
    bodies: Vec<Body>,
    lookup: HashMap<usize, usize>,
    car_mass: f32,
    accumulator: f32,
    impacts: usize,
    recycled: usize,
}
impl Objects {
    pub fn new(bindings: &[Binding], levels: &[Level], car_mass: f32) -> Result<Self> {
        if !car_mass.is_finite() || car_mass <= 0. {
            return Err("Invalid native vehicle mass".into());
        }
        let mut bodies = Vec::new();
        let mut lookup = HashMap::new();
        for binding in bindings {
            let original = levels
                .get(binding.level)
                .and_then(|l| l.instances.get(binding.instance))
                .ok_or("Invalid dynamic furniture binding")?
                .clone();
            if !binding.mass.is_finite()
                || binding.mass <= 0.
                || lookup.contains_key(&binding.entity)
            {
                return Err("Invalid dynamic furniture mass/identity".into());
            }
            lookup.insert(binding.entity, bodies.len());
            bodies.push(Body {
                binding: binding.clone(),
                original,
                origin: binding.pivot,
                points: Vec::new(),
                matrix: IDENTITY,
                velocity: [0.; 3],
                direction: [0.; 3],
                angle: 0.,
                angular_speed: 0.,
                awake: false,
                sleeping: false,
                sleep_time: 0.,
            });
        }
        Ok(Self {
            bodies,
            lookup,
            car_mass,
            accumulator: 0.,
            impacts: 0,
            recycled: 0,
        })
    }

    /// Consume the actual car sweep contact, never a proximity trigger. A solid
    /// wall/other actor cannot wake a prop hidden behind it or sharing its model.
    pub fn impact(&mut self, world: &mut CollisionWorld, car: &mut Car) {
        let Some(impact) = car.impact.take() else {
            return;
        };
        let Some(index) = impact
            .contact
            .entity
            .and_then(|e| self.lookup.get(&e))
            .copied()
        else {
            return;
        };
        let body = &mut self.bodies[index];
        let n = impact.contact.normal;
        let speed = (-dot(impact.velocity, n)).max(0.);
        let impulse =
            speed * self.car_mass * body.binding.mass / (self.car_mass + body.binding.mass);
        let change = impulse / body.binding.mass;
        // Deliberately provisional break/push thresholds, in this engine's units.
        if change < if body.binding.pole { 0.8 } else { 0.12 } {
            return;
        }
        if body.binding.pole && body.awake {
            return;
        }
        if !body.awake {
            let Some(points) = world.awaken(body.binding.entity) else {
                return;
            };
            body.points = points;
            body.binding.pivot[2] = body
                .points
                .iter()
                .map(|p| p[2])
                .fold(f32::INFINITY, f32::min);
            body.awake = true;
            if !body.binding.pole {
                body.matrix[14] += body.support_rise(world, &body.matrix, 0.2);
                world.move_entity(body.binding.entity, body.matrix);
            }
        }
        let length = n[0].hypot(n[1]);
        if length < 0.1 {
            return;
        }
        body.direction = [-n[0] / length, -n[1] / length, 0.];
        body.sleeping = false;
        if body.binding.pole {
            // Serialized roots can be offset from the foot of a curved lamp.
            // Tip over the forward edge of the native base, not its root/centre:
            // the latter would drive the front of the base into the pavement.
            let base: Vec<_> = body
                .points
                .iter()
                .filter(|p| p[2] < body.binding.pivot[2] + 0.35)
                .collect();
            if !base.is_empty() {
                for a in 0..2 {
                    body.binding.pivot[a] =
                        base.iter().map(|p| p[a]).sum::<f32>() / base.len() as f32;
                }
                let forward = base
                    .iter()
                    .map(|p| dot(**p, body.direction))
                    .fold(f32::NEG_INFINITY, f32::max);
                let shift = forward - dot(body.binding.pivot, body.direction);
                for a in 0..2 {
                    body.binding.pivot[a] += body.direction[a] * shift;
                }
            }
            let height = body
                .points
                .iter()
                .map(|p| p[2] - body.binding.pivot[2])
                .fold(1., f32::max);
            body.angular_speed = (change * 2. / height).clamp(0.3, 2.);
        } else {
            for a in 0..2 {
                body.velocity[a] += body.direction[a] * change.min(8.);
            }
            let speed = body.velocity[0].hypot(body.velocity[1]);
            if speed > 8. {
                for a in 0..2 {
                    body.velocity[a] *= 8. / speed;
                }
            }
        }
        // The sweep has already stopped before contact. Retain only the
        // post-impact momentum for the NEXT tick; never teleport through it.
        let forward = [car.heading.sin(), car.heading.cos(), 0.];
        car.speed =
            dot(impact.velocity, forward) * self.car_mass / (self.car_mass + body.binding.mass);
        self.impacts += 1;
    }

    pub fn step(
        &mut self,
        levels: &mut [Level],
        world: &mut CollisionWorld,
        blockers: &[CollisionInstance<'_>],
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        self.accumulator += dt.min(0.1);
        while self.accumulator + 1e-7 >= STEP {
            self.accumulator = (self.accumulator - STEP).max(0.);
            for body in &mut self.bodies {
                if !body.awake || !world.entity_active(body.binding.entity) {
                    continue;
                }
                if body.sleeping {
                    // Recheck support periodically: a car or another object
                    // may have moved out from under this object since it rested.
                    body.sleep_time += STEP;
                    if body.sleep_time < 0.25 {
                        continue;
                    }
                    body.sleep_time = 0.;
                    if body.binding.pole && body.angle >= std::f32::consts::FRAC_PI_2 {
                        continue;
                    }
                    body.sleeping = false;
                    if body.binding.pole {
                        body.angular_speed = 0.1;
                    }
                }
                if body.binding.pole {
                    body.fall(world, blockers);
                } else {
                    body.slide(world, blockers);
                }
                world.move_entity(body.binding.entity, body.matrix);
                world.set_ignores_actors(body.binding.entity, body.ignores_actors(&body.matrix));
                let instance = &mut levels[body.binding.level].instances[body.binding.instance];
                instance.matrix = multiply(&body.matrix, &body.original.matrix);
                let center = point(&body.matrix, body.original.sphere[..3].try_into().unwrap());
                instance.sphere[..3].copy_from_slice(&center);
            }
        }
    }

    /// Native map objects start as type 1 (0x1314cc), not temporary type 3.
    /// 0x2b06d0..0x2b0718 recycles them beyond 80m from both origins, after
    /// an occupancy check (0x2b14d0). 0x2b1248 restores the saved matrix to a
    /// dummy. Vehicle-debris deadlines (4/10/20s) do NOT apply to this pool.
    pub fn recycle_far(
        &mut self,
        levels: &mut [Level],
        world: &mut CollisionWorld,
        blockers: &[CollisionInstance<'_>],
        focus: Point,
    ) {
        if focus.iter().any(|v| !v.is_finite()) {
            return;
        }
        for body in &mut self.bodies {
            if !body.awake
                || !body.far_from(focus)
                || !world.restoration_clear(
                    body.binding.entity,
                    body.origin,
                    body.binding.restore_radius,
                    blockers,
                )
            {
                continue;
            }
            levels[body.binding.level].instances[body.binding.instance] = body.original.clone();
            world.restore_entity(body.binding.entity);
            body.reset();
            self.recycled += 1;
        }
    }

    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"available":self.bodies.len(),"impacts":self.impacts,"recycled":self.recycled,
            "moved":self.bodies.iter().filter(|b|b.awake).map(|b|serde_json::json!({
                "entity":b.binding.entity,"pole":b.binding.pole,"mass":b.binding.mass,
                "angle_degrees":b.angle.to_degrees(),"sleeping":b.sleeping,
                "ignores_actors":b.ignores_actors(&b.matrix),
                "pivot":point(&b.matrix,b.binding.pivot),"matrix":b.matrix
            })).collect::<Vec<_>>()})
    }
}

impl Body {
    fn ignores_actors(&self, matrix: &Matrix) -> bool {
        self.binding.ignore_when_tilted
            && dot([matrix[2], matrix[6], matrix[10]], self.binding.original_up)
                < ACTOR_COLLISION_UP_Z
    }

    fn far_from(&self, focus: Point) -> bool {
        [self.origin, point(&self.matrix, self.origin)]
            .iter()
            .all(|p| {
                (0..3).map(|a| (p[a] - focus[a]).powi(2)).sum::<f32>() > RECYCLE_DISTANCE.powi(2)
            })
    }

    fn reset(&mut self) {
        self.binding.pivot = self.origin;
        self.points.clear();
        self.matrix = IDENTITY;
        self.velocity = [0.; 3];
        self.direction = [0.; 3];
        self.angle = 0.;
        self.angular_speed = 0.;
        self.awake = false;
        self.sleeping = false;
        self.sleep_time = 0.;
    }

    fn sweep(
        &self,
        world: &CollisionWorld,
        blockers: &[CollisionInstance<'_>],
        next: &Matrix,
    ) -> f32 {
        let mut fraction: f32 = 1.;
        for &p in &self.points {
            // The lamp's base is anchored and may start embedded in pavement.
            if self.binding.pole && p[2] < self.binding.pivot[2] + 0.2 {
                continue;
            }
            let from = point(&self.matrix, p);
            let to = point(next, p);
            let delta = std::array::from_fn(|a| to[a] - from[a]);
            let length = dot(delta, delta).sqrt();
            if length < 1e-7 {
                continue;
            }
            let mut hit =
                world.raycast_except(from, delta, length + 0.008, Some(self.binding.entity));
            for blocker in blockers.iter().filter(|_| !self.ignores_actors(next)) {
                if let Some(h) = blocker.raycast(
                    from,
                    delta,
                    hit.as_ref().map_or(length + 0.008, |h| h.distance),
                ) {
                    hit = Some(h);
                }
            }
            if let Some(h) = hit {
                fraction = fraction.min(((h.distance - 0.008) / length).clamp(0., 1.));
            }
        }
        fraction
    }

    fn fall(&mut self, world: &CollisionWorld, blockers: &[CollisionInstance<'_>]) {
        // Hinged fall under gravity, using a single rigid transform for mesh
        // and collider. No guessed native inertia or fracture fields.
        self.angular_speed += 3. * (self.angle + 0.03).sin() * STEP;
        let wanted = (self.angle + self.angular_speed * STEP).min(std::f32::consts::FRAC_PI_2);
        let next = tilt(self.binding.pivot, self.direction, wanted);
        let fraction = self.sweep(world, blockers, &next);
        self.angle += (wanted - self.angle) * fraction;
        self.matrix = tilt(self.binding.pivot, self.direction, self.angle);
        if fraction < 1. || self.angle >= std::f32::consts::FRAC_PI_2 {
            self.angular_speed = 0.;
            self.sleeping = true;
        }
    }

    // Follow gentle terrain slopes with the lowest native contact points.
    // The small search window does not let a bench climb walls or tall curbs.
    fn support_rise(&self, world: &CollisionWorld, next: &Matrix, limit: f32) -> f32 {
        let mut rise: f32 = 0.;
        for &p in &self.points {
            if p[2] > self.binding.pivot[2] + 0.25 {
                continue;
            }
            let q = point(next, p);
            if let Some(h) = world.raycast_except(
                [q[0], q[1], q[2] + limit],
                [0., 0., -1.],
                limit + 0.06,
                Some(self.binding.entity),
            ) {
                if h.normal[2] > 0.75 {
                    rise = rise.max((h.position[2] + 0.008 - q[2]).clamp(0., limit));
                }
            }
        }
        rise
    }

    fn slide(&mut self, world: &CollisionWorld, blockers: &[CollisionInstance<'_>]) {
        self.velocity[2] = (self.velocity[2] - 9.81 * STEP).max(-25.);
        let mut grounded = false;
        for axis in [0, 1, 2] {
            let mut next = self.matrix;
            next[12 + axis] += self.velocity[axis] * STEP;
            let rise = if axis < 2 {
                self.support_rise(world, &next, 0.04)
            } else {
                0.
            };
            next[14] += rise;
            grounded |= rise > 0.;
            let fraction = self.sweep(world, blockers, &next);
            self.matrix[12 + axis] += self.velocity[axis] * STEP * fraction;
            self.matrix[14] += rise * fraction;
            if fraction < 1. {
                if axis == 2 && self.velocity[2] < 0. {
                    grounded = true;
                }
                self.velocity[axis] = 0.;
            }
        }
        if grounded {
            let speed = self.velocity[0].hypot(self.velocity[1]);
            let remaining = (speed - 3.5 * STEP).max(0.);
            if speed > 0. {
                for a in 0..2 {
                    self.velocity[a] *= remaining / speed;
                }
            }
            if remaining < 0.02 {
                self.velocity = [0.; 3];
                self.sleeping = true;
            }
        }
    }
}

fn dot(a: Point, b: Point) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn point(m: &Matrix, p: Point) -> Point {
    std::array::from_fn(|a| m[a] * p[0] + m[4 + a] * p[1] + m[8 + a] * p[2] + m[12 + a])
}
fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    std::array::from_fn(|i| (0..4).map(|k| a[k * 4 + i % 4] * b[i / 4 * 4 + k]).sum())
}
fn tilt(pivot: Point, direction: Point, angle: f32) -> Matrix {
    let axis = [-direction[1], direction[0], 0.];
    let (s, c) = angle.sin_cos();
    let [x, y, z] = axis;
    let mut m = [
        c + x * x * (1. - c),
        y * x * (1. - c) + z * s,
        z * x * (1. - c) - y * s,
        0.,
        x * y * (1. - c) - z * s,
        c + y * y * (1. - c),
        z * y * (1. - c) + x * s,
        0.,
        x * z * (1. - c) + y * s,
        y * z * (1. - c) - x * s,
        c + z * z * (1. - c),
        0.,
        0.,
        0.,
        0.,
        1.,
    ];
    let rotated = point(&m, pivot);
    for a in 0..3 {
        m[12 + a] = pivot[a] - rotated[a];
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{test_box_collider, Triangle};
    #[test]
    #[ignore = "Requires the user's local VCS PS2 PAL ISO"]
    fn native_car_impacts_move_furniture_and_capture() {
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut levels = vec![
            crate::leeds::load_level(&mut disc, "BEACH").unwrap(),
            crate::leeds::load_level(&mut disc, "MAINLA").unwrap(),
        ];
        let furniture = crate::props::attach(&mut disc, &game, &mut levels).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        world.set_active_levels(&[1, 2]);
        let vehicle = crate::vehicle::VehicleModel::load_banshee(&mut disc, &game).unwrap();
        let points = vehicle.collision.support_points();
        println!(
            "native car collider bounds {:?}",
            [
                std::array::from_fn::<_, 3, _>(|a| points
                    .iter()
                    .map(|p| p[a])
                    .fold(f32::INFINITY, f32::min)),
                std::array::from_fn::<_, 3, _>(|a| points
                    .iter()
                    .map(|p| p[a])
                    .fold(f32::NEG_INFINITY, f32::max))
            ]
        );
        let mass = crate::iso::f32le(&game, vehicle.stats.handling_record_address + 0xbc).unwrap();
        let mut objects = Objects::new(&furniture.bindings, &levels, mass).unwrap();
        let out = std::path::Path::new(crate::DEFAULT_ISO)
            .parent()
            .unwrap()
            .join("object-physics-check");
        std::fs::create_dir_all(&out).unwrap();
        for (label, pole, target) in [
            ("bench", false, [-207.5587, -968.0712]),
            ("lamp", true, [233.43976, -669.8031]),
        ] {
            let index = objects
                .bodies
                .iter()
                .position(|b| {
                    b.binding.pole == pole
                        && (b.binding.pivot[0] - target[0]).abs() < 0.1
                        && (b.binding.pivot[1] - target[1]).abs() < 0.1
                })
                .unwrap();
            let binding = objects.bodies[index].binding.clone();
            let mut contact = None;
            for side in 0..16 {
                let heading = side as f32 * std::f32::consts::TAU / 16.;
                let (s, c) = heading.sin_cos();
                let start = [
                    binding.pivot[0] - s * 12.,
                    binding.pivot[1] - c * 12.,
                    binding.pivot[2] + 5.,
                ];
                let Some(ground) = world.raycast(start, [0., 0., -1.], 20.) else {
                    continue;
                };
                let mut car = Car::new([start[0], start[1], ground.position[2] + 0.015], heading);
                car.settle_on_ground(&world, true, 0.);
                car.speed = 15.;
                for _ in 0..240 {
                    car.step(&world, 0., 0., 0., STEP);
                    if let Some(hit) = car.impact {
                        if hit.contact.entity == Some(binding.entity) {
                            contact = Some(car);
                        }
                        break;
                    }
                }
                if contact.is_some() {
                    break;
                }
            }
            let mut car = contact.unwrap_or_else(|| panic!("no actual car contact with {label}"));
            println!(
                "{label} contact car={:?} heading={} impact={:?}",
                car.position, car.heading, car.impact
            );
            let ground = car.position[2];
            let view_heading = car.heading + 0.9;
            let distance = if pole { 18. } else { 12. };
            let target = [
                binding.pivot[0] + car.heading.sin() * 2.,
                binding.pivot[1] + car.heading.cos() * 2.,
                ground + if pole { 2.5 } else { 0.6 },
            ];
            let camera = crate::render::Camera {
                pos: [
                    target[0] - view_heading.sin() * distance,
                    target[1] - view_heading.cos() * distance,
                    target[2] + 5.,
                ],
                yaw: view_heading,
                pitch: -(5.0f32 / distance).atan(),
            };
            let triangles = vehicle.animated_triangles(car.wheel_angle, car.steering_angle);
            let vf = crate::gpu::VehicleFrame {
                triangles: &triangles,
                position: car.position,
                heading: car.heading,
            };
            let mut frame = crate::render::Frame::new();
            frame.draw_scene_all(
                &levels,
                0,
                &camera,
                true,
                None,
                Some((&vf, &vehicle.textures)),
            );
            frame
                .write_bmp(&out.join(format!("{label}-before.bmp")))
                .unwrap();
            objects.impact(&mut world, &mut car);
            assert!(objects.bodies[index].awake);
            for _ in 0..720 {
                objects.step(
                    &mut levels,
                    &mut world,
                    &[vehicle.collision.at(car.position, car.heading)],
                    STEP,
                );
            }
            frame.draw_scene_all(
                &levels,
                0,
                &camera,
                true,
                None,
                Some((&vf, &vehicle.textures)),
            );
            frame
                .write_bmp(&out.join(format!("{label}-after.bmp")))
                .unwrap();
            let b = &objects.bodies[index];
            let center: Point = b.original.sphere[..3].try_into().unwrap();
            let moved_center = point(&b.matrix, center);
            let displacement = (moved_center[0] - center[0]).hypot(moved_center[1] - center[1]);
            println!(
                "{label}: angle={} displacement={} sleeping={} points={}",
                b.angle.to_degrees(),
                displacement,
                b.sleeping,
                b.points.len()
            );
            std::fs::write(
                out.join("physics.json"),
                serde_json::to_vec_pretty(&objects.snapshot()).unwrap(),
            )
            .unwrap();
            assert!(b.sleeping, "{label} must settle");
            if pole {
                assert!(b.angle.to_degrees() > 60., "native lamp must visibly fall");
            } else {
                assert!(displacement > 0.5, "native bench must visibly move");
            }
            let visible = &levels[b.binding.level].instances[b.binding.instance];
            assert_eq!(visible.matrix, multiply(&b.matrix, &b.original.matrix));
            // Compare the moved world query to its exact transformed original
            // shape, using the visible object's updated bounds as the target.
            let center: Point = visible.sphere[..3].try_into().unwrap();
            let hit = world.raycast([center[0], center[1], center[2] + 10.], [0., 0., -1.], 25.);
            assert!(
                hit.is_some(),
                "scene must retain terrain under moved furniture"
            );
            if pole {
                assert!(b.binding.ignore_when_tilted && b.ignores_actors(&b.matrix));
                let start = car.position;
                let mut crossing = None;
                for _ in 0..240 {
                    car.step(&world, 1., 0., 0., STEP);
                    assert_ne!(
                        car.impact.map(|i| i.contact.entity),
                        Some(Some(binding.entity)),
                        "native fallen lamp must no longer stop the Banshee"
                    );
                    objects.impact(&mut world, &mut car);
                    objects.step(
                        &mut levels,
                        &mut world,
                        &[vehicle.collision.at(car.position, car.heading)],
                        STEP,
                    );
                    let forward = [car.heading.sin(), car.heading.cos(), 0.];
                    if crossing.is_none()
                        && dot(std::array::from_fn(|a| car.position[a] - start[a]), forward) > 6.
                    {
                        crossing = Some(car);
                    }
                }
                let forward = [car.heading.sin(), car.heading.cos(), 0.];
                let travel = dot(std::array::from_fn(|a| car.position[a] - start[a]), forward);
                println!("native Banshee travels {travel:.3}m past fallen lamp");
                assert!(travel > 5.);
                let crossing = crossing.expect("car must cross the fallen lamp in view");
                let vf = crate::gpu::VehicleFrame {
                    triangles: &triangles,
                    position: crossing.position,
                    heading: crossing.heading,
                };
                frame.draw_scene_all(
                    &levels,
                    0,
                    &camera,
                    true,
                    None,
                    Some((&vf, &vehicle.textures)),
                );
                frame.write_bmp(&out.join("lamp-car-passed.bmp")).unwrap();
            }
            let origin = objects.bodies[index].origin;
            let far = [origin[0] + 200., origin[1] + 200., origin[2]];
            objects.recycle_far(&mut levels, &mut world, &[], origin);
            assert!(
                objects.bodies[index].awake,
                "no timer cleanup beside player"
            );
            objects.recycle_far(
                &mut levels,
                &mut world,
                &[vehicle.collision.at(origin, 0.)],
                far,
            );
            assert!(objects.bodies[index].awake, "occupied placement must wait");
            objects.recycle_far(&mut levels, &mut world, &[], far);
            let b = &objects.bodies[index];
            assert!(!b.awake, "distant native furniture must recycle");
            assert_eq!(
                levels[b.binding.level].instances[b.binding.instance].matrix,
                b.original.matrix
            );
            // Same original car sweep must hit the restored original placement.
            let mut retry = contact.unwrap();
            retry.speed = 8.;
            retry.step(&world, 1., 0., 0., STEP);
            assert_eq!(retry.impact.unwrap().contact.entity, Some(binding.entity));
            objects.impact(&mut world, &mut retry);
            assert!(
                objects.bodies[index].awake,
                "recycled prop can be struck again"
            );
            objects.recycle_far(&mut levels, &mut world, &[], far);
            frame.draw_scene_all(&levels, 0, &camera, true, None, None);
            frame
                .write_bmp(&out.join(format!("{label}-restored.bmp")))
                .unwrap();
        }
        std::fs::write(
            out.join("lifecycle.json"),
            serde_json::to_vec_pretty(&objects.snapshot()).unwrap(),
        )
        .unwrap();
    }
    fn ground() -> CollisionWorld {
        CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-100., -100., 0.], [100., -100., 0.], [100., 100., 0.]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-100., -100., 0.], [100., 100., 0.], [-100., 100., 0.]],
                surface: 0,
                model_id: 1,
            },
        ])
    }
    fn body(world: &mut CollisionWorld, pole: bool) -> Body {
        world.test_add_entity(
            42,
            test_box_collider(if pole {
                [[-0.08, -0.08, 0.], [0.08, 0.08, 7.]]
            } else {
                [[-1., -0.4, 0.02], [1., 0.4, 1.]]
            }),
            1,
        );
        Body {
            binding: Binding {
                entity: 42,
                level: 0,
                instance: 0,
                mass: if pole { 600. } else { 50. },
                pole,
                pivot: [0.; 3],
                original_up: [0., 0., 1.],
                ignore_when_tilted: pole,
                restore_radius: if pole { 7. } else { 2. },
            },
            origin: [0.; 3],
            original: Instance {
                id: 0,
                resource: 0,
                pass: 4,
                matrix: IDENTITY,
                sphere: [0., 0., 3., 4.],
            },
            points: world.awaken(42).unwrap(),
            matrix: IDENTITY,
            velocity: [4., 0., 0.],
            direction: [1., 0., 0.],
            angle: 0.,
            angular_speed: 1.,
            awake: true,
            sleeping: false,
            sleep_time: 0.,
        }
    }
    #[test]
    fn moving_one_placement_removes_only_its_old_collision() {
        let mut world = ground();
        let mut b = body(&mut world, false);
        world.test_add_entity(43, test_box_collider([[-1., 4., 0.02], [1., 5., 1.]]), 1);
        b.matrix[12] = 10.;
        world.move_entity(42, b.matrix);
        assert!(world.raycast([-3., 0., 0.5], [1., 0., 0.], 6.).is_none());
        assert_eq!(
            world
                .raycast([7., 0., 0.5], [1., 0., 0.], 6.)
                .unwrap()
                .entity,
            Some(42)
        );
        assert_eq!(
            world
                .raycast([-3., 4.5, 0.5], [1., 0., 0.], 6.)
                .unwrap()
                .entity,
            Some(43)
        );
        world.set_active_level(Some(3));
        assert!(world.raycast([7., 0., 0.5], [1., 0., 0.], 6.).is_none());
    }
    #[test]
    fn bench_slides_stops_and_remains_above_ground() {
        let mut world = ground();
        let mut b = body(&mut world, false);
        for _ in 0..600 {
            if !b.sleeping {
                b.slide(&world, &[]);
                world.move_entity(42, b.matrix);
            }
        }
        assert!(b.sleeping);
        assert!(b.matrix[12] > 1. && b.matrix[12] < 4.);
        for p in &b.points {
            assert!(point(&b.matrix, *p)[2] >= -0.001);
        }
    }
    #[test]
    fn bench_does_not_tunnel_through_wall_or_actor() {
        for actor in [false, true] {
            let mut world = ground();
            let mut b = body(&mut world, false);
            b.velocity = [8., 0., 0.];
            let wall = test_box_collider([[3., -4., 0.], [3.05, 4., 4.]]);
            if !actor {
                world.test_add_entity(43, wall.clone(), 1);
            }
            let blockers = if actor {
                vec![wall.at([0.; 3], 0.)]
            } else {
                vec![]
            };
            for _ in 0..300 {
                if !b.sleeping {
                    b.slide(&world, &blockers);
                    world.move_entity(42, b.matrix);
                }
            }
            assert!(b.matrix[12] > 1.5 && b.matrix[12] < 2., "{:?}", b.matrix);
        }
    }
    #[test]
    fn lamp_falls_and_its_collision_rotates_with_mesh() {
        let mut world = ground();
        let mut b = body(&mut world, true);
        assert!(world.raycast([-1., 0., 5.], [1., 0., 0.], 2.).is_some());
        for _ in 0..600 {
            if !b.sleeping {
                b.fall(&world, &[]);
                world.move_entity(42, b.matrix);
            }
        }
        assert!(b.sleeping && b.angle.to_degrees() > 85.);
        assert!(world.raycast([-1., 0., 5.], [1., 0., 0.], 2.).is_none());
        assert_eq!(
            world
                .raycast([5., 0., 1.], [0., 0., -1.], 2.)
                .unwrap()
                .entity,
            Some(42)
        );
        for p in &b.points {
            if p[2] > 0.2 {
                assert!(point(&b.matrix, *p)[2] >= -0.001);
            }
        }
    }

    #[test]
    fn native_tilt_rule_only_filters_actors_and_preserves_walls() {
        use crate::collision::{ActorQuery, CollisionScene, Ray};
        let mut world = ground();
        let mut b = body(&mut world, true);
        for (up, ignored) in [(0.661, false), (0.66, false), (0.659, true), (0., true)] {
            let mut matrix = IDENTITY;
            matrix[10] = up;
            assert_eq!(b.ignores_actors(&matrix), ignored);
        }
        b.angle = 60f32.to_radians();
        b.matrix = tilt([0.; 3], [1., 0., 0.], b.angle);
        world.move_entity(42, b.matrix);
        world.set_ignores_actors(42, b.ignores_actors(&b.matrix));
        let ray = Ray {
            origin: [1.8, -2., 1.],
            direction: [0., 1., 0.],
            distance: 4.,
        };
        assert_eq!(
            world
                .raycast(ray.origin, ray.direction, ray.distance)
                .unwrap()
                .entity,
            Some(42)
        );
        let actors = ActorQuery(&world);
        assert!(actors
            .raycast(ray.origin, ray.direction, ray.distance)
            .is_none());
        assert!(actors.raycast_batch(&[ray])[0].is_none());
        world.test_add_entity(43, test_box_collider([[1., 1., 0.], [3., 1.1, 2.]]), 1);
        let scene = CollisionScene {
            world: &world,
            instances: &[],
        };
        let actors = ActorQuery(&scene);
        assert_eq!(
            actors
                .raycast(ray.origin, ray.direction, ray.distance)
                .unwrap()
                .entity,
            Some(43)
        );
        assert_eq!(actors.raycast_batch(&[ray])[0].unwrap().entity, Some(43));
        b.binding.ignore_when_tilted = false;
        assert!(!b.ignores_actors(&b.matrix));
        world.restore_entity(42);
        assert_eq!(
            world
                .actor_raycast([-1., 0., 5.], [1., 0., 0.], 2.)
                .unwrap()
                .entity,
            Some(42)
        );
        assert!(world.raycast(ray.origin, ray.direction, 2.5).is_none());
    }

    #[test]
    fn distant_recycling_uses_both_origins_and_checks_occupancy() {
        let mut world = ground();
        let mut b = body(&mut world, false);
        b.matrix[12] = 160.;
        world.move_entity(42, b.matrix);
        assert!(!b.far_from([0.; 3]));
        assert!(!b.far_from([160., 0., 0.]));
        assert!(!b.far_from([80., 0., 0.]));
        assert!(b.far_from([80., 1., 0.]));
        let car = test_box_collider([[-1., -2., 0.], [1., 2., 1.5]]);
        assert!(!world.restoration_clear(42, b.origin, 2., &[car.at([0.; 3], 0.)]));
        assert!(world.restoration_clear(42, b.origin, 2., &[car.at([20., 0., 0.], 0.)]));
        world.restore_entity(42);
        b.reset();
        assert!(!b.awake);
        assert_eq!(b.matrix, IDENTITY);
        assert_eq!(b.binding.pivot, b.origin);
        assert_eq!(
            world
                .actor_raycast([-3., 0., 0.5], [1., 0., 0.], 6.)
                .unwrap()
                .entity,
            Some(42)
        );
        assert!(world.raycast([157., 0., 0.5], [1., 0., 0.], 6.).is_none());
    }

    #[test]
    fn car_and_pedestrian_cross_fallen_lamp_without_ground_or_wall_loss() {
        let mut world = ground();
        let mut b = body(&mut world, true);
        // A partially fallen pole intersects bumper and pedestrian height.
        b.matrix = tilt([0.; 3], [1., 0., 0.], 60f32.to_radians());
        world.move_entity(42, b.matrix);
        world.set_ignores_actors(42, b.ignores_actors(&b.matrix));
        let mut car = Car::new([1.8, -8., 0.015], 0.);
        car.speed = 12.;
        let camera = crate::render::Camera {
            pos: [1.8, -2., 1.67],
            yaw: 0.,
            pitch: 0.,
        };
        let mut ped = crate::player::Walker::from_camera(&world, &camera).unwrap();
        for _ in 0..240 {
            car.step(&world, 0.5, 0., 0., STEP);
            assert!(car.impact.is_none());
            ped.step(&world, [0., 1.], 1.8, false, STEP);
        }
        assert!(car.position[1] > 10. && car.grounded);
        assert!(ped.feet[1] > 1. && ped.grounded);
        assert!(car.position[2] < 0.1 && ped.feet[2] < 0.1);
    }
    #[test]
    fn car_hits_thin_poles_between_old_bumper_probes() {
        for x in [-0.73, -0.61, -0.29, 0.15, 0.73] {
            let mut world = ground();
            world.test_add_entity(
                42,
                test_box_collider([[x - 0.04, 0., 0.], [x + 0.04, 0.08, 7.]]),
                1,
            );
            let mut car = Car::new([0., -8., 0.015], 0.);
            car.speed = 20.;
            let mut hit = false;
            for _ in 0..120 {
                car.step(&world, 0., 0., 0., STEP);
                if let Some(impact) = car.impact {
                    assert_eq!(impact.contact.entity, Some(42));
                    hit = true;
                    break;
                }
            }
            assert!(hit, "missed pole x={x}");
            assert!(car.position[1] < -2.7);
        }
    }
}
