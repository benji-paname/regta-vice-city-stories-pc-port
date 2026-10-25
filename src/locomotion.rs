//! Original jump/fall clips driven by actual controller contacts. Native clip
//! durations determine launch/landing phases; gravity/impulse remain those of
//! the provisional walker. The 80ms transition blend is a PC approximation.
use crate::character::{Character, Pose};
use crate::collision::CollisionQuery;
use crate::iso::Result;
use crate::player::Walker;
use crate::water::WaterGrid;

const BLEND: f32 = 0.08;
const STEP: f32 = 1. / 120.;
// Provisional body immersion and water drag. Native water levels, animation
// durations and stroke displacement are read from GAME; these forces are not.
pub const SWIM_DEPTH: f32 = 0.9;
const SWIM_IDLE_DEPTH: f32 = 1.25;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Ground,
    Launch,
    Air,
    Fall,
    Land,
    FallLand,
    SwimIdle,
    SwimStart,
    Swim,
    SwimFast,
    SwimStop,
}

pub struct Locomotion {
    phase: Phase,
    clip: &'static str,
    clock: f32,
    previous: Option<Pose>,
    blend_clock: f32,
    launch: f32,
    landing: f32,
    fall_landing: f32,
    swim_start: f32,
    swim_stop: f32,
    swim_speeds: [f32; 2],
    swim_velocity: [f32; 2],
}

impl Locomotion {
    pub fn new(character: &Character) -> Result<Self> {
        let duration = |name| {
            character
                .duration(name)
                .ok_or_else(|| format!("Missing native locomotion clip {name}"))
        };
        let speed = |name| {
            character
                .stats
                .clips
                .iter()
                .find(|c| c.name == name)
                .map(|c| c.root_planar_speed)
                .filter(|v| v.is_finite() && *v > 0.)
                .ok_or_else(|| format!("Missing native swimming root speed {name}"))
        };
        Ok(Self::with_durations(
            duration("JUMP_launch")?,
            duration("JUMP_land")?,
            duration("FALL_land")?,
            duration("swim_start")?,
            duration("swim_to_idle")?,
            [speed("swim")?, speed("swim_fast")?],
        ))
    }

    fn with_durations(
        launch: f32,
        landing: f32,
        fall_landing: f32,
        swim_start: f32,
        swim_stop: f32,
        swim_speeds: [f32; 2],
    ) -> Self {
        Self {
            phase: Phase::Ground,
            clip: "idle",
            clock: 0.,
            previous: None,
            blend_clock: BLEND,
            launch,
            landing,
            fall_landing,
            swim_start,
            swim_stop,
            swim_speeds,
            swim_velocity: [0.; 2],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::with_durations(
            self.launch,
            self.landing,
            self.fall_landing,
            self.swim_start,
            self.swim_stop,
            self.swim_speeds,
        );
    }

    pub fn swimming(&self) -> bool {
        matches!(
            self.phase,
            Phase::SwimIdle | Phase::SwimStart | Phase::Swim | Phase::SwimFast | Phase::SwimStop
        )
    }

    pub fn pose(&self) -> Pose {
        Pose {
            clip: self.clip,
            time: self.clock,
            looping: matches!(
                self.phase,
                Phase::Ground | Phase::Fall | Phase::SwimIdle | Phase::Swim | Phase::SwimFast
            ),
            root_height: self.phase != Phase::Ground,
        }
    }

    pub fn blend(&self) -> Option<(Pose, f32)> {
        self.previous
            .filter(|_| self.blend_clock < BLEND)
            .map(|p| (p, self.blend_clock / BLEND))
    }

    fn transition(&mut self, phase: Phase, clip: &'static str) {
        if self.phase == phase && self.clip == clip {
            return;
        }
        self.previous = Some(self.pose());
        self.blend_clock = 0.;
        self.clock = 0.;
        self.phase = phase;
        self.clip = clip;
    }

    pub fn step(
        &mut self,
        walker: &mut Walker,
        world: &impl CollisionQuery,
        water: Option<&WaterGrid>,
        axis: [f32; 2],
        speed: f32,
        jump: bool,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0. || !speed.is_finite() || axis.iter().any(|v| !v.is_finite())
        {
            return;
        }
        let dt = dt.min(0.25);
        let ticks = (dt / STEP).ceil().max(1.) as usize;
        for tick in 0..ticks {
            let dt = dt / ticks as f32;
            let height = water.and_then(|w| w.height(walker.feet));
            let in_water = height.is_some_and(|z| walker.feet[2] < z - SWIM_DEPTH + 0.18);
            if self.swimming() || in_water {
                if let Some(height) =
                    height.filter(|z| !walker.grounded || walker.feet[2] < *z - SWIM_DEPTH + 0.25)
                {
                    self.swim(walker, world, axis, speed, height, dt);
                    continue;
                }
                self.swim_velocity = [0.; 2];
                if walker.grounded {
                    self.transition(Phase::Ground, "idle");
                } else {
                    self.transition(Phase::Fall, "FALL_glide");
                }
            }
            let grounded = walker.grounded;
            if jump && tick == 0 && grounded && self.phase == Phase::Ground {
                self.transition(Phase::Launch, "JUMP_launch");
            }
            self.clock += dt;
            self.blend_clock += dt;
            let impulse =
                self.phase == Phase::Launch && self.clock + 1e-6 >= self.launch && grounded;
            let before = walker.feet;
            walker.step(world, axis, speed, impulse, dt);
            if impulse {
                self.transition(Phase::Air, "JUMP_glide");
            } else if grounded && !walker.grounded {
                // A real ledge departure, not a requested jump. Curbs that keep
                // controller support do not trigger airborne/landing poses.
                self.transition(Phase::Fall, "FALL_glide");
            }
            if !grounded && walker.grounded {
                if self.phase == Phase::Air {
                    self.transition(Phase::Land, "JUMP_land");
                } else {
                    self.transition(Phase::FallLand, "FALL_land");
                }
            }
            if (self.phase == Phase::Land && self.clock >= self.landing)
                || (self.phase == Phase::FallLand && self.clock >= self.fall_landing)
            {
                self.transition(Phase::Ground, "idle");
            }
            if self.phase == Phase::Ground {
                if !walker.grounded {
                    self.transition(Phase::Fall, "FALL_glide");
                } else {
                    let moved =
                        (before[0] - walker.feet[0]).hypot(before[1] - walker.feet[1]) > 0.00001;
                    self.transition(
                        Phase::Ground,
                        if !moved {
                            "idle"
                        } else if speed > 2. {
                            "run"
                        } else {
                            "walk"
                        },
                    );
                }
            }
        }
    }

    fn swim(
        &mut self,
        walker: &mut Walker,
        world: &impl CollisionQuery,
        axis: [f32; 2],
        speed: f32,
        height: f32,
        dt: f32,
    ) {
        if !self.swimming() {
            self.transition(Phase::SwimIdle, "swim_idle");
        }
        let length = axis[0].hypot(axis[1]);
        let moving = length > 0.001 && speed > 0.;
        if moving {
            if matches!(self.phase, Phase::SwimIdle | Phase::SwimStop) {
                self.transition(Phase::SwimStart, "swim_start");
            } else if self.phase != Phase::SwimStart || self.clock >= self.swim_start {
                if speed > 2. {
                    self.transition(Phase::SwimFast, "swim_fast");
                } else {
                    self.transition(Phase::Swim, "swim");
                }
            }
        } else if matches!(self.phase, Phase::Swim | Phase::SwimFast | Phase::SwimStart) {
            self.transition(Phase::SwimStop, "swim_to_idle");
        } else if self.phase == Phase::SwimStop && self.clock >= self.swim_stop {
            self.transition(Phase::SwimIdle, "swim_idle");
        }
        self.clock += dt;
        self.blend_clock += dt;
        let stroke_speed = if moving {
            self.swim_speeds[(speed > 2.) as usize]
        } else {
            0.
        };
        let response = 1. - (-4. * dt).exp();
        for a in 0..2 {
            let target = axis[a] / length.max(1.) * stroke_speed;
            self.swim_velocity[a] += (target - self.swim_velocity[a]) * response;
        }
        walker.swim_step(world, self.swim_velocity, height, self.immersion(), dt);
    }

    fn immersion(&self) -> f32 {
        // Upright treading needs a lower body anchor than the horizontal stroke.
        // Follow the native transition durations to avoid a surface-height snap.
        let active = match self.phase {
            Phase::SwimIdle => 0.,
            Phase::SwimStart => (self.clock / self.swim_start).clamp(0., 1.),
            Phase::SwimStop => 1. - (self.clock / self.swim_stop).clamp(0., 1.),
            _ => 1.,
        };
        SWIM_IDLE_DEPTH + (SWIM_DEPTH - SWIM_IDLE_DEPTH) * active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        collision::{CollisionWorld, Triangle},
        render::Camera,
    };

    fn controller() -> Locomotion {
        Locomotion::with_durations(0.2, 0.2, 0.5666667, 0.5, 1.3333333, [1.368557, 2.856152])
    }
    fn triangle(v: [[f32; 3]; 3]) -> Triangle {
        Triangle {
            vertices: v,
            surface: 0,
            model_id: 1,
        }
    }
    fn floor(z: f32) -> CollisionWorld {
        CollisionWorld::from_triangles(vec![
            triangle([[-100., -100., z], [100., -100., z], [100., 100., z]]),
            triangle([[-100., -100., z], [100., 100., z], [-100., 100., z]]),
        ])
    }
    fn walker(world: &CollisionWorld, water: Option<&WaterGrid>) -> Walker {
        Walker::from_camera_with_water(
            world,
            &Camera {
                pos: [0., 0., 20.],
                yaw: 0.,
                pitch: 0.,
            },
            water,
        )
        .unwrap()
    }
    #[test]
    fn jump_launch_air_landing_follow_contacts_and_reject_air_jump() {
        let world = floor(0.);
        let mut a = walker(&world, None);
        let mut b = walker(&world, None);
        let mut ca = controller();
        let mut cb = controller();
        let mut air = false;
        let mut landing = false;
        for i in 0..180 {
            ca.step(&mut a, &world, None, [0.; 2], 0., i == 0 || i == 40, STEP);
            cb.step(&mut b, &world, None, [0.; 2], 0., i == 0, STEP);
            assert_eq!(a.feet, b.feet, "second jump changed airborne trajectory");
            if i == 11 {
                assert!(a.grounded);
                assert_eq!(ca.pose().clip, "JUMP_launch");
            }
            if !a.grounded {
                air = true;
                assert_eq!(ca.phase, Phase::Air);
            }
            if ca.phase == Phase::Land {
                landing = true;
                assert!(a.grounded);
            }
        }
        assert!(air && landing && a.grounded);
        assert_eq!(ca.phase, Phase::Ground);
    }
    #[test]
    fn fall_uses_fall_clips_and_water_disabled_in_interior() {
        let world = floor(0.);
        let mut w = walker(&world, None);
        let mut c = controller();
        w.feet[2] = 4.;
        w.grounded = false;
        c.step(&mut w, &world, None, [0.; 2], 0., false, STEP);
        assert_eq!(c.phase, Phase::Fall);
        let mut landed = false;
        for _ in 0..180 {
            c.step(&mut w, &world, None, [0.; 2], 0., false, STEP);
            landed |= c.phase == Phase::FallLand;
        }
        assert!(landed && w.grounded && !c.swimming());
        assert!((w.feet[2] - 0.02).abs() < 0.001);
    }
    #[test]
    fn swimming_floats_accelerates_stops_and_cannot_jump() {
        let world = floor(-20.);
        let water = WaterGrid::test_flat(6.);
        let mut w = walker(&world, Some(&water));
        let mut c = controller();
        for _ in 0..360 {
            c.step(&mut w, &world, Some(&water), [1., 0.], 4.5, true, STEP);
        }
        assert_eq!(c.phase, Phase::SwimFast);
        assert!(w.feet[0] > 7.5 && w.feet[0] < 8.1);
        assert!((w.feet[2] - 5.1).abs() < 0.001 && !w.grounded);
        let x = w.feet[0];
        for _ in 0..360 {
            c.step(&mut w, &world, Some(&water), [0.; 2], 0., false, STEP);
        }
        assert_eq!(c.phase, Phase::SwimIdle);
        assert!((w.feet[2] - (6. - SWIM_IDLE_DEPTH)).abs() < 0.01);
        assert!(w.feet[0] > x && w.feet[0] < x + 0.75);
        assert!(c.swim_velocity[0].abs() < 0.001);
        c.reset();
        assert!(!c.swimming());
    }
    #[test]
    fn falling_into_water_is_arrested_by_buoyancy() {
        let world = floor(-20.);
        let water = WaterGrid::test_flat(6.);
        let mut w = walker(&world, Some(&water));
        let mut c = controller();
        w.feet[2] = 14.;
        w.grounded = false;
        let mut minimum = f32::INFINITY;
        for _ in 0..600 {
            c.step(&mut w, &world, Some(&water), [0.; 2], 0., false, STEP);
            minimum = minimum.min(w.feet[2]);
        }
        assert!(c.swimming() && minimum > 4.4, "water entry z={minimum}");
        assert!((w.feet[2] - (6. - SWIM_IDLE_DEPTH)).abs() < 0.01);
    }
    #[test]
    fn swimming_uses_body_collisions_at_quay_walls() {
        let mut ts = floor(-20.).triangles;
        ts.extend([
            triangle([[3., -20., -20.], [3., 20., -20.], [3., 20., 10.]]),
            triangle([[3., -20., -20.], [3., 20., 10.], [3., -20., 10.]]),
        ]);
        let world = CollisionWorld::from_triangles(ts);
        let water = WaterGrid::test_flat(6.);
        let mut w = walker(&world, Some(&water));
        let mut c = controller();
        for _ in 0..600 {
            c.step(&mut w, &world, Some(&water), [1., 0.], 4.5, false, STEP);
        }
        assert!(w.feet[0] > 2.5 && w.feet[0] < 2.71, "{:?}", w.feet);
        assert!((w.feet[2] - 5.1).abs() < 0.01);
    }
    #[test]
    fn swimmer_can_leave_water_on_a_sloping_bank() {
        let mut ts = floor(-20.).triangles;
        ts.extend([
            triangle([[0., -10., 4.], [20., -10., 10.], [20., 10., 10.]]),
            triangle([[0., -10., 4.], [20., 10., 10.], [0., 10., 4.]]),
        ]);
        let world = CollisionWorld::from_triangles(ts);
        let water = WaterGrid::test_flat(6.);
        let mut w = walker(&world, Some(&water));
        let mut c = controller();
        for _ in 0..960 {
            c.step(&mut w, &world, Some(&water), [1., 0.], 1.8, false, STEP);
        }
        assert!(
            w.feet[0] > 9. && w.grounded && !c.swimming(),
            "feet={:?} phase={:?}",
            w.feet,
            c.phase
        );
    }
    #[test]
    fn simulation_substeps_match_across_frame_rates() {
        let world = floor(-20.);
        let water = WaterGrid::test_flat(6.);
        let mut a = walker(&world, Some(&water));
        let mut b = walker(&world, Some(&water));
        let mut ca = controller();
        let mut cb = controller();
        for _ in 0..360 {
            ca.step(&mut a, &world, Some(&water), [0., 1.], 1.8, false, STEP);
        }
        for _ in 0..90 {
            cb.step(&mut b, &world, Some(&water), [0., 1.], 1.8, false, 1. / 30.);
        }
        for axis in 0..3 {
            assert!((a.feet[axis] - b.feet[axis]).abs() < 0.0001);
        }
    }

    #[test]
    fn launch_under_a_low_ceiling_still_lands_and_recovers() {
        let mut ts = floor(0.).triangles;
        ts.extend([
            triangle([[-20., -20., 2.1], [20., 20., 2.1], [20., -20., 2.1]]),
            triangle([[-20., -20., 2.1], [-20., 20., 2.1], [20., 20., 2.1]]),
        ]);
        let world = CollisionWorld::from_triangles(ts);
        let mut w = Walker::from_camera(
            &world,
            &Camera {
                pos: [0., 0., 1.9],
                yaw: 0.,
                pitch: 0.,
            },
        )
        .unwrap();
        let mut c = controller();
        for i in 0..180 {
            c.step(&mut w, &world, None, [0.; 2], 0., i == 0, STEP);
            assert!(w.feet[2] < 0.24);
        }
        assert!(w.grounded && c.phase == Phase::Ground);
    }

    #[test]
    #[ignore = "Requires the user's local ISO"]
    fn native_water_and_land_contacts_select_swimming_correctly() {
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        crate::scene::select_collisions(&mut world, 0);
        let water = WaterGrid::load(&game).unwrap();
        for speed in [0., 1.8, 4.5] {
            let camera = Camera {
                pos: [-500., -850., 12.],
                yaw: 0.,
                pitch: 0.,
            };
            let mut w = Walker::from_camera_with_water(&world, &camera, Some(&water)).unwrap();
            let mut c = controller();
            let start = w.feet;
            for _ in 0..360 {
                c.step(&mut w, &world, Some(&water), [0., 1.], speed, false, STEP);
            }
            let z = water.height(w.feet).unwrap();
            assert!(c.swimming() && !w.grounded && (w.feet[2] - (z - c.immersion())).abs() < 0.01);
            println!(
                "Native swim speed={speed} clip={} start={start:?} end={:?}",
                c.pose().clip,
                w.feet
            );
        }
        let paths = crate::paths::PathNetwork::load(&game).unwrap();
        let mut checked = 0;
        for node in paths.nodes.iter().skip(paths.car_nodes).step_by(11) {
            let p = node.position;
            let camera = Camera {
                pos: [p[0], p[1], p[2] + 2.],
                yaw: 0.,
                pitch: 0.,
            };
            let Some(mut w) = Walker::from_camera_with_water(&world, &camera, Some(&water)) else {
                continue;
            };
            if !w.grounded || !w.has_clear_body(&world) {
                continue;
            }
            let mut c = controller();
            for _ in 0..12 {
                c.step(&mut w, &world, Some(&water), [0.; 2], 0., false, STEP);
            }
            assert!(
                !c.swimming(),
                "dry footpath incorrectly swims at {:?}",
                w.feet
            );
            checked += 1;
            if checked >= 100 {
                break;
            }
        }
        assert_eq!(checked, 100);
    }
}
