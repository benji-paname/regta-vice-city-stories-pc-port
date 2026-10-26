#![allow(dead_code)]
//! Advanced ballistics, rocket propulsion, flamethrower jets, and sniper optics.
//! Implements CProjectileRocket, CFireFlamethrower, and CSniperScopeView.

#[derive(Clone, Debug)]
pub struct CProjectileRocket {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: f32,
    pub top_speed: f32,
    pub blast_radius: f32,
    pub blast_damage: f32,
    pub lifetime: f32,
    pub max_lifetime: f32,
    pub smoke_timer: f32,
    pub smoke_trail: Vec<[f32; 3]>,
    pub detonated: bool,
}

impl CProjectileRocket {
    pub fn new(origin: [f32; 3], direction: [f32; 3], initial_speed: f32) -> Self {
        let dir_norm = {
            let len = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt().max(1e-4);
            [direction[0] / len, direction[1] / len, direction[2] / len]
        };

        Self {
            position: origin,
            velocity: [dir_norm[0] * initial_speed, dir_norm[1] * initial_speed, dir_norm[2] * initial_speed],
            acceleration: 45.0, // Rocket booster thrust
            top_speed: 85.0,
            blast_radius: 8.5,
            blast_damage: 280.0,
            lifetime: 0.0,
            max_lifetime: 5.0,
            smoke_timer: 0.0,
            smoke_trail: Vec::with_capacity(64),
            detonated: false,
        }
    }

    pub fn update(&mut self, dt: f32) -> Option<[f32; 3]> {
        if self.detonated || !dt.is_finite() || dt <= 0.0 {
            return None;
        }

        self.lifetime += dt;
        if self.lifetime >= self.max_lifetime {
            self.detonated = true;
            return Some(self.position);
        }

        // Accelerate along velocity direction
        let current_speed = (self.velocity[0] * self.velocity[0] + self.velocity[1] * self.velocity[1] + self.velocity[2] * self.velocity[2]).sqrt();
        if current_speed < self.top_speed && current_speed > 1e-4 {
            let factor = 1.0 + (self.acceleration * dt) / current_speed;
            self.velocity[0] *= factor;
            self.velocity[1] *= factor;
            self.velocity[2] *= factor;
        }

        self.position[0] += self.velocity[0] * dt;
        self.position[1] += self.velocity[1] * dt;
        self.position[2] += self.velocity[2] * dt;

        // Emit smoke
        self.smoke_timer += dt;
        if self.smoke_timer >= 0.035 {
            self.smoke_timer = 0.0;
            if self.smoke_trail.len() >= 48 {
                self.smoke_trail.remove(0);
            }
            self.smoke_trail.push(self.position);
        }

        None
    }

    pub fn trigger_detonation(&mut self) -> [f32; 3] {
        self.detonated = true;
        self.position
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlameParticle {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub radius: f32,
    pub life: f32,
    pub max_life: f32,
}

#[derive(Clone, Debug)]
pub struct CFireFlamethrower {
    pub nozzle_pos: [f32; 3],
    pub direction: [f32; 3],
    pub is_firing: bool,
    pub stream_rate: f32,
    pub stream_timer: f32,
    pub fuel_remaining: f32,
    pub active_particles: Vec<FlameParticle>,
}

impl Default for CFireFlamethrower {
    fn default() -> Self {
        Self {
            nozzle_pos: [0.0; 3],
            direction: [0.0, 1.0, 0.0],
            is_firing: false,
            stream_rate: 35.0, // 35 particles per second
            stream_timer: 0.0,
            fuel_remaining: 500.0,
            active_particles: Vec::with_capacity(128),
        }
    }
}

impl CFireFlamethrower {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start_firing(&mut self) {
        self.is_firing = true;
    }

    pub fn stop_firing(&mut self) {
        self.is_firing = false;
    }

    pub fn update(&mut self, origin: [f32; 3], forward: [f32; 3], dt: f32) -> usize {
        if !dt.is_finite() || dt <= 0.0 {
            return 0;
        }

        self.nozzle_pos = origin;
        self.direction = forward;

        // Spawn new flames when firing
        if self.is_firing && self.fuel_remaining > 0.0 {
            self.fuel_remaining = (self.fuel_remaining - dt * 25.0).max(0.0);
            self.stream_timer += dt;
            let spawn_interval = 1.0 / self.stream_rate;

            while self.stream_timer >= spawn_interval {
                self.stream_timer -= spawn_interval;
                let spread_x = (self.active_particles.len() as f32 * 11.3).sin() * 0.15;
                let spread_z = (self.active_particles.len() as f32 * 19.7).cos() * 0.10;

                let vel = [
                    (self.direction[0] + spread_x) * 16.0,
                    (self.direction[1] + spread_x * 0.5) * 16.0,
                    (self.direction[2] + spread_z + 0.05) * 16.0,
                ];

                self.active_particles.push(FlameParticle {
                    position: self.nozzle_pos,
                    velocity: vel,
                    radius: 0.25,
                    life: 0.0,
                    max_life: 0.65,
                });
            }
        }

        // Update active flame entities
        for p in &mut self.active_particles {
            p.life += dt;
            p.position[0] += p.velocity[0] * dt;
            p.position[1] += p.velocity[1] * dt;
            p.position[2] += p.velocity[2] * dt;

            // Expand flame ball as it consumes oxygen and heats air
            let progress = (p.life / p.max_life).clamp(0.0, 1.0);
            p.radius = 0.25 + progress * 1.45;

            // Buoyancy / thermal updraft
            p.velocity[2] += 2.5 * dt;
            // Drag deceleration
            p.velocity[0] *= 1.0 - 1.8 * dt;
            p.velocity[1] *= 1.0 - 1.8 * dt;
        }

        self.active_particles.retain(|p| p.life < p.max_life);
        self.active_particles.len()
    }

    pub fn check_point_burned(&self, target_pos: [f32; 3]) -> bool {
        for p in &self.active_particles {
            let dx = target_pos[0] - p.position[0];
            let dy = target_pos[1] - p.position[1];
            let dz = target_pos[2] - p.position[2];
            let dist_sq = dx * dx + dy * dy + dz * dz;
            if dist_sq <= p.radius * p.radius {
                return true;
            }
        }
        false
    }
}

#[derive(Clone, Debug)]
pub struct CSniperScopeView {
    pub is_scoped: bool,
    pub zoom_level: f32, // 4.0x to 12.0x
    pub breath_held: bool,
    pub breath_stamina: f32,
    pub sway_time: f32,
    pub sway_offset: [f32; 2],
    pub target_distance_m: f32,
}

impl Default for CSniperScopeView {
    fn default() -> Self {
        Self {
            is_scoped: false,
            zoom_level: 6.0,
            breath_held: false,
            breath_stamina: 1.0,
            sway_time: 0.0,
            sway_offset: [0.0, 0.0],
            target_distance_m: 75.0,
        }
    }
}

impl CSniperScopeView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_scoped(&mut self, scoped: bool) {
        self.is_scoped = scoped;
        if !scoped {
            self.breath_held = false;
        }
    }

    pub fn hold_breath(&mut self, hold: bool) {
        if self.is_scoped && self.breath_stamina > 0.15 {
            self.breath_held = hold;
        } else {
            self.breath_held = false;
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom_level = (self.zoom_level + 1.0).min(12.0);
    }

    pub fn zoom_out(&mut self) {
        self.zoom_level = (self.zoom_level - 1.0).max(3.0);
    }

    pub fn update(&mut self, target_distance: f32, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        self.target_distance_m = target_distance;

        if self.breath_held {
            self.breath_stamina = (self.breath_stamina - dt * 0.25).max(0.0);
            if self.breath_stamina <= 0.0 {
                self.breath_held = false;
            }
        } else {
            self.breath_stamina = (self.breath_stamina + dt * 0.35).min(1.0);
        }

        if self.is_scoped {
            self.sway_time += dt;
            let sway_amplitude = if self.breath_held { 0.0008 } else { 0.0055 };

            // Natural 8-shaped breathing Lissajous sway
            self.sway_offset = [
                (self.sway_time * 1.8).sin() * sway_amplitude,
                (self.sway_time * 3.6).sin() * sway_amplitude * 0.5,
            ];
        } else {
            self.sway_offset = [0.0, 0.0];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rocket_accelerates_and_emits_smoke_trail() {
        let mut rocket = CProjectileRocket::new([0.0, 0.0, 10.0], [0.0, 1.0, 0.0], 20.0);
        assert_eq!(rocket.position, [0.0, 0.0, 10.0]);
        for _ in 0..10 {
            rocket.update(0.05);
        }
        assert!(rocket.velocity[1] > 20.0, "Rocket should accelerate");
        assert!(!rocket.smoke_trail.is_empty(), "Rocket should emit smoke");
    }

    #[test]
    fn flamethrower_emits_expanding_fire_stream() {
        let mut ft = CFireFlamethrower::new();
        ft.start_firing();
        let count = ft.update([0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0.1);
        assert!(count > 0, "Flamethrower should spawn particles");
        let flame_pos = ft.active_particles[0].position;
        assert!(ft.check_point_burned(flame_pos));
    }

    #[test]
    fn sniper_scope_stabilizes_when_holding_breath() {
        let mut scope = CSniperScopeView::new();
        scope.set_scoped(true);
        scope.update(120.0, 0.1);
        let sway_normal = scope.sway_offset[0].abs();

        scope.hold_breath(true);
        scope.update(120.0, 0.1);
        let sway_held = scope.sway_offset[0].abs();

        assert!(sway_held < sway_normal, "Holding breath should reduce scope sway");
    }
}
