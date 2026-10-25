//! Optional PC movement inspired by later GTA games. These are deliberately
//! isolated from original-mode controls; this is not decoded VCS/SA physics.
use crate::{
    character::{Character, Pose},
    collision::{ActorQuery, CollisionQuery},
    player::Walker,
    render::Camera,
};

pub struct Crouch {
    pub wanted: bool,
    pub clock: f32,
    pub gait: f32,
    pub walking: bool,
    heights: [f32; 17],
}
impl Crouch {
    pub fn new(actor: &Character) -> Self {
        let duration = actor.duration("WEAPON_crouch").unwrap_or(1. / 3.);
        let heights = std::array::from_fn(|i| {
            let pose = Pose {
                clip: "WEAPON_crouch",
                time: duration * i as f32 / 16.,
                looping: false,
                root_height: true,
            };
            let mesh = actor.crouch_triangles(pose, 0., false);
            (mesh
                .iter()
                .flat_map(|t| &t.vertices)
                .map(|v| v.p[2])
                .fold(0., f32::max)
                + 0.03)
                .clamp(1.23, 1.85)
        });
        Self {
            wanted: false,
            clock: 0.,
            gait: 0.,
            walking: false,
            heights,
        }
    }
    pub fn reset(&mut self) {
        self.wanted = false;
        self.clock = 0.;
        self.gait = 0.;
        self.walking = false;
    }
    pub fn active(&self) -> bool {
        self.clock > 0.0001 || self.wanted
    }
    pub fn moved(&mut self, before: [f32; 3], after: [f32; 3]) {
        let d = (after[0] - before[0]).hypot(after[1] - before[1]);
        self.walking = d > 0.0001;
        if self.active() {
            self.gait += d / 0.75;
        }
    }
    pub fn tick(
        &mut self,
        actor: &Character,
        walker: &mut Walker,
        world: &impl CollisionQuery,
        enabled: bool,
        dt: f32,
    ) {
        if !enabled {
            self.wanted = false;
        }
        if !dt.is_finite() || dt <= 0. {
            return;
        }
        let duration = actor.duration("WEAPON_crouch").unwrap_or(1. / 3.);
        let time = (self.clock + if self.wanted { dt } else { -dt }).clamp(0., duration);
        // Use an upper envelope of actual skinned pose heights. A linear body
        // shrink can lower the collider before the native head has bent down.
        let sample = (time / duration * 16.).clamp(0., 16.);
        let height = if time <= 0. {
            1.85
        } else {
            self.heights[sample.floor() as usize].max(self.heights[sample.ceil() as usize])
        };
        if walker.set_crouch(world, (1.85 - height) / 0.62) {
            self.clock = time;
        }
    }
    pub fn pose(&self) -> Pose {
        Pose {
            clip: "WEAPON_crouch",
            time: self.clock,
            looping: false,
            root_height: true,
        }
    }
}

pub struct Vault {
    start: [f32; 3],
    end: [f32; 3],
    height: f32,
    pub clock: f32,
}
impl Vault {
    pub fn begin(world: &impl CollisionQuery, w: &Walker, heading: f32) -> Option<Self> {
        if !w.grounded || w.crouch_amount() > 0.01 {
            return None;
        }
        let world = &ActorQuery(world);
        let (s, c) = heading.sin_cos();
        let wall = world.raycast([w.feet[0], w.feet[1], w.feet[2] + 0.45], [s, c, 0.], 0.95)?;
        if wall.normal[2].abs() > 0.35 {
            return None;
        }
        let top = world.raycast(
            [
                wall.position[0] + s * 0.08,
                wall.position[1] + c * 0.08,
                w.feet[2] + 1.65,
            ],
            [0., 0., -1.],
            1.3,
        )?;
        let rise = top.position[2] + 0.02 - w.feet[2];
        if top.normal[2] < 0.7 || !(0.4..=1.45).contains(&rise) {
            return None;
        }
        // Prefer crossing a narrow wall with a safe floor on the other side;
        // otherwise climb onto a wide, clear low platform. Never into water or
        // down an unprobed drop, and never bypass an overhang.
        for distance in [2.0, 1.65, 1.3, wall.distance + 0.55] {
            let p = [
                w.feet[0] + s * distance,
                w.feet[1] + c * distance,
                top.position[2] + 0.15,
            ];
            let Some(floor) = world.raycast(p, [0., 0., -1.], rise + 0.5) else {
                continue;
            };
            if floor.normal[2] < 0.7 || floor.position[2] < w.feet[2] - 0.3 {
                continue;
            }
            let Some(end) = Walker::from_camera(
                world,
                &Camera {
                    pos: p,
                    yaw: heading,
                    pitch: 0.,
                },
            ) else {
                continue;
            };
            if !end.has_clear_body(world) {
                continue;
            }
            let vault = Self {
                start: w.feet,
                end: end.feet,
                height: top.position[2] + 0.12,
                clock: 0.,
            };
            let mut probe = w.clone();
            if (1..=120).all(|i| probe.traverse_to(world, vault.at(i as f32 / 120.))) {
                return Some(vault);
            }
        }
        None
    }
    fn at(&self, t: f32) -> [f32; 3] {
        let smooth = |x: f32| {
            let x = x.clamp(0., 1.);
            x * x * (3. - 2. * x)
        };
        let across = smooth((t - 0.34) / 0.43);
        let z = if t < 0.34 {
            self.start[2] + (self.height - self.start[2]) * smooth(t / 0.34)
        } else if t < 0.77 {
            self.height
        } else {
            self.height + (self.end[2] - self.height) * smooth((t - 0.77) / 0.23)
        };
        [
            self.start[0] + (self.end[0] - self.start[0]) * across,
            self.start[1] + (self.end[1] - self.start[1]) * across,
            z,
        ]
    }
    pub fn tick(&mut self, world: &impl CollisionQuery, walker: &mut Walker, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0. {
            return false;
        }
        let ticks = (dt.min(0.1) * 120.).ceil().max(1.) as usize;
        for _ in 0..ticks {
            self.clock = (self.clock + dt.min(0.1) / ticks as f32).min(0.9);
            if !walker.traverse_to(world, self.at(self.clock / 0.9)) {
                return true;
            }
        }
        if self.clock >= 0.9 {
            walker.grounded = true;
            return true;
        }
        false
    }
    pub fn pose(&self, actor: &Character) -> Pose {
        let t = self.clock / 0.9;
        let (clip, time) = if t < 0.34 {
            (
                "JUMP_launch",
                t / 0.34 * actor.duration("JUMP_launch").unwrap_or(0.3),
            )
        } else if t < 0.77 {
            (
                "JUMP_glide",
                (t - 0.34) / 0.43 * actor.duration("JUMP_glide").unwrap_or(0.3),
            )
        } else {
            (
                "JUMP_land",
                (t - 0.77) / 0.23 * actor.duration("JUMP_land").unwrap_or(0.3),
            )
        };
        Pose {
            clip,
            time,
            looping: false,
            root_height: true,
        }
    }
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"time":self.clock,"start":self.start,"end":self.end,"height":self.height})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Requires the user's local ISO"]
    fn native_low_ledge_can_be_crossed_without_teleporting() {
        let mut disc = crate::iso::Disc::open(std::path::Path::new(crate::DEFAULT_ISO)).unwrap();
        let game = crate::leeds::inflate(&disc.read("PS2/GAME.DTZ").unwrap()).unwrap();
        let mut world = crate::collision::load_world(&mut disc, &game).unwrap();
        crate::scene::select_collisions(&mut world, 0);
        for centre in [[239., -676.], [500., -500.], [-209., -964.]] {
            for x in -32..32 {
                for y in -32..32 {
                    let camera = Camera {
                        pos: [centre[0] + x as f32 * 0.5, centre[1] + y as f32 * 0.5, 30.],
                        yaw: 0.,
                        pitch: 0.,
                    };
                    let Some(w) = Walker::from_camera(&world, &camera) else {
                        continue;
                    };
                    if !w.has_clear_body(&world) {
                        continue;
                    }
                    for angle in 0..8 {
                        let heading = angle as f32 * std::f32::consts::TAU / 8.;
                        let Some(mut v) = Vault::begin(&world, &w, heading) else {
                            continue;
                        };
                        let view =
                            (0..8)
                                .map(|i| i as f32 * std::f32::consts::TAU / 8.)
                                .find(|&yaw| {
                                    [w.feet, v.at(0.5), v.end].iter().all(|&p| {
                                        let cam = crate::render::follow_camera(
                                            &Camera {
                                                pos: p,
                                                yaw,
                                                pitch: -0.25,
                                            },
                                            p,
                                            &world,
                                        );
                                        (cam.pos[0] - p[0]).hypot(cam.pos[1] - p[1]) > 3.8
                                    })
                                });
                        let Some(view) = view else { continue };
                        let start = w.feet;
                        let mut w = w.clone();
                        for _ in 0..110 {
                            if v.tick(&world, &mut w, 1. / 120.) {
                                break;
                            }
                        }
                        assert!(
                            w.grounded && (w.feet[0] - v.end[0]).hypot(w.feet[1] - v.end[1]) < 0.01
                        );
                        std::fs::write("../../work/modern-ledge-fixture.json",serde_json::to_vec_pretty(&serde_json::json!({"start":start,"end":w.feet,"yaw":heading,"view":[view,-0.25],"camera":[start[0],start[1],start[2]+2.,heading,-0.1]})).unwrap()).unwrap();
                        println!(
                            "native ledge start={start:?} end={:?} yaw={heading}",
                            w.feet
                        );
                        return;
                    }
                }
            }
        }
        panic!("No low ledge found in native street fixture");
    }
    use crate::collision::{test_box_collider, CollisionScene, CollisionWorld, Triangle};
    fn ground() -> CollisionWorld {
        CollisionWorld::from_triangles(vec![
            Triangle {
                vertices: [[-20., -20., 0.], [20., -20., 0.], [20., 20., 0.]],
                surface: 0,
                model_id: 1,
            },
            Triangle {
                vertices: [[-20., -20., 0.], [20., 20., 0.], [-20., 20., 0.]],
                surface: 0,
                model_id: 1,
            },
        ])
    }
    #[test]
    fn modern_vault_sweeps_low_wall_and_rejects_high_wall_and_ceiling() {
        let world = ground();
        let low = test_box_collider([[-3., 0.6, 0.], [3., 1.1, 0.9]]);
        let instances = [low.at([0.; 3], 0.)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let mut w = Walker::from_camera(
            &scene,
            &Camera {
                pos: [0., 0., 3.],
                yaw: 0.,
                pitch: 0.,
            },
        )
        .unwrap();
        let mut v = Vault::begin(&scene, &w, 0.).expect("low wall");
        for _ in 0..110 {
            v.tick(&scene, &mut w, 1. / 120.);
        }
        assert!(w.feet[1] > 1.4 && w.grounded && (w.feet[2] - 0.02).abs() < 0.01);
        let high = test_box_collider([[-3., 0.6, 0.], [3., 1.1, 3.]]);
        let ceiling = test_box_collider([[-3., -2., 2.], [3., 3., 3.]]);
        for shapes in [
            vec![high.at([0.; 3], 0.)],
            vec![low.at([0.; 3], 0.), ceiling.at([0.; 3], 0.)],
        ] {
            let scene = CollisionScene {
                world: &world,
                instances: &shapes,
            };
            let w = Walker::from_camera(
                &world,
                &Camera {
                    pos: [0., 0., 3.],
                    yaw: 0.,
                    pitch: 0.,
                },
            )
            .unwrap();
            assert!(Vault::begin(&scene, &w, 0.).is_none());
        }
    }
    #[test]
    fn crouched_body_cannot_stand_into_a_low_ceiling() {
        let world = ground();
        let roof = test_box_collider([[-2., 1., 1.4], [2., 3., 2.]]);
        let instances = [roof.at([0.; 3], 0.)];
        let scene = CollisionScene {
            world: &world,
            instances: &instances,
        };
        let mut w = Walker::from_camera(
            &world,
            &Camera {
                pos: [0., 0., 3.],
                yaw: 0.,
                pitch: 0.,
            },
        )
        .unwrap();
        assert!(w.set_crouch(&scene, 1.));
        for _ in 0..200 {
            w.step(&scene, [0., 1.], 1., false, 1. / 120.);
        }
        assert!(w.feet[1] > 1.4);
        assert!(!w.can_stand(&scene));
        assert!(!w.set_crouch(&scene, 0.));
        for _ in 0..200 {
            w.step(&scene, [0., -1.], 1., false, 1. / 120.);
        }
        assert!(w.set_crouch(&scene, 0.));
    }

    #[test]
    fn modern_vault_detects_a_thin_low_wall() {
        let world = ground();
        let wall = test_box_collider([[-3., 0.6, 0.], [3., 0.74, 1.1]]);
        let shapes = [wall.at([0.; 3], 0.)];
        let scene = CollisionScene {
            world: &world,
            instances: &shapes,
        };
        let mut w = Walker::from_camera(
            &world,
            &Camera {
                pos: [0., 0., 3.],
                yaw: 0.,
                pitch: 0.,
            },
        )
        .unwrap();
        let mut vault =
            Vault::begin(&scene, &w, 0.).expect("14 cm wall must have a detectable top");
        for _ in 0..110 {
            if vault.tick(&scene, &mut w, 1. / 120.) {
                break;
            }
        }
        assert!(w.grounded && w.feet[1] > 1.4 && w.feet[2] < 0.03);
    }
}
