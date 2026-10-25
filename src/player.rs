//! Experimental first-person movement on original static PS2 collision surfaces.
//! This is a new controller, not a claim to reproduce VCS pedestrian physics.
//! Discrete body and footprint rays approximate a capsule. The dense circular
//! sweep covers native street poles; sub-probe gaps remain a limitation.
use crate::collision::{ActorQuery, CollisionQuery, Ray};
use crate::render::Camera;

const EYE: f32 = 1.65;
const RADIUS: f32 = 0.30;
const STEP: f32 = 0.35;
const HEIGHT: f32 = 1.85; // Native player's bind height is 1.806 m, plus head clearance.
const SKIN: f32 = 0.02;
const WALKABLE: f32 = 0.70;
// Slightly pad ground support beyond the horizontal skin. Otherwise a diagonal
// curb can stop the body just before any of its feet rays reaches the top.
const FOOTPRINT: [[f32; 2]; 17] = {
    let ring = [
        [1.0, 0.0],
        [0.9238795, 0.3826834],
        [0.7071068, 0.7071068],
        [0.3826834, 0.9238795],
        [0.0, 1.0],
        [-0.3826834, 0.9238795],
        [-0.7071068, 0.7071068],
        [-0.9238795, 0.3826834],
        [-1.0, 0.0],
        [-0.9238795, -0.3826834],
        [-0.7071068, -0.7071068],
        [-0.3826834, -0.9238795],
        [0.0, -1.0],
        [0.3826834, -0.9238795],
        [0.7071068, -0.7071068],
        [0.9238795, -0.3826834],
    ];
    let mut footprint = [[0.0; 2]; 17];
    let mut i = 0;
    while i < 16 {
        footprint[i + 1] = [ring[i][0] * (RADIUS + SKIN), ring[i][1] * (RADIUS + SKIN)];
        i += 1;
    }
    footprint
};

fn footprint_rays(position: [f32; 3], direction: [f32; 3], distance: f32) -> [Ray; 17] {
    FOOTPRINT.map(|o| Ray {
        origin: [position[0] + o[0], position[1] + o[1], position[2]],
        direction,
        distance,
    })
}
fn support(world: &impl CollisionQuery, position: [f32; 3], distance: f32) -> Option<f32> {
    world
        .raycast_batch(&footprint_rays(position, [0., 0., -1.], distance))
        .into_iter()
        .flatten()
        .filter(|h| h.normal[2] >= WALKABLE)
        .map(|h| h.position[2] + SKIN)
        .max_by(f32::total_cmp)
}
fn clear_rise(world: &impl CollisionQuery, feet: [f32; 3], rise: f32, height: f32) -> f32 {
    world
        .raycast_batch(&footprint_rays(
            [feet[0], feet[1], feet[2] + height],
            [0., 0., 1.],
            rise + SKIN,
        ))
        .into_iter()
        .flatten()
        .fold(rise, |allowed, h| allowed.min((h.distance - SKIN).max(0.)))
}
fn standing_clear(world: &impl CollisionQuery, feet: [f32; 3], height: f32) -> bool {
    world
        .raycast_batch(&footprint_rays(
            [feet[0], feet[1], feet[2] + SKIN],
            [0., 0., 1.],
            height - SKIN,
        ))
        .iter()
        .all(Option::is_none)
}

#[derive(Clone)]
pub struct Walker {
    pub feet: [f32; 3],
    velocity: f32,
    height: f32,
    pub grounded: bool,
}
impl Walker {
    pub fn from_camera(world: &impl CollisionQuery, camera: &Camera) -> Option<Self> {
        let world = &ActorQuery(world);
        let hit = world.raycast(camera.pos, [0.0, 0.0, -1.0], 800.0)?;
        if hit.normal[2] < WALKABLE {
            return None;
        }
        let mut feet = [camera.pos[0], camera.pos[1], hit.position[2] + SKIN];
        // Establish the same footprint support used during walking, including
        // when switching from flight onto an inclined surface.
        if let Some(floor) = support(world, [feet[0], feet[1], feet[2] + STEP], STEP + SKIN * 2.0) {
            if floor >= feet[2] && floor - feet[2] <= STEP {
                feet[2] = floor;
            }
        }
        // A flying camera can be in a gap too low to stand in.
        for offset in FOOTPRINT {
            let origin = [feet[0] + offset[0], feet[1] + offset[1], feet[2] + SKIN];
            if world
                .raycast(origin, [0.0, 0.0, 1.0], HEIGHT - SKIN)
                .is_some()
            {
                return None;
            }
        }
        Some(Self {
            feet,
            velocity: 0.0,
            height: HEIGHT,
            grounded: true,
        })
    }
    pub fn eye(&self) -> [f32; 3] {
        [
            self.feet[0],
            self.feet[1],
            self.feet[2] + EYE - (HEIGHT - self.height),
        ]
    }

    /// Check the full standing footprint before materializing beside a vehicle.
    pub fn has_clear_body(&self, world: &impl CollisionQuery) -> bool {
        self.body_clearance(world, RADIUS + SKIN)
    }
    pub(crate) fn body_clearance(&self, world: &impl CollisionQuery, radius: f32) -> bool {
        let world = &ActorQuery(world);
        for z in [
            0.08,
            self.height * 0.22,
            self.height * 0.43,
            self.height * 0.65,
            self.height * 0.86,
            self.height,
        ] {
            for i in 0..16 {
                let angle = i as f32 * std::f32::consts::TAU / 16.0;
                let (s, c) = angle.sin_cos();
                if world
                    .raycast(
                        [self.feet[0], self.feet[1], self.feet[2] + z],
                        [s, c, 0.0],
                        radius,
                    )
                    .is_some()
                {
                    return false;
                }
            }
        }
        clear_rise(world, self.feet, SKIN, self.height) >= SKIN
    }
    pub fn crouch_amount(&self) -> f32 {
        ((HEIGHT - self.height) / 0.62).clamp(0., 1.)
    }
    pub fn can_stand(&self, world: &impl CollisionQuery) -> bool {
        clear_rise(
            &ActorQuery(world),
            self.feet,
            HEIGHT - self.height,
            self.height,
        ) + 1e-5
            >= HEIGHT - self.height
    }
    pub fn set_crouch(&mut self, world: &impl CollisionQuery, amount: f32) -> bool {
        let wanted = HEIGHT - 0.62 * amount.clamp(0., 1.);
        if wanted > self.height
            && clear_rise(
                &ActorQuery(world),
                self.feet,
                wanted - self.height,
                self.height,
            ) + 1e-5
                < wanted - self.height
        {
            return false;
        }
        self.height = wanted;
        true
    }
    /// Swept movement of a modern ledge transition. Fails before crossing any
    /// obstruction, including newly arriving cars/pedestrians.
    pub fn traverse_to(&mut self, world: &impl CollisionQuery, target: [f32; 3]) -> bool {
        let world = &ActorQuery(world);
        let original = self.clone();
        let delta = [target[0] - self.feet[0], target[1] - self.feet[1]];
        if target[2] > self.feet[2] {
            let rise = target[2] - self.feet[2];
            if clear_rise(world, self.feet, rise, self.height) + 1e-4 < rise {
                return false;
            }
            self.feet[2] = target[2];
        }
        let d = delta[0].hypot(delta[1]);
        if d > 1e-6 {
            self.move_horizontal(world, delta.map(|x| x / d), d, 1., false);
        }
        if (self.feet[0] - target[0]).hypot(self.feet[1] - target[1]) > 0.005 {
            *self = original;
            return false;
        }
        if target[2] < self.feet[2] {
            let max = self.feet[2] - target[2] + SKIN;
            if let Some(z) = support(
                world,
                [self.feet[0], self.feet[1], self.feet[2] + SKIN],
                max,
            ) {
                if z > target[2] + 0.015 {
                    *self = original;
                    return false;
                }
            }
        }
        self.feet = target;
        self.velocity = 0.;
        self.grounded = false;
        true
    }

    pub fn step(
        &mut self,
        world: &impl CollisionQuery,
        axis: [f32; 2],
        speed: f32,
        jump: bool,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 || !speed.is_finite() || axis.iter().any(|v| !v.is_finite())
        {
            return;
        }
        // Protect direct callers and stalled frames as well as fixed viewer ticks.
        let dt = dt.min(0.25);
        let ticks = (dt / (1.0 / 120.0)).ceil().max(1.0) as usize;
        let world = &ActorQuery(world);
        for tick in 0..ticks {
            self.tick(world, axis, speed, jump && tick == 0, dt / ticks as f32);
        }
    }
    fn tick(
        &mut self,
        world: &impl CollisionQuery,
        axis: [f32; 2],
        speed: f32,
        jump: bool,
        dt: f32,
    ) {
        if jump && self.grounded {
            self.velocity = 6.0;
            self.grounded = false;
        }
        self.move_horizontal(world, axis, speed, dt, self.grounded);
        self.velocity -= 18.0 * dt;
        self.move_vertical(world, dt);
    }
    fn move_horizontal(
        &mut self,
        world: &impl CollisionQuery,
        axis: [f32; 2],
        speed: f32,
        dt: f32,
        can_step: bool,
    ) {
        let length = (axis[0] * axis[0] + axis[1] * axis[1]).sqrt().max(1.0);
        let delta = [axis[0] / length * speed * dt, axis[1] / length * speed * dt];
        // Grounded movement can rise onto a verified low step under the footprint.
        // Resolve each horizontal axis separately for wall sliding.
        for axis in 0..2 {
            if delta[axis].abs() < 1e-7 {
                continue;
            }
            let mut dir = [0.0; 3];
            dir[axis] = delta[axis].signum();
            let mut proposed = self.feet;
            proposed[axis] += delta[axis];
            let mut rise = 0.0;
            if can_step {
                let origin = [proposed[0], proposed[1], self.feet[2] + STEP + SKIN];
                if let Some(floor) = support(world, origin, STEP + SKIN + 0.12) {
                    let candidate = floor - self.feet[2];
                    if candidate > 1e-5 && candidate <= STEP + 1e-5 {
                        // A downward probe can hit the bottom of a closed car
                        // box. Its underside is not a step: the body above that
                        // contact would intersect the car's roof/seat volume.
                        let raised = [proposed[0], proposed[1], self.feet[2] + candidate];
                        // Validate both ends before lifting into a ceiling/overhang.
                        if clear_rise(world, self.feet, candidate, self.height) + 1e-5 < candidate
                            || clear_rise(world, proposed, candidate, self.height) + 1e-5
                                < candidate
                            || !standing_clear(world, raised, self.height)
                        {
                            continue;
                        }
                        rise = candidate;
                    }
                }
            }
            let side = 1 - axis;
            let mut allowed = delta[axis].abs();
            let mut rays = Vec::with_capacity(78);
            let mut reaches = Vec::with_capacity(78);
            for z in [
                0.05,
                STEP + 0.05,
                self.height * 0.4,
                self.height * 0.6,
                self.height * 0.8,
                self.height,
            ] {
                // Parallel slices of a circular body, every 5 cm. The former
                // three full-radius rays both missed slim posts between them
                // and behaved like a wide square against diagonal curbs.
                for slice in 0..=12 {
                    let offset = -RADIUS + 2.0 * RADIUS * slice as f32 / 12.0;
                    let reach = (RADIUS * RADIUS - offset * offset).max(0.0).sqrt();
                    let mut origin = [self.feet[0], self.feet[1], self.feet[2] + rise + z];
                    origin[side] += offset;
                    rays.push(Ray {
                        origin,
                        direction: dir,
                        distance: allowed + reach,
                    });
                    reaches.push(reach);
                }
            }
            for (hit, reach) in world.raycast_batch(&rays).into_iter().zip(reaches) {
                if let Some(hit) = hit {
                    // Grounded ramps are resolved by support; airborne bodies
                    // still collide with them. Batch queries preserve the rays.
                    if !can_step || hit.normal[2].abs() < WALKABLE {
                        allowed = allowed.min((hit.distance - reach - 0.01).max(0.));
                    }
                }
            }
            if allowed > 1e-7 {
                self.feet[axis] += allowed * dir[axis];
                self.feet[2] += rise;
            }
        }
    }

    fn move_vertical(&mut self, world: &impl CollisionQuery, dt: f32) {
        let previous_z = self.feet[2];
        let mut next_z = previous_z + self.velocity * dt;
        if next_z > previous_z {
            let wanted = next_z - previous_z;
            let allowed = clear_rise(world, self.feet, wanted, self.height);
            if allowed + 1e-6 < wanted {
                self.velocity = 0.0;
            }
            next_z = previous_z + allowed;
            self.grounded = false;
        } else {
            // A safe horizontal step was already applied. A falling body sweeps
            // from its previous feet and cannot snap upward by STEP onto a ledge.
            let max = (previous_z - next_z).max(0.0) + SKIN * 2.0 + 0.12;
            if let Some(floor) =
                support(world, [self.feet[0], self.feet[1], previous_z + SKIN], max)
            {
                if next_z <= floor && floor <= previous_z + SKIN + 1e-5 {
                    next_z = floor;
                    self.velocity = 0.0;
                    self.grounded = true;
                } else {
                    self.grounded = false;
                }
            } else {
                self.grounded = false;
            }
        }
        self.feet[2] = next_z;
    }

    /// Surface swimming uses the same swept body/ceiling/terrain contacts as
    /// walking. Spring and drag are provisional PC parameters, not decoded VCS.
    pub fn swim_step(
        &mut self,
        world: &impl CollisionQuery,
        velocity: [f32; 2],
        water_height: f32,
        immersion: f32,
        dt: f32,
    ) {
        if !dt.is_finite()
            || dt <= 0.
            || !water_height.is_finite()
            || !immersion.is_finite()
            || immersion <= 0.
            || velocity.iter().any(|v| !v.is_finite())
        {
            return;
        }
        let dt = dt.min(1. / 60.);
        let world = &ActorQuery(world);
        let speed = velocity[0].hypot(velocity[1]);
        let axis = if speed > 1e-6 {
            velocity.map(|v| v / speed)
        } else {
            [0.; 2]
        };
        self.move_horizontal(world, axis, speed, dt, true);
        let target = water_height - immersion;
        // Limit entry speed to avoid a one-frame leap out of deep water.
        self.velocity = (self.velocity
            + ((target - self.feet[2]) * 28. - self.velocity * 10.) * dt)
            .clamp(-4., 4.);
        self.move_vertical(world, dt);
    }

    pub fn from_camera_with_water(
        world: &impl CollisionQuery,
        camera: &Camera,
        water: Option<&crate::water::WaterGrid>,
    ) -> Option<Self> {
        let walking = Self::from_camera(world, camera);
        let height = water.and_then(|w| w.height(camera.pos));
        if let Some(z) = height {
            if walking
                .as_ref()
                .is_none_or(|w| w.feet[2] < z - crate::locomotion::SWIM_DEPTH)
            {
                let swimmer = Self {
                    feet: [
                        camera.pos[0],
                        camera.pos[1],
                        z - crate::locomotion::SWIM_DEPTH,
                    ],
                    velocity: 0.,
                    height: HEIGHT,
                    grounded: false,
                };
                if swimmer.has_clear_body(world) {
                    return Some(swimmer);
                }
            }
        }
        walking
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::{CollisionWorld, Triangle};

    #[test]
    #[ignore = "Requires the user's local VCS PS2 PAL ISO"]
    fn native_street_props_and_low_curbs_block_and_step_correctly() {
        use crate::iso::{f32le, u16le, u32le};
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        crate::scene::select_collisions(&mut world, 0);
        let pool = u32le(&game, 0x2c).unwrap() as usize;
        let items = u32le(&game, pool).unwrap() as usize;
        let flags = u32le(&game, pool + 4).unwrap() as usize;
        let count = u32le(&game, pool + 8).unwrap() as usize;
        let mut checked = std::collections::BTreeMap::<u16, usize>::new();
        for i in 0..count {
            let row = items + i * 96;
            let model = u16le(&game, row + 0x56).unwrap();
            if game[flags + i] & 128 != 0
                || !matches!(model, 451 | 457 | 458 | 513 | 516 | 524 | 528)
                || checked.get(&model).copied().unwrap_or(0) >= 4
            {
                continue;
            }
            let p = [
                f32le(&game, row + 48).unwrap(),
                f32le(&game, row + 52).unwrap(),
                f32le(&game, row + 56).unwrap(),
            ];
            for a in 0..8 {
                let (s, c) = (a as f32 * std::f32::consts::TAU / 8.0).sin_cos();
                let camera = Camera {
                    pos: [p[0] + s * 4.0, p[1] + c * 4.0, p[2] + 3.0],
                    yaw: 0.0,
                    pitch: 0.0,
                };
                let Some(mut walker) = Walker::from_camera(&world, &camera) else {
                    continue;
                };
                if !walker.has_clear_body(&world) {
                    continue;
                }
                let start = walker.feet;
                let Some(hit) =
                    world.raycast([start[0], start[1], start[2] + 0.65], [-s, -c, 0.0], 6.0)
                else {
                    continue;
                };
                if hit.model_id != model || hit.distance < 1.0 {
                    continue;
                }
                for _ in 0..360 {
                    let before = walker.feet;
                    walker.step(&world, [-s, -c], 1.8, false, 1.0 / 120.0);
                    let delta: [f32; 3] = std::array::from_fn(|a| walker.feet[a] - before[a]);
                    let length = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
                    if length > 0.00001 {
                        // Sliding around an offset pole is valid. Check every
                        // movement segment instead of assuming a straight path.
                        for height in [0.20, 0.65, 1.2] {
                            let crossing = world.raycast(
                                [before[0], before[1], before[2] + height],
                                delta,
                                length,
                            );
                            assert!(
                                !crossing.is_some_and(|h| h.model_id == model),
                                "crossed model={model}: from={before:?} to={:?}",
                                walker.feet
                            );
                        }
                    }
                }
                assert!(walker.grounded);
                *checked.entry(model).or_default() += 1;
                if checked[&model] >= 4 {
                    break;
                }
            }
        }
        println!("Native furniture blocking checks: {checked:?}");
        assert_eq!(checked.len(), 7);
        let mut cells = std::collections::HashSet::new();
        let mut curbs = 0;
        for tri in &world.triangles {
            // Small props include beveled lamp/telegraph-pole bases. They are
            // obstacles, not sidewalk candidates; their checks are above.
            if tri.model_id < 1000 {
                continue;
            }
            let p = tri.vertices;
            let low = p.iter().map(|v| v[2]).fold(f32::INFINITY, f32::min);
            let high = p.iter().map(|v| v[2]).fold(f32::NEG_INFINITY, f32::max);
            if low < 7.0 || !(0.10..0.30).contains(&(high - low)) {
                continue;
            }
            let e: [f32; 3] = std::array::from_fn(|a| p[1][a] - p[0][a]);
            let f: [f32; 3] = std::array::from_fn(|a| p[2][a] - p[0][a]);
            let n = [
                e[1] * f[2] - e[2] * f[1],
                e[2] * f[0] - e[0] * f[2],
                e[0] * f[1] - e[1] * f[0],
            ];
            let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
            if len < 0.01 || n[2].abs() > len * 0.05 {
                continue;
            }
            let center: [f32; 3] = std::array::from_fn(|a| (p[0][a] + p[1][a] + p[2][a]) / 3.0);
            let key = [
                (center[0] / 32.0).floor() as i32,
                (center[1] / 32.0).floor() as i32,
            ];
            if cells.contains(&key) {
                continue;
            }
            let direction = [n[0] / len, n[1] / len];
            let mut sides = Vec::new();
            for sign in [-1.0, 1.0] {
                let camera = Camera {
                    pos: [
                        center[0] + direction[0] * 0.8 * sign,
                        center[1] + direction[1] * 0.8 * sign,
                        high + 1.0,
                    ],
                    yaw: 0.0,
                    pitch: 0.0,
                };
                if let Some(w) = Walker::from_camera(&world, &camera) {
                    sides.push(w);
                }
            }
            if sides.len() != 2 {
                continue;
            }
            if sides[0].feet[2] > sides[1].feet[2] {
                sides.swap(0, 1);
            }
            let rise = sides[1].feet[2] - sides[0].feet[2];
            if !(0.08..0.30).contains(&rise) || sides.iter().any(|w| !w.has_clear_body(&world)) {
                continue;
            }
            let mut walker = sides.remove(0);
            let start = walker.feet;
            let target = sides[0].feet;
            let dir = [
                (target[0] - start[0]) / 1.6,
                (target[1] - start[1]) / 1.6,
                0.0,
            ];
            // Require an unobstructed body-width corridor above the curb.
            // A beveled building edge beside a wall is not a crossable sidewalk.
            if [-RADIUS, 0.0, RADIUS].iter().any(|side| {
                [rise + 0.05, STEP + 0.05, 0.65, 1.4, HEIGHT]
                    .iter()
                    .any(|z| {
                        world
                            .raycast(
                                [
                                    start[0] - dir[1] * side,
                                    start[1] + dir[0] * side,
                                    start[2] + z,
                                ],
                                dir,
                                1.6,
                            )
                            .is_some()
                    })
            }) {
                continue;
            }
            for _ in 0..100 {
                walker.step(&world, [dir[0], dir[1]], 1.8, false, 1.0 / 120.0);
            }
            let travel =
                (walker.feet[0] - start[0]) * dir[0] + (walker.feet[1] - start[1]) * dir[1];
            assert!(
                travel > 1.1 && walker.grounded,
                "curb model={} start={start:?}, target={target:?}, end={:?}",
                tri.model_id,
                walker.feet
            );
            if curbs < 3 {
                println!(
                    "Native curb: from={start:?} to={:?}, rise={rise}",
                    walker.feet
                );
            }
            cells.insert(key);
            curbs += 1;
            if curbs >= 80 {
                break;
            }
        }
        println!("Native curb crossings: {curbs}");
        assert!(curbs >= 20);
    }

    #[test]
    fn player_cannot_run_through_car_from_any_side_or_rotation() {
        use crate::collision::{test_box_collider, CollisionScene};
        let world = fixture(false, false);
        let collider = test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]);
        for heading in [0.0f32, 0.73, 2.6] {
            let (s, c) = heading.sin_cos();
            let instances = [collider.at([0.0; 3], heading)];
            let scene = CollisionScene {
                world: &world,
                instances: &instances,
            };
            for local in [[-6.0, 0.0], [6.0, 0.0], [0.0, -6.0], [0.0, 6.0]] {
                let point = [c * local[0] + s * local[1], -s * local[0] + c * local[1]];
                let mut walker = Walker {
                    feet: [point[0], point[1], SKIN],
                    velocity: 0.0,
                    height: HEIGHT,
                    grounded: true,
                };
                for _ in 0..40 {
                    walker.step(&scene, [-point[0], -point[1]], 4.5, false, 0.1);
                }
                let x = c * walker.feet[0] - s * walker.feet[1];
                let y = s * walker.feet[0] + c * walker.feet[1];
                assert!(
                    x.abs() >= 1.20 || y <= -2.9 || y >= 3.1,
                    "{heading}: {:?}",
                    walker.feet
                );
                assert!((walker.feet[2] - SKIN).abs() < 0.001);
            }
        }
    }

    #[test]
    fn player_can_slide_along_car_and_walk_away() {
        use crate::collision::{test_box_collider, CollisionScene};
        let world = fixture(false, false);
        let collider = test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]);
        let instances = [collider.at([0.0; 3], 0.0)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let mut walker = Walker {
            feet: [1.35, -1.5, SKIN],
            velocity: 0.0,
            height: HEIGHT,
            grounded: true,
        };
        for _ in 0..120 {
            walker.step(&scene, [-1.0, 1.0], 4.5, false, 1.0 / 120.0);
        }
        assert!(
            walker.feet[0] > 1.29 && walker.feet[1] > 1.0,
            "{:?}",
            walker.feet
        );
        for _ in 0..120 {
            walker.step(&scene, [1.0, 0.0], 4.5, false, 1.0 / 120.0);
        }
        assert!(walker.feet[0] > 5.0);
    }

    #[test]
    fn falling_player_lands_on_car_and_can_step_off_it() {
        use crate::collision::{test_box_collider, CollisionScene};
        let world = fixture(false, false);
        let collider = test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]);
        let instances = [collider.at([0.0; 3], 0.0)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let mut walker = Walker {
            feet: [0.0, 0.0, 3.0],
            velocity: 0.0,
            height: HEIGHT,
            grounded: false,
        };
        for _ in 0..120 {
            walker.step(&scene, [0.0; 2], 4.5, false, 1.0 / 120.0);
        }
        assert!((walker.feet[2] - 1.32).abs() < 0.001 && walker.grounded);
        for _ in 0..180 {
            walker.step(&scene, [1.0, 0.0], 4.5, false, 1.0 / 120.0);
        }
        assert!((walker.feet[2] - SKIN).abs() < 0.001 && walker.grounded);
    }

    #[test]
    fn car_on_other_floor_does_not_block_player() {
        use crate::collision::{test_box_collider, CollisionScene};
        let world = fixture(false, false);
        let collider = test_box_collider([[-1.0, -2.7, 0.15], [1.0, 2.9, 1.3]]);
        let instances = [collider.at([0.0, 0.0, 5.0], 0.0)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let mut walker = grounded();
        for _ in 0..120 {
            walker.step(&scene, [1.0, 0.0], 4.5, false, 1.0 / 120.0);
        }
        assert!(walker.feet[0] > 4.0 && walker.grounded);
    }
    fn triangle(vertices: [[f32; 3]; 3]) -> Triangle {
        Triangle {
            vertices,
            surface: 0,
            model_id: 1,
        }
    }
    fn fixture(wall: bool, ceiling: bool) -> CollisionWorld {
        let mut t = vec![
            triangle([[-20.0, -20.0, 0.0], [20.0, -20.0, 0.0], [20.0, 20.0, 0.0]]),
            triangle([[-20.0, -20.0, 0.0], [20.0, 20.0, 0.0], [-20.0, 20.0, 0.0]]),
        ];
        if wall {
            t.extend([
                triangle([[2.0, -20.0, 0.0], [2.0, 20.0, 0.0], [2.0, 20.0, 5.0]]),
                triangle([[2.0, -20.0, 0.0], [2.0, 20.0, 5.0], [2.0, -20.0, 5.0]]),
            ]);
        }
        if ceiling {
            t.extend([
                triangle([[-20.0, -20.0, 2.5], [20.0, -20.0, 2.5], [20.0, 20.0, 2.5]]),
                triangle([[-20.0, -20.0, 2.5], [20.0, 20.0, 2.5], [-20.0, 20.0, 2.5]]),
            ]);
        }
        CollisionWorld::from_triangles(t)
    }
    #[test]
    fn ground_contact_and_wall_prevent_tunneling() {
        let world = fixture(true, false);
        let camera = Camera {
            pos: [0.0, 0.0, 10.0],
            yaw: 0.0,
            pitch: 0.0,
        };
        let mut walker = Walker::from_camera(&world, &camera).unwrap();
        for _ in 0..120 {
            walker.step(&world, [1.0, 0.0], 8.0, false, 1.0 / 60.0);
        }
        assert!(
            walker.feet[0] > 1.5 && walker.feet[0] < 1.71,
            "{:?}",
            walker.feet
        );
        assert!((walker.feet[2] - 0.02).abs() < 0.001);
        assert!(walker.grounded);
    }
    #[test]
    fn falling_lands_on_native_surface() {
        let world = fixture(false, false);
        let mut walker = Walker {
            feet: [0.0, 0.0, 4.0],
            velocity: 0.0,
            height: HEIGHT,
            grounded: false,
        };
        for _ in 0..100 {
            walker.step(&world, [0.0; 2], 4.0, false, 1.0 / 60.0);
        }
        assert!(walker.grounded);
        assert!((walker.feet[2] - 0.02).abs() < 0.001);
    }
    #[test]
    fn jump_is_stopped_by_ceiling() {
        let world = fixture(false, true);
        let mut walker = Walker {
            feet: [0.0, 0.0, 0.02],
            velocity: 0.0,
            height: HEIGHT,
            grounded: true,
        };
        for tick in 0..100 {
            walker.step(&world, [0.0; 2], 4.0, tick == 0, 1.0 / 60.0);
            assert!(walker.eye()[2] < 2.50, "{:?}", walker.eye());
        }
        assert!(walker.grounded);
    }
    fn step_fixture(height: f32, ceiling: Option<f32>) -> CollisionWorld {
        let mut t = fixture(false, false).triangles;
        t.extend([
            triangle([[1.0, -20.0, 0.0], [1.0, 20.0, 0.0], [1.0, 20.0, height]]),
            triangle([[1.0, -20.0, 0.0], [1.0, 20.0, height], [1.0, -20.0, height]]),
            triangle([
                [1.0, -20.0, height],
                [20.0, -20.0, height],
                [20.0, 20.0, height],
            ]),
            triangle([
                [1.0, -20.0, height],
                [20.0, 20.0, height],
                [1.0, 20.0, height],
            ]),
        ]);
        if let Some(z) = ceiling {
            t.extend([
                triangle([[-20.0, -20.0, z], [20.0, -20.0, z], [20.0, 20.0, z]]),
                triangle([[-20.0, -20.0, z], [20.0, 20.0, z], [-20.0, 20.0, z]]),
            ]);
        }
        CollisionWorld::from_triangles(t)
    }
    fn grounded() -> Walker {
        Walker {
            feet: [0.0, 0.0, SKIN],
            velocity: 0.0,
            height: HEIGHT,
            grounded: true,
        }
    }
    #[test]
    fn curb_just_above_step_height_blocks_without_tunneling() {
        let world = step_fixture(STEP + 0.01, None);
        let mut walker = grounded();
        for _ in 0..80 {
            walker.step(&world, [1.0, 0.0], 4.0, false, 1.0 / 60.0);
        }
        assert!(walker.feet[0] <= 1.0 - RADIUS, "{:?}", walker.feet);
        assert!((walker.feet[2] - SKIN).abs() < 0.001);
    }
    #[test]
    fn low_step_is_climbed_when_headroom_is_clear() {
        let world = step_fixture(0.30, None);
        let mut walker = grounded();
        for _ in 0..50 {
            walker.step(&world, [1.0, 0.0], 4.0, false, 1.0 / 60.0);
        }
        assert!(walker.feet[0] > 2.0, "{:?}", walker.feet);
        assert!(
            (walker.feet[2] - 0.30 - SKIN).abs() < 0.001,
            "{:?}",
            walker.feet
        );
        assert!(walker.grounded);
    }

    #[test]
    fn thin_lamppost_between_old_body_probes_stops_player() {
        use crate::collision::{test_box_collider, CollisionScene};
        let floor = fixture(false, false);
        for x in [0.075, 0.15, 0.225, -0.15] {
            let pole = test_box_collider([[x - 0.04, 1.0, 0.0], [x + 0.04, 1.12, 3.0]]);
            let bodies = [pole.at([0.0; 3], 0.0)];
            let world = CollisionScene {
                world: &floor,
                instances: &bodies,
            };
            let mut walker = grounded();
            for _ in 0..120 {
                walker.step(&world, [0.0, 1.0], 4.5, false, 1.0 / 60.0);
            }
            assert!(walker.feet[1] < 1.0, "pole at x={x}: {:?}", walker.feet);
        }
    }

    #[test]
    fn low_curbs_can_be_crossed_at_diagonal_angles() {
        for angle in [0.0f32, 0.2, 0.5, 0.7853982, 1.2, 1.5707963] {
            let (s, c) = angle.sin_cos();
            let world = CollisionWorld::from_triangles(
                step_fixture(0.20, None)
                    .triangles
                    .into_iter()
                    .map(|mut t| {
                        t.vertices = t
                            .vertices
                            .map(|p| [c * p[0] - s * p[1], s * p[0] + c * p[1], p[2]]);
                        t
                    })
                    .collect(),
            );
            let mut walker = grounded();
            for _ in 0..90 {
                walker.step(&world, [c, s], 1.8, false, 1.0 / 60.0);
            }
            assert!(
                walker.feet[0] * c + walker.feet[1] * s > 2.0,
                "angle={angle}: {:?}",
                walker.feet
            );
            assert!((walker.feet[2] - 0.20 - SKIN).abs() < 0.001 && walker.grounded);
        }
    }
    #[test]
    fn step_with_blocked_headroom_cannot_enter_ceiling() {
        let ceiling = HEIGHT + SKIN + 0.08;
        let world = step_fixture(0.30, Some(ceiling));
        let mut walker = grounded();
        assert!(walker.feet[2] + HEIGHT < ceiling);
        for _ in 0..50 {
            walker.step(&world, [1.0, 0.0], 4.0, false, 1.0 / 60.0);
            assert!(walker.feet[2] + HEIGHT < ceiling, "{:?}", walker.feet);
        }
        assert!(walker.feet[0] < 1.0 - RADIUS, "{:?}", walker.feet);
        assert!(walker.grounded);
    }
    #[test]
    fn airborne_body_does_not_snap_onto_platform_above_feet() {
        let mut t = fixture(false, false).triangles;
        t.extend([
            triangle([
                [-20.0, -20.0, 0.30],
                [20.0, -20.0, 0.30],
                [20.0, 20.0, 0.30],
            ]),
            triangle([
                [-20.0, -20.0, 0.30],
                [20.0, 20.0, 0.30],
                [-20.0, 20.0, 0.30],
            ]),
        ]);
        let world = CollisionWorld::from_triangles(t);
        let mut walker = Walker {
            feet: [0.0, 0.0, 0.10],
            velocity: -1.0,
            height: HEIGHT,
            grounded: false,
        };
        walker.step(&world, [0.0; 2], 4.0, false, 1.0 / 120.0);
        assert!(walker.feet[2] < 0.10, "{:?}", walker.feet);
        assert!(!walker.grounded);
    }
    #[test]
    fn frame_hitch_does_not_put_feet_inside_walkable_ramp() {
        let world = CollisionWorld::from_triangles(vec![
            triangle([
                [-20.0, -20.0, -20.0],
                [20.0, -20.0, 20.0],
                [20.0, 20.0, 20.0],
            ]),
            triangle([
                [-20.0, -20.0, -20.0],
                [20.0, 20.0, 20.0],
                [-20.0, 20.0, -20.0],
            ]),
        ]);
        let camera = Camera {
            pos: [0.0, 0.0, 10.0],
            yaw: 0.0,
            pitch: 0.0,
        };
        let mut walker = Walker::from_camera(&world, &camera).unwrap();
        walker.step(&world, [1.0, 0.0], 8.0, false, 0.10);
        assert!(walker.feet[0] > 0.70, "{:?}", walker.feet);
        assert!(
            walker.feet[2] >= walker.feet[0] + SKIN - 0.001,
            "{:?}",
            walker.feet
        );
        assert!(walker.grounded);
    }
    #[test]
    fn walking_spawn_rejects_gap_too_low_to_stand_in() {
        let world = step_fixture(0.30, Some(1.20));
        let camera = Camera {
            pos: [0.0, 0.0, 1.0],
            yaw: 0.0,
            pitch: 0.0,
        };
        assert!(Walker::from_camera(&world, &camera).is_none());
    }
}
