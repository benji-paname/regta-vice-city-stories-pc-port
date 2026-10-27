//! Native PS2 aquatic vehicle mechanics, hydrodynamics, buoyancy, and wave interactions.
//! Simulates Vice City Stories boats, speedboats, yachts, dinghies, patrol craft,
//! personal watercraft (Jetski), and amphibious hovercraft (Bovver '64).

use crate::collision::CollisionQuery;
use crate::water::WaterGrid;
use serde::Serialize;

/// Categories of watercraft in Vice City Stories.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
pub enum BoatCategory {
    /// High-performance speedboats (Squalo, Jetmax, Speeder, Scarab)
    Speedboat,
    /// Medium-to-heavy cruising vessels (Tropic, Rio, Marquis, Reefer)
    Cruiser,
    /// Law enforcement and military patrol craft (Predator, Coastguard)
    Patrol,
    /// Light rigid inflatables and skiffs (Dinghy, Yola)
    Inflatable,
    /// Agile personal watercraft with direct nozzle jet vectoring (Jetski)
    Jetski,
    /// Amphibious air-cushion vehicle capable of water and land travel (Bovver '64 Hovercraft)
    Hovercraft,
    /// Seaplane fitted with pontoons for water taxiing (Skimmer)
    Seaplane,
}

/// Physical and hydrodynamic tuning parameters for each watercraft category.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize)]
pub struct WatercraftParams {
    /// Total vessel mass (kg)
    pub mass: f32,
    /// Maximum forward engine / waterjet thrust (Newtons)
    pub forward_thrust: f32,
    /// Maximum reverse thrust for stopping / backing up (Newtons)
    pub reverse_thrust: f32,
    /// Theoretical maximum water speed (m/s)
    pub top_speed: f32,
    /// Rudder torque / steering agility coefficient
    pub rudder_agility: f32,
    /// Hydrodynamic longitudinal drag coefficient
    pub longitudinal_drag: f32,
    /// Hydrodynamic lateral keel resistance coefficient (damps sideslip)
    pub keel_resistance: f32,
    /// Archimedes hydrostatic buoyancy spring stiffness
    pub buoyancy_stiffness: f32,
    /// Hydrodynamic vertical heave damping
    pub buoyancy_damping: f32,
    /// Forward velocity threshold where the hull starts planing (m/s)
    pub planing_speed: f32,
    /// Maximum banking roll angle during sharp high-speed turns (radians)
    pub max_roll_angle: f32,
    /// Maximum bow trim pitch angle under acceleration / wave chop (radians)
    pub max_pitch_angle: f32,
    /// Draft: nominal hull immersion depth at rest (meters)
    pub nominal_draft: f32,
    /// Whether the craft is amphibious (air-cushioned hovercraft)
    pub is_amphibious: bool,
}

#[allow(dead_code)]
impl BoatCategory {
    pub fn default_params(&self) -> WatercraftParams {
        match self {
            BoatCategory::Speedboat => WatercraftParams {
                mass: 1400.0,
                forward_thrust: 28000.0,
                reverse_thrust: 9000.0,
                top_speed: 52.0,
                rudder_agility: 1.85,
                longitudinal_drag: 0.18,
                keel_resistance: 4.5,
                buoyancy_stiffness: 42.0,
                buoyancy_damping: 9.5,
                planing_speed: 12.0,
                max_roll_angle: 0.38,
                max_pitch_angle: 0.16,
                nominal_draft: 0.45,
                is_amphibious: false,
            },
            BoatCategory::Cruiser => WatercraftParams {
                mass: 4200.0,
                forward_thrust: 34000.0,
                reverse_thrust: 14000.0,
                top_speed: 32.0,
                rudder_agility: 1.10,
                longitudinal_drag: 0.35,
                keel_resistance: 7.0,
                buoyancy_stiffness: 55.0,
                buoyancy_damping: 14.0,
                planing_speed: 20.0,
                max_roll_angle: 0.22,
                max_pitch_angle: 0.09,
                nominal_draft: 0.85,
                is_amphibious: false,
            },
            BoatCategory::Patrol => WatercraftParams {
                mass: 2200.0,
                forward_thrust: 32000.0,
                reverse_thrust: 12000.0,
                top_speed: 44.0,
                rudder_agility: 1.65,
                longitudinal_drag: 0.22,
                keel_resistance: 5.5,
                buoyancy_stiffness: 48.0,
                buoyancy_damping: 11.0,
                planing_speed: 13.5,
                max_roll_angle: 0.30,
                max_pitch_angle: 0.13,
                nominal_draft: 0.55,
                is_amphibious: false,
            },
            BoatCategory::Inflatable => WatercraftParams {
                mass: 750.0,
                forward_thrust: 16000.0,
                reverse_thrust: 6000.0,
                top_speed: 38.0,
                rudder_agility: 2.20,
                longitudinal_drag: 0.25,
                keel_resistance: 3.8,
                buoyancy_stiffness: 38.0,
                buoyancy_damping: 8.0,
                planing_speed: 9.0,
                max_roll_angle: 0.40,
                max_pitch_angle: 0.18,
                nominal_draft: 0.30,
                is_amphibious: false,
            },
            BoatCategory::Jetski => WatercraftParams {
                mass: 380.0,
                forward_thrust: 15000.0,
                reverse_thrust: 5000.0,
                top_speed: 46.0,
                rudder_agility: 3.10, // Vectoring nozzle allows high angular acceleration
                longitudinal_drag: 0.20,
                keel_resistance: 3.2,
                buoyancy_stiffness: 35.0,
                buoyancy_damping: 7.5,
                planing_speed: 8.0,
                max_roll_angle: 0.45,
                max_pitch_angle: 0.20,
                nominal_draft: 0.25,
                is_amphibious: false,
            },
            BoatCategory::Hovercraft => WatercraftParams {
                mass: 1100.0,
                forward_thrust: 22000.0,
                reverse_thrust: 8000.0,
                top_speed: 40.0,
                rudder_agility: 2.40,
                longitudinal_drag: 0.12, // Air cushion has very low surface friction
                keel_resistance: 0.8,    // Low keel drag allows drift/slides on water & land
                buoyancy_stiffness: 45.0,
                buoyancy_damping: 10.0,
                planing_speed: 0.0, // Always rides cushion
                max_roll_angle: 0.18,
                max_pitch_angle: 0.08,
                nominal_draft: 0.15,
                is_amphibious: true,
            },
            BoatCategory::Seaplane => WatercraftParams {
                mass: 1600.0,
                forward_thrust: 20000.0,
                reverse_thrust: 4000.0,
                top_speed: 48.0,
                rudder_agility: 1.40,
                longitudinal_drag: 0.15,
                keel_resistance: 4.0,
                buoyancy_stiffness: 44.0,
                buoyancy_damping: 9.0,
                planing_speed: 14.0,
                max_roll_angle: 0.25,
                max_pitch_angle: 0.15,
                nominal_draft: 0.40,
                is_amphibious: false,
            },
        }
    }

    pub fn from_game_name(name: &str) -> Self {
        match name {
            "JETSKI" => BoatCategory::Jetski,
            "HOVER" => BoatCategory::Hovercraft,
            "SKIMMER" => BoatCategory::Seaplane,
            "SQUALO" | "JETMAX" | "SPEEDER" | "SPEEDER2" | "SCARAB" => BoatCategory::Speedboat,
            "PREDATR" | "COASTG" => BoatCategory::Patrol,
            "DINGHY" | "YOLA" => BoatCategory::Inflatable,
            _ => BoatCategory::Cruiser, // Tropic, Marquis, Rio, Reefer
        }
    }
}

/// Dynamic aquatic vessel state.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize)]
pub struct Watercraft {
    pub model_id: u16,
    pub name: String,
    pub category: BoatCategory,
    pub position: [f32; 3],
    pub heading: f32,
    pub speed: f32,
    pub lateral_velocity: f32,
    pub vertical_velocity: f32,
    pub pitch: f32,
    pub roll: f32,
    pub rudder_angle: f32,
    pub engine_rpm: f32,
    pub submerged_depth: f32,
    pub in_water: bool,
    pub beached: bool,
    pub wake_intensity: f32,
    pub spray_active: bool,
    pub wave_phase: f32,
    pub health: f32,
    pub exploded: bool,
    pub params: WatercraftParams,
}

#[allow(dead_code)]
impl Watercraft {
    /// Create a new watercraft instance by model ID or name.
    pub fn new(model_id: u16, game_name: &str, position: [f32; 3], heading: f32) -> Self {
        let category = BoatCategory::from_game_name(game_name);
        let params = category.default_params();
        Self {
            model_id,
            name: game_name.to_string(),
            category,
            position,
            heading,
            speed: 0.0,
            lateral_velocity: 0.0,
            vertical_velocity: 0.0,
            pitch: 0.0,
            roll: 0.0,
            rudder_angle: 0.0,
            engine_rpm: 800.0,
            submerged_depth: 0.0,
            in_water: false,
            beached: false,
            wake_intensity: 0.0,
            spray_active: false,
            wave_phase: 0.0,
            health: 1000.0,
            exploded: false,
            params,
        }
    }

    /// Evaluates the dynamic surface water level including 3D tropical waves and swell.
    pub fn wave_elevation(&self, water: &WaterGrid, position: [f32; 3]) -> Option<f32> {
        let base_z = water.height(position)?;
        // Vice City coastal swell & ocean chop: two superimposed sinusoidal waves
        let wave_1 = (position[0] * 0.045 + position[1] * 0.035 + self.wave_phase * 1.8).sin() * 0.32;
        let wave_2 = (position[0] * 0.09 - position[1] * 0.08 - self.wave_phase * 2.6).cos() * 0.14;
        Some(base_z + wave_1 + wave_2)
    }

    /// Primary dynamic physics simulation step for aquatic craft.
    ///
    /// - `throttle`: [-1.0..=1.0] signed input (positive = forward thrust, negative = reverse/braking)
    /// - `steer`: [-1.0..=1.0] signed steering input (positive = turn right, negative = turn left)
    /// - `dt`: simulation delta time (seconds)
    pub fn step(
        &mut self,
        water: &WaterGrid,
        terrain: &impl CollisionQuery,
        throttle: f32,
        steer: f32,
        dt: f32,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let dt = dt.min(0.1);
        self.wave_phase += dt;

        if self.exploded {
            self.speed *= (-2.0 * dt).exp();
            self.lateral_velocity *= (-2.0 * dt).exp();
            self.vertical_velocity = (self.vertical_velocity - 9.81 * dt).max(-15.0);
            self.position[2] += self.vertical_velocity * dt;
            return;
        }

        let throttle = throttle.clamp(-1.0, 1.0);
        let steer = steer.clamp(-1.0, 1.0);

        // 1. Water surface query at current hull location
        let surface_z = self.wave_elevation(water, self.position);

        // 2. Buoyancy & Vertical Dynamics
        const GRAVITY: f32 = 9.81;
        let mut on_solid_ground = false;

        if let Some(water_z) = surface_z {
            let hull_bottom = self.position[2] - self.params.nominal_draft;
            let immersion = water_z - hull_bottom;
            self.submerged_depth = immersion.max(0.0);
            self.in_water = immersion > 0.0;

            if self.in_water {
                // Archimedes hydrostatic upward spring force + damping
                let spring_accel = (immersion * self.params.buoyancy_stiffness).min(35.0);
                let damp_accel = -self.vertical_velocity * self.params.buoyancy_damping;
                let net_vertical_accel = (spring_accel + damp_accel) - GRAVITY;
                self.vertical_velocity += net_vertical_accel * dt;
            } else {
                // Airborne above water (jumping waves)
                self.vertical_velocity -= GRAVITY * dt;
            }
        } else {
            // Not over a water grid cell (over dry ground or outside map bounds)
            self.in_water = false;
            self.submerged_depth = 0.0;
            self.vertical_velocity -= GRAVITY * dt;
        }

        // 3. Ground contact & Beaching check
        let ground_check_rise = 0.4;
        let ground_check_dist = ground_check_rise + self.params.nominal_draft + 0.3;
        if let Some(hit) = terrain.raycast(
            [self.position[0], self.position[1], self.position[2] + ground_check_rise],
            [0.0, 0.0, -1.0],
            ground_check_dist,
        ) {
            let target_z = hit.position[2] + self.params.nominal_draft;
            if self.position[2] <= target_z + 0.05 {
                self.position[2] = target_z;
                self.vertical_velocity = self.vertical_velocity.max(0.0);
                on_solid_ground = true;
                if !self.in_water || hit.position[2] > surface_z.unwrap_or(f32::NEG_INFINITY) {
                    self.beached = true;
                }
            }
        } else {
            self.beached = false;
        }

        self.position[2] += self.vertical_velocity * dt;

        // 4. Longitudinal Propulsion & Hydrodynamic Drag
        let is_operative = self.in_water || (self.params.is_amphibious && on_solid_ground);

        if is_operative {
            // Thrust computation
            let thrust_force = if throttle >= 0.0 {
                throttle * self.params.forward_thrust
            } else if self.speed > 0.5 {
                // Active propeller counter-rotation hydrodynamic braking
                throttle * (self.params.reverse_thrust + self.params.forward_thrust * 0.75)
            } else {
                throttle * self.params.reverse_thrust
            };
            let thrust_accel = thrust_force / self.params.mass;

            // Planing effect: above planing_speed, effective wetted surface drops by up to 45%
            let is_planing = self.speed.abs() > self.params.planing_speed;
            let drag_reduction = if is_planing { 0.55 } else { 1.0 };

            let drag_force = 0.5
                * self.params.longitudinal_drag
                * drag_reduction
                * self.speed
                * self.speed.abs();
            let net_accel = thrust_accel - (drag_force / self.params.mass);
            self.speed += net_accel * dt;

            // Keel lateral resistance: prevents sideslip drift
            let keel_damp = (self.params.keel_resistance * dt).min(0.9);
            self.lateral_velocity *= 1.0 - keel_damp;

            // Engine RPM modeling
            let target_rpm = 850.0 + (self.speed.abs() / self.params.top_speed) * 4500.0
                + throttle.abs() * 1200.0;
            self.engine_rpm += (target_rpm - self.engine_rpm) * (8.0 * dt).min(1.0);
        } else if on_solid_ground {
            // Non-amphibious boat sliding on land: high friction stops the hull
            let ground_friction = 14.0 * dt;
            if self.speed > 0.0 {
                self.speed = (self.speed - ground_friction).max(0.0);
            } else {
                self.speed = (self.speed + ground_friction).min(0.0);
            }
            self.lateral_velocity *= (-12.0 * dt).exp();
            self.engine_rpm += (850.0 - self.engine_rpm) * (5.0 * dt).min(1.0);
        } else {
            // Airborne
            self.speed *= (-0.05 * dt).exp();
            self.engine_rpm += (850.0 - self.engine_rpm) * (2.0 * dt).min(1.0);
        }

        // 5. Steering & Rudder / Jet Vectoring
        self.rudder_angle += (steer - self.rudder_angle) * (12.0 * dt).min(1.0);

        if is_operative {
            let turn_rate = match self.category {
                BoatCategory::Jetski => {
                    // Vectoring nozzle directs jet thrust: turns sharply even from standstill with throttle
                    let thrust_factor = 0.4 + throttle.abs() * 0.6;
                    -steer * self.params.rudder_agility * thrust_factor
                }
                BoatCategory::Hovercraft => {
                    // Air rudders work independently of water contact
                    -steer * self.params.rudder_agility
                }
                _ => {
                    // Rudder deflection generates turning moment proportional to speed through water
                    let speed_ratio = (self.speed / 16.0).clamp(-1.2, 1.2);
                    -steer * self.params.rudder_agility * speed_ratio
                }
            };
            self.heading += turn_rate * dt;

            // Dynamic banking / roll into turns (heeling)
            let target_roll = (self.speed * turn_rate * 0.22).clamp(
                -self.params.max_roll_angle,
                self.params.max_roll_angle,
            );
            self.roll += (target_roll - self.roll) * (8.0 * dt).min(1.0);

            // Dynamic pitch: bow rises under acceleration and hydroplaning
            let target_pitch = if self.speed > 4.0 {
                let speed_ratio = (self.speed / self.params.top_speed).clamp(0.0, 1.0);
                self.params.max_pitch_angle * (0.4 + 0.6 * speed_ratio)
            } else {
                0.0
            };
            self.pitch += (target_pitch - self.pitch) * (6.0 * dt).min(1.0);
        } else {
            // Stabilize roll & pitch toward neutral
            self.roll *= (-3.0 * dt).exp();
            self.pitch *= (-3.0 * dt).exp();
        }

        // Normalize heading to [-PI..PI]
        while self.heading > std::f32::consts::PI {
            self.heading -= 2.0 * std::f32::consts::PI;
        }
        while self.heading < -std::f32::consts::PI {
            self.heading += 2.0 * std::f32::consts::PI;
        }

        // 6. Coordinate displacement
        let (s, c) = self.heading.sin_cos();
        let forward_dx = s * self.speed * dt;
        let forward_dy = c * self.speed * dt;
        let lateral_dx = c * self.lateral_velocity * dt;
        let lateral_dy = -s * self.lateral_velocity * dt;

        self.position[0] += forward_dx + lateral_dx;
        self.position[1] += forward_dy + lateral_dy;

        // 7. Visual Wake & Spray Intensity
        let speed_ratio = (self.speed.abs() / self.params.top_speed).clamp(0.0, 1.0);
        self.wake_intensity = if self.in_water {
            speed_ratio * (0.6 + throttle.abs() * 0.4)
        } else {
            0.0
        };
        self.spray_active = self.in_water && self.speed > 9.0;
    }

    /// Applies collision or combat damage.
    pub fn apply_damage(&mut self, amount: f32) {
        if self.exploded {
            return;
        }
        self.health = (self.health - amount).max(0.0);
        if self.health <= 0.0 {
            self.exploded = true;
            self.speed *= 0.2;
            self.wake_intensity = 0.0;
        }
    }

    /// Restores full health and operational status.
    pub fn repair(&mut self) {
        self.health = 1000.0;
        self.exploded = false;
    }

    /// World coordinates of stern wake emitters (left and right foam trails).
    pub fn wake_trail_emitters(&self) -> [[f32; 3]; 2] {
        let (s, c) = self.heading.sin_cos();
        let stern_dist = -2.2;
        let wake_spread = 0.9 + self.wake_intensity * 0.8;
        let z = self.position[2] - self.params.nominal_draft * 0.5;

        let left = [
            self.position[0] + s * stern_dist - c * wake_spread,
            self.position[1] + c * stern_dist + s * wake_spread,
            z,
        ];
        let right = [
            self.position[0] + s * stern_dist + c * wake_spread,
            self.position[1] + c * stern_dist - s * wake_spread,
            z,
        ];
        [left, right]
    }

    /// World coordinates of bow spray emitters (cutting water).
    pub fn bow_spray_emitters(&self) -> [[f32; 3]; 2] {
        let (s, c) = self.heading.sin_cos();
        let bow_dist = 2.4;
        let bow_width = 0.4;
        let z = self.position[2] + 0.1;

        let left = [
            self.position[0] + s * bow_dist - c * bow_width,
            self.position[1] + c * bow_dist + s * bow_width,
            z,
        ];
        let right = [
            self.position[0] + s * bow_dist + c * bow_width,
            self.position[1] + c * bow_dist - s * bow_width,
            z,
        ];
        [left, right]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FlatSea;
    impl CollisionQuery for FlatSea {
        fn raycast(
            &self,
            _origin: [f32; 3],
            _direction: [f32; 3],
            _max_distance: f32,
        ) -> Option<crate::collision::Hit> {
            None
        }
    }

    struct FlatGround(f32);
    impl CollisionQuery for FlatGround {
        fn raycast(
            &self,
            origin: [f32; 3],
            direction: [f32; 3],
            max_distance: f32,
        ) -> Option<crate::collision::Hit> {
            if direction[2] < 0.0 && origin[2] >= self.0 {
                let dist = origin[2] - self.0;
                if dist <= max_distance {
                    return Some(crate::collision::Hit {
                        position: [origin[0], origin[1], self.0],
                        normal: [0.0, 0.0, 1.0],
                        distance: dist,
                        entity: None,
                        surface: 0,
                        model_id: 1,
                    });
                }
            }
            None
        }
    }

    #[test]
    fn speedboat_floats_at_equilibrium_water_surface() {
        let water = WaterGrid::test_flat(5.0);
        let sea = FlatSea;
        let mut boat = Watercraft::new(257, "SQUALO", [-2300.0, -1800.0, 5.0], 0.0);

        // Run simulation steps to allow buoyancy spring to settle
        for _ in 0..120 {
            boat.step(&water, &sea, 0.0, 0.0, 1.0 / 60.0);
        }

        assert!(boat.in_water);
        assert!(!boat.beached);
        // The vessel settles with its draft slightly below the water plane at hydrostatic equilibrium
        let equilibrium_z = 5.0 + boat.params.nominal_draft - 9.81 / boat.params.buoyancy_stiffness;
        assert!(
            (boat.position[2] - equilibrium_z).abs() < 0.20,
            "Boat altitude {} should be near calculated equilibrium water height {}",
            boat.position[2],
            equilibrium_z
        );
    }

    #[test]
    fn full_throttle_accelerates_and_reaches_planing_speed() {
        let water = WaterGrid::test_flat(0.0);
        let sea = FlatSea;
        let mut boat = Watercraft::new(257, "SQUALO", [-2300.0, -1800.0, 0.0], 0.0);

        // Apply full forward throttle for 3 seconds
        for _ in 0..180 {
            boat.step(&water, &sea, 1.0, 0.0, 1.0 / 60.0);
        }

        assert!(
            boat.speed > boat.params.planing_speed,
            "Squalo speed {} should exceed planing threshold {}",
            boat.speed,
            boat.params.planing_speed
        );
        assert!(boat.wake_intensity > 0.4);
        assert!(boat.pitch > 0.05, "Bow should rise during planing");
        assert!(boat.engine_rpm > 2500.0);
    }

    #[test]
    fn jetski_vectoring_turns_from_standstill() {
        let water = WaterGrid::test_flat(0.0);
        let sea = FlatSea;
        let mut jetski = Watercraft::new(233, "JETSKI", [-2300.0, -1800.0, 0.0], 0.0);

        let initial_heading = jetski.heading;
        // Turn right while applying quick burst of throttle
        for _ in 0..30 {
            jetski.step(&water, &sea, 0.8, 1.0, 1.0 / 60.0);
        }

        assert!(
            (jetski.heading - initial_heading).abs() > 0.25,
            "Jetski vectoring nozzle should produce sharp angular deflection"
        );
        assert!(jetski.roll < 0.0, "Jetski should bank into the right turn");
    }

    #[test]
    fn hovercraft_slides_on_dry_land() {
        let water = WaterGrid::test_flat(-50.0); // No water at elevation 0
        let land = FlatGround(0.0);
        let mut hover = Watercraft::new(198, "HOVER", [-2300.0, -1800.0, 0.2], 0.0);

        // Apply throttle over dry terrain
        for _ in 0..60 {
            hover.step(&water, &land, 1.0, 0.0, 1.0 / 60.0);
        }

        assert!(hover.beached || !hover.in_water);
        assert!(
            hover.speed > 5.0,
            "Amphibious hovercraft should generate forward speed over land, speed = {}",
            hover.speed
        );
    }

    #[test]
    fn beaching_stops_standard_boat_on_shore() {
        let water = WaterGrid::test_flat(-50.0); // Ground rises above water
        let land = FlatGround(2.0);
        let mut boat = Watercraft::new(250, "TROPIC", [-2300.0, -1800.0, 2.0], 0.0);
        boat.speed = 15.0; // Enters shore with initial speed

        // Simulate sliding on ground with no throttle
        for _ in 0..120 {
            boat.step(&water, &land, 0.0, 0.0, 1.0 / 60.0);
        }

        assert!(boat.beached);
        assert!(
            boat.speed < 1.0,
            "Ground friction should arrest non-amphibious boat"
        );
    }

    #[test]
    fn reverse_throttle_acts_as_water_brake() {
        let water = WaterGrid::test_flat(0.0);
        let sea = FlatSea;
        let mut boat = Watercraft::new(219, "PREDATR", [-2300.0, -1800.0, 0.0], 0.0);
        boat.speed = 25.0;
        let initial_speed = boat.speed;

        // Apply full reverse throttle for 1 second
        for _ in 0..60 {
            boat.step(&water, &sea, -1.0, 0.0, 1.0 / 60.0);
        }

        assert!(
            boat.speed < initial_speed - 5.0,
            "Reverse propeller thrust should decelerate the vessel from {}, got {}",
            initial_speed,
            boat.speed
        );
    }

    #[test]
    fn emitters_provide_plausible_world_coordinates() {
        let boat = Watercraft::new(257, "SQUALO", [100.0, 200.0, 2.0], 0.0);
        let wake = boat.wake_trail_emitters();
        let spray = boat.bow_spray_emitters();

        // Emitters should be behind and in front of vessel center
        assert!(wake[0][1] < 200.0 && wake[1][1] < 200.0, "Wake must be astern");
        assert!(spray[0][1] > 200.0 && spray[1][1] > 200.0, "Spray must be ahead");
    }
}
