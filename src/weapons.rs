#![allow(dead_code)]
//! 3D weapons, ballistics and projectile physics for VCS-Rust.
//! Supports hitscan bullet traces, parabolic grenade arcs (g = 9.81 m/s²),
//! RPG rockets with smoke trails, flamethrower particles and sniper scopes.

use crate::collision::CollisionQuery;

pub const GRAVITY: f32 = 9.81;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponType {
    Fists,
    Pistol,
    MicroSmg,
    Shotgun,
    AssaultRifle,
    SniperRifle,
    RocketLauncher,
    Grenade,
    Flamethrower,
}

impl WeaponType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Fists => "Mains nues",
            Self::Pistol => "Pistolet 9mm",
            Self::MicroSmg => "Micro-Mitraillette",
            Self::Shotgun => "Fusil à pompe",
            Self::AssaultRifle => "Fusil d'assaut AK-47",
            Self::SniperRifle => "Fusil de sniper",
            Self::RocketLauncher => "Lance-roquettes RPG-7",
            Self::Grenade => "Grenade quadrillée",
            Self::Flamethrower => "Lance-flammes",
        }
    }

    pub fn fire_interval(&self) -> f32 {
        match self {
            Self::Fists => 0.45,
            Self::Pistol => 0.28,
            Self::MicroSmg => 0.08,
            Self::Shotgun => 0.75,
            Self::AssaultRifle => 0.12,
            Self::SniperRifle => 1.10,
            Self::RocketLauncher => 1.50,
            Self::Grenade => 0.85,
            Self::Flamethrower => 0.04,
        }
    }

    pub fn damage(&self) -> u32 {
        match self {
            Self::Fists => 12,
            Self::Pistol => 25,
            Self::MicroSmg => 18,
            Self::Shotgun => 85,
            Self::AssaultRifle => 35,
            Self::SniperRifle => 150,
            Self::RocketLauncher => 250,
            Self::Grenade => 180,
            Self::Flamethrower => 15,
        }
    }

    pub fn range(&self) -> f32 {
        match self {
            Self::Fists => 1.8,
            Self::Pistol => 60.0,
            Self::MicroSmg => 55.0,
            Self::Shotgun => 35.0,
            Self::AssaultRifle => 120.0,
            Self::SniperRifle => 300.0,
            Self::RocketLauncher => 180.0,
            Self::Grenade => 35.0,
            Self::Flamethrower => 18.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileKind {
    Rocket,
    Grenade,
    Flame,
}

#[derive(Clone, Debug)]
pub struct Projectile3D {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub gravity: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub radius: f32,
    pub kind: ProjectileKind,
    pub exploded: bool,
    pub trail: Vec<[f32; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub struct BulletTracer {
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Smoke,
    Flame,
    Spark,
}

#[derive(Clone, Copy, Debug)]
pub struct Particle3D {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub size: f32,
    pub life: f32,
    pub max_life: f32,
    pub color: [f32; 4],
    pub kind: ParticleKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrosshairKind {
    None,
    DotCross,
    CircleSpread,
    SniperScope,
}

#[derive(Clone, Debug)]
pub struct WeaponArsenal {
    pub current: WeaponType,
    pub ammo: [u32; 9],
    pub cooldown: f32,
    pub is_aiming: bool,
    pub sniper_zoom: bool,
    pub projectiles: Vec<Projectile3D>,
    pub tracers: Vec<BulletTracer>,
    pub particles: Vec<Particle3D>,
}

impl Default for WeaponArsenal {
    fn default() -> Self {
        Self {
            current: WeaponType::Pistol,
            ammo: [999, 120, 300, 48, 250, 20, 10, 15, 200],
            cooldown: 0.0,
            is_aiming: false,
            sniper_zoom: false,
            projectiles: Vec::new(),
            tracers: Vec::new(),
            particles: Vec::new(),
        }
    }
}

impl WeaponArsenal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn select(&mut self, weapon: WeaponType) {
        self.current = weapon;
        self.sniper_zoom = false;
        self.cooldown = 0.0;
    }

    pub fn cycle_next(&mut self) {
        let weapons = [
            WeaponType::Fists,
            WeaponType::Pistol,
            WeaponType::MicroSmg,
            WeaponType::Shotgun,
            WeaponType::AssaultRifle,
            WeaponType::SniperRifle,
            WeaponType::RocketLauncher,
            WeaponType::Grenade,
            WeaponType::Flamethrower,
        ];
        let idx = weapons.iter().position(|w| *w == self.current).unwrap_or(0);
        self.select(weapons[(idx + 1) % weapons.len()]);
    }

    pub fn crosshair_kind(&self) -> CrosshairKind {
        if !self.is_aiming && self.current != WeaponType::SniperRifle {
            return CrosshairKind::None;
        }
        match self.current {
            WeaponType::Fists => CrosshairKind::None,
            WeaponType::Shotgun => CrosshairKind::CircleSpread,
            WeaponType::SniperRifle => {
                if self.is_aiming {
                    CrosshairKind::SniperScope
                } else {
                    CrosshairKind::DotCross
                }
            }
            _ => CrosshairKind::DotCross,
        }
    }

    pub fn fire(
        &mut self,
        origin: [f32; 3],
        direction: [f32; 3],
        world: &impl CollisionQuery,
    ) -> bool {
        if self.cooldown > 0.0 {
            return false;
        }
        let weapon_idx = match self.current {
            WeaponType::Fists => 0,
            WeaponType::Pistol => 1,
            WeaponType::MicroSmg => 2,
            WeaponType::Shotgun => 3,
            WeaponType::AssaultRifle => 4,
            WeaponType::SniperRifle => 5,
            WeaponType::RocketLauncher => 6,
            WeaponType::Grenade => 7,
            WeaponType::Flamethrower => 8,
        };

        if self.current != WeaponType::Fists && self.ammo[weapon_idx] == 0 {
            return false;
        }
        if self.current != WeaponType::Fists {
            self.ammo[weapon_idx] = self.ammo[weapon_idx].saturating_sub(1);
        }

        self.cooldown = self.current.fire_interval();
        let norm_dir = normalize(direction);

        match self.current {
            WeaponType::Fists => {}
            WeaponType::Pistol | WeaponType::MicroSmg | WeaponType::AssaultRifle | WeaponType::SniperRifle => {
                let range = self.current.range();
                let hit_end = if let Some(hit) = world.raycast(origin, norm_dir, range) {
                    hit.position
                } else {
                    [
                        origin[0] + norm_dir[0] * range,
                        origin[1] + norm_dir[1] * range,
                        origin[2] + norm_dir[2] * range,
                    ]
                };
                let color = if self.current == WeaponType::SniperRifle {
                    [0.2, 0.9, 1.0, 0.9]
                } else {
                    [1.0, 0.85, 0.2, 0.85]
                };
                self.tracers.push(BulletTracer {
                    start: origin,
                    end: hit_end,
                    lifetime: 0.14,
                    max_lifetime: 0.14,
                    color,
                });
                self.spawn_impact_sparks(hit_end);
            }
            WeaponType::Shotgun => {
                let range = self.current.range();
                for pellet in 0..7 {
                    let spread_x = ((pellet as f32 * 1.3).sin() * 0.05).clamp(-0.06, 0.06);
                    let spread_z = ((pellet as f32 * 2.1).cos() * 0.05).clamp(-0.06, 0.06);
                    let p_dir = normalize([
                        norm_dir[0] + spread_x,
                        norm_dir[1],
                        norm_dir[2] + spread_z,
                    ]);
                    let hit_end = if let Some(hit) = world.raycast(origin, p_dir, range) {
                        hit.position
                    } else {
                        [
                            origin[0] + p_dir[0] * range,
                            origin[1] + p_dir[1] * range,
                            origin[2] + p_dir[2] * range,
                        ]
                    };
                    self.tracers.push(BulletTracer {
                        start: origin,
                        end: hit_end,
                        lifetime: 0.12,
                        max_lifetime: 0.12,
                        color: [1.0, 0.75, 0.3, 0.75],
                    });
                }
            }
            WeaponType::RocketLauncher => {
                let speed = 48.0;
                let velocity = [norm_dir[0] * speed, norm_dir[1] * speed, norm_dir[2] * speed];
                self.projectiles.push(Projectile3D {
                    position: origin,
                    velocity,
                    gravity: 0.0,
                    lifetime: 3.5,
                    max_lifetime: 3.5,
                    radius: 0.25,
                    kind: ProjectileKind::Rocket,
                    exploded: false,
                    trail: vec![origin],
                });
            }
            WeaponType::Grenade => {
                let throw_speed = 18.0;
                let velocity = [
                    norm_dir[0] * throw_speed,
                    norm_dir[1] * throw_speed,
                    (norm_dir[2] + 0.35) * throw_speed,
                ];
                self.projectiles.push(Projectile3D {
                    position: origin,
                    velocity,
                    gravity: -GRAVITY,
                    lifetime: 2.8,
                    max_lifetime: 2.8,
                    radius: 0.15,
                    kind: ProjectileKind::Grenade,
                    exploded: false,
                    trail: vec![origin],
                });
            }
            WeaponType::Flamethrower => {
                let speed = 22.0;
                for i in 0..3 {
                    let spread_rnd = (i as f32 - 1.0) * 0.08;
                    let flame_dir = normalize([
                        norm_dir[0] + spread_rnd,
                        norm_dir[1],
                        norm_dir[2] + spread_rnd * 0.5,
                    ]);
                    self.projectiles.push(Projectile3D {
                        position: origin,
                        velocity: [
                            flame_dir[0] * speed,
                            flame_dir[1] * speed,
                            flame_dir[2] * speed,
                        ],
                        gravity: -1.5,
                        lifetime: 0.85,
                        max_lifetime: 0.85,
                        radius: 0.45,
                        kind: ProjectileKind::Flame,
                        exploded: false,
                        trail: vec![origin],
                    });
                }
            }
        }
        true
    }

    pub fn update(&mut self, world: &impl CollisionQuery, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.1);
        self.cooldown = (self.cooldown - dt).max(0.0);

        // Update bullet tracers
        for tracer in &mut self.tracers {
            tracer.lifetime -= dt;
        }
        self.tracers.retain(|t| t.lifetime > 0.0);

        // Update particles
        for p in &mut self.particles {
            p.life -= dt;
            p.position[0] += p.velocity[0] * dt;
            p.position[1] += p.velocity[1] * dt;
            p.position[2] += p.velocity[2] * dt;
            if p.kind == ParticleKind::Smoke {
                p.size += 0.4 * dt;
                p.color[3] = (p.life / p.max_life).max(0.0) * 0.6;
            } else if p.kind == ParticleKind::Flame {
                p.size += 0.6 * dt;
                p.color[3] = (p.life / p.max_life).max(0.0) * 0.8;
            }
        }
        self.particles.retain(|p| p.life > 0.0);

        // Update active projectiles
        let mut new_particles = Vec::new();
        for proj in &mut self.projectiles {
            proj.lifetime -= dt;
            proj.velocity[2] += proj.gravity * dt;

            let step = [
                proj.velocity[0] * dt,
                proj.velocity[1] * dt,
                proj.velocity[2] * dt,
            ];
            let step_dist = (step[0] * step[0] + step[1] * step[1] + step[2] * step[2]).sqrt();
            let step_dir = if step_dist > 1e-6 {
                [step[0] / step_dist, step[1] / step_dist, step[2] / step_dist]
            } else {
                [0.0, 0.0, 1.0]
            };

            if let Some(hit) = world.raycast(proj.position, step_dir, step_dist + proj.radius) {
                if proj.kind == ProjectileKind::Rocket || proj.kind == ProjectileKind::Flame {
                    proj.exploded = true;
                    proj.position = hit.position;
                    spawn_explosion_particles(&mut new_particles, hit.position, proj.kind);
                } else if proj.kind == ProjectileKind::Grenade {
                    // Grenade bounce
                    proj.position = [
                        hit.position[0] + hit.normal[0] * 0.05,
                        hit.position[1] + hit.normal[1] * 0.05,
                        hit.position[2] + hit.normal[2] * 0.05,
                    ];
                    let dot = proj.velocity[0] * hit.normal[0]
                        + proj.velocity[1] * hit.normal[1]
                        + proj.velocity[2] * hit.normal[2];
                    proj.velocity[0] = (proj.velocity[0] - 2.0 * dot * hit.normal[0]) * 0.45;
                    proj.velocity[1] = (proj.velocity[1] - 2.0 * dot * hit.normal[1]) * 0.45;
                    proj.velocity[2] = (proj.velocity[2] - 2.0 * dot * hit.normal[2]) * 0.45;
                }
            } else {
                proj.position[0] += step[0];
                proj.position[1] += step[1];
                proj.position[2] += step[2];
            }

            if proj.kind == ProjectileKind::Rocket {
                // Smoke trail puff
                new_particles.push(Particle3D {
                    position: proj.position,
                    velocity: [0.0, 0.0, 0.2],
                    size: 0.35,
                    life: 0.6,
                    max_life: 0.6,
                    color: [0.8, 0.8, 0.85, 0.5],
                    kind: ParticleKind::Smoke,
                });
            } else if proj.kind == ProjectileKind::Flame {
                new_particles.push(Particle3D {
                    position: proj.position,
                    velocity: [0.0, 0.0, 0.5],
                    size: 0.45,
                    life: 0.4,
                    max_life: 0.4,
                    color: [1.0, 0.5, 0.1, 0.7],
                    kind: ParticleKind::Flame,
                });
            }

            if proj.lifetime <= 0.0 && !proj.exploded {
                proj.exploded = true;
                spawn_explosion_particles(&mut new_particles, proj.position, proj.kind);
            }
        }

        self.projectiles.retain(|p| !p.exploded && p.lifetime > 0.0);
        self.particles.extend(new_particles);
    }

    fn spawn_impact_sparks(&mut self, pos: [f32; 3]) {
        for i in 0..4 {
            let angle = i as f32 * 1.57;
            self.particles.push(Particle3D {
                position: pos,
                velocity: [angle.cos() * 3.0, angle.sin() * 3.0, 2.5],
                size: 0.08,
                life: 0.25,
                max_life: 0.25,
                color: [1.0, 0.9, 0.4, 0.9],
                kind: ParticleKind::Spark,
            });
        }
    }
}

fn spawn_explosion_particles(out: &mut Vec<Particle3D>, pos: [f32; 3], kind: ProjectileKind) {
    let count = match kind {
        ProjectileKind::Rocket | ProjectileKind::Grenade => 16,
        ProjectileKind::Flame => 8,
    };
    for i in 0..count {
        let theta = (i as f32 * 0.4).sin();
        let phi = (i as f32 * 0.7).cos();
        let speed = if kind == ProjectileKind::Flame { 2.5 } else { 7.5 };
        out.push(Particle3D {
            position: pos,
            velocity: [theta * speed, phi * speed, (i as f32 % 3.0) * 2.0],
            size: if kind == ProjectileKind::Flame { 0.5 } else { 0.8 },
            life: 0.55,
            max_life: 0.55,
            color: if kind == ProjectileKind::Flame {
                [1.0, 0.4, 0.05, 0.8]
            } else {
                [1.0, 0.6, 0.1, 0.9]
            },
            kind: if kind == ProjectileKind::Flame {
                ParticleKind::Flame
            } else {
                ParticleKind::Smoke
            },
        });
    }
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-6 {
        [v[0] / len, v[1] / len, v[2] / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWorld;
    impl CollisionQuery for MockWorld {
        fn raycast(&self, _origin: [f32; 3], _direction: [f32; 3], _max_distance: f32) -> Option<crate::collision::Hit> {
            None
        }
    }

    #[test]
    fn weapon_switching_and_ammo_deduction() {
        let mut arsenal = WeaponArsenal::new();
        arsenal.select(WeaponType::Pistol);
        let initial_ammo = arsenal.ammo[1];
        let fired = arsenal.fire([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], &MockWorld);
        assert!(fired);
        assert_eq!(arsenal.ammo[1], initial_ammo - 1);
        assert_eq!(arsenal.tracers.len(), 1);
    }

    #[test]
    fn parabolic_grenade_trajectory_obeys_gravity() {
        let mut arsenal = WeaponArsenal::new();
        arsenal.select(WeaponType::Grenade);
        arsenal.fire([0.0, 0.0, 2.0], [0.0, 1.0, 0.0], &MockWorld);
        assert_eq!(arsenal.projectiles.len(), 1);
        let _initial_z = arsenal.projectiles[0].position[2];
        arsenal.update(&MockWorld, 0.2);
        // After small dt with upwards throw, grenade moves and accelerates downward by -g
        assert_eq!(arsenal.projectiles[0].gravity, -GRAVITY);
        assert!(arsenal.projectiles[0].position[1] > 0.0);
    }

    #[test]
    fn rocket_spawns_smoke_particles_during_flight() {
        let mut arsenal = WeaponArsenal::new();
        arsenal.select(WeaponType::RocketLauncher);
        arsenal.fire([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], &MockWorld);
        assert_eq!(arsenal.projectiles.len(), 1);
        arsenal.update(&MockWorld, 0.1);
        assert!(!arsenal.particles.is_empty(), "Rocket must leave smoke puffs in trail");
    }

    #[test]
    fn sniper_scope_crosshair_activates_on_aim() {
        let mut arsenal = WeaponArsenal::new();
        arsenal.select(WeaponType::SniperRifle);
        assert_eq!(arsenal.crosshair_kind(), CrosshairKind::DotCross);
        arsenal.is_aiming = true;
        assert_eq!(arsenal.crosshair_kind(), CrosshairKind::SniperScope);
    }
}
