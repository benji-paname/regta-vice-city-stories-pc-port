//! Experimental first-person movement on original static PS2 collision surfaces.
//! This is a new controller, not a claim to reproduce VCS pedestrian physics.
//! Discrete body and footprint rays approximate a capsule. Thin obstacles
//! between the body probes and curved edge contacts remain limitations.
use crate::collision::CollisionWorld;
use crate::render::Camera;

const EYE: f32 = 1.65;
const RADIUS: f32 = 0.30;
const STEP: f32 = 0.35;
const HEIGHT: f32 = 1.85; // Native player's bind height is 1.806 m, plus head clearance.
const SKIN: f32 = 0.02;
const WALKABLE: f32 = 0.70;
const FOOTPRINT: [[f32; 2]; 5] = [
    [0.0, 0.0],
    [RADIUS, 0.0],
    [-RADIUS, 0.0],
    [0.0, RADIUS],
    [0.0, -RADIUS],
];

fn support(world: &CollisionWorld, position: [f32; 3], distance: f32) -> Option<f32> {
    FOOTPRINT
        .iter()
        .filter_map(|offset| {
            let origin = [
                position[0] + offset[0],
                position[1] + offset[1],
                position[2],
            ];
            let hit = world.raycast(origin, [0.0, 0.0, -1.0], distance)?;
            (hit.normal[2] >= WALKABLE).then_some(hit.position[2] + SKIN)
        })
        .max_by(f32::total_cmp)
}
fn clear_rise(world: &CollisionWorld, feet: [f32; 3], rise: f32) -> f32 {
    let mut allowed = rise;
    for offset in FOOTPRINT {
        let origin = [feet[0] + offset[0], feet[1] + offset[1], feet[2] + HEIGHT];
        if let Some(hit) = world.raycast(origin, [0.0, 0.0, 1.0], rise + SKIN) {
            allowed = allowed.min((hit.distance - SKIN).max(0.0));
        }
    }
    allowed
}

pub struct Walker {
    pub feet: [f32; 3],
    velocity: f32,
    pub grounded: bool,
}
impl Walker {
    pub fn from_camera(world: &CollisionWorld, camera: &Camera) -> Option<Self> {
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
            grounded: true,
        })
    }
    pub fn eye(&self) -> [f32; 3] {
        [self.feet[0], self.feet[1], self.feet[2] + EYE]
    }
    pub fn step(
        &mut self,
        world: &CollisionWorld,
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
        for tick in 0..ticks {
            self.tick(world, axis, speed, jump && tick == 0, dt / ticks as f32);
        }
    }
    fn tick(&mut self, world: &CollisionWorld, axis: [f32; 2], speed: f32, jump: bool, dt: f32) {
        if jump && self.grounded {
            self.velocity = 6.0;
            self.grounded = false;
        }
        let can_step = self.grounded;
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
                        // Validate both ends before lifting into a ceiling/overhang.
                        if clear_rise(world, self.feet, candidate) + 1e-5 < candidate
                            || clear_rise(world, proposed, candidate) + 1e-5 < candidate
                        {
                            continue;
                        }
                        rise = candidate;
                    }
                }
            }
            let side = 1 - axis;
            let mut allowed = delta[axis].abs();
            for z in [0.05, STEP + 0.05, 0.75, 1.10, 1.45, HEIGHT] {
                for offset in [-RADIUS, 0.0, RADIUS] {
                    let mut origin = [self.feet[0], self.feet[1], self.feet[2] + rise + z];
                    origin[side] += offset;
                    if let Some(hit) = world.raycast(origin, dir, allowed + RADIUS) {
                        // Grounded walkable ramps are covered by the support query;
                        // an airborne body cannot pass through those same surfaces.
                        if !can_step || hit.normal[2].abs() < WALKABLE {
                            allowed = allowed.min((hit.distance - RADIUS - 0.01).max(0.0));
                        }
                    }
                }
            }
            if allowed > 1e-7 {
                self.feet[axis] += allowed * dir[axis];
                self.feet[2] += rise;
            }
        }
        self.velocity -= 18.0 * dt;
        let previous_z = self.feet[2];
        let mut next_z = previous_z + self.velocity * dt;
        if next_z > previous_z {
            let wanted = next_z - previous_z;
            let allowed = clear_rise(world, self.feet, wanted);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collision::Triangle;
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
