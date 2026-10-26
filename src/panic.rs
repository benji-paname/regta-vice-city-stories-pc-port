#![allow(dead_code)]
//! Pedestrian panic reactions, crowd threat fleeing, and stampede AI for VCS-Rust.
//! Implements CPedestrianPanicReaction.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanicCause {
    Gunfire,
    Explosion,
    VehicleThreat,
    WeatherStorm,
}

#[derive(Clone, Debug)]
pub struct CPedestrianPanicReaction {
    pub is_panicking: bool,
    pub panic_cause: PanicCause,
    pub threat_origin: [f32; 3],
    pub threat_radius: f32,
    pub panic_timer: f32,
    pub max_duration: f32,
    pub flee_direction: [f32; 3],
    pub scream_triggered: bool,
}

impl Default for CPedestrianPanicReaction {
    fn default() -> Self {
        Self {
            is_panicking: false,
            panic_cause: PanicCause::Gunfire,
            threat_origin: [0.0; 3],
            threat_radius: 25.0,
            panic_timer: 0.0,
            max_duration: 8.0,
            flee_direction: [0.0, 0.0, 0.0],
            scream_triggered: false,
        }
    }
}

impl CPedestrianPanicReaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn trigger_threat(
        &mut self,
        ped_pos: [f32; 3],
        threat_pos: [f32; 3],
        cause: PanicCause,
        radius: f32,
        duration: f32,
    ) -> bool {
        let dx = ped_pos[0] - threat_pos[0];
        let dy = ped_pos[1] - threat_pos[1];
        let dz = ped_pos[2] - threat_pos[2];
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist <= radius {
            self.is_panicking = true;
            self.panic_cause = cause;
            self.threat_origin = threat_pos;
            self.threat_radius = radius;
            self.panic_timer = duration;
            self.max_duration = duration;
            self.scream_triggered = true;

            // Direct flee vector away from threat
            let safe_dist = dist.max(0.1);
            self.flee_direction = [dx / safe_dist, dy / safe_dist, 0.0];
            return true;
        }

        false
    }

    pub fn update(&mut self, ped_pos: [f32; 3], dt: f32) -> Option<[f32; 3]> {
        if !self.is_panicking || !dt.is_finite() || dt <= 0.0 {
            return None;
        }

        self.panic_timer = (self.panic_timer - dt).max(0.0);
        if self.panic_timer <= 0.0 {
            self.is_panicking = false;
            self.scream_triggered = false;
            return None;
        }

        // Dynamically maintain flee heading away from threat
        let dx = ped_pos[0] - self.threat_origin[0];
        let dy = ped_pos[1] - self.threat_origin[1];
        let dist = (dx * dx + dy * dy).sqrt().max(0.1);

        self.flee_direction = [dx / dist, dy / dist, 0.0];
        Some(self.flee_direction)
    }

    pub fn panic_speed_multiplier(&self) -> f32 {
        if self.is_panicking {
            2.4 // Sprinting flee speed
        } else {
            1.0
        }
    }

    pub fn should_scream(&mut self) -> bool {
        if self.scream_triggered {
            self.scream_triggered = false;
            true
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.is_panicking = false;
        self.panic_timer = 0.0;
        self.scream_triggered = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_explosion_triggers_panic_and_flee_vector() {
        let mut reaction = CPedestrianPanicReaction::new();
        let ped = [10.0, 0.0, 0.0];
        let explosion = [0.0, 0.0, 0.0];

        let triggered = reaction.trigger_threat(ped, explosion, PanicCause::Explosion, 30.0, 6.0);
        assert!(triggered);
        assert!(reaction.is_panicking);
        assert!(reaction.flee_direction[0] > 0.9, "Should flee in positive X direction");
        assert_eq!(reaction.panic_speed_multiplier(), 2.4);
    }

    #[test]
    fn distant_gunfire_does_not_trigger_panic() {
        let mut reaction = CPedestrianPanicReaction::new();
        let ped = [150.0, 200.0, 0.0];
        let gunfire = [0.0, 0.0, 0.0];

        let triggered = reaction.trigger_threat(ped, gunfire, PanicCause::Gunfire, 40.0, 5.0);
        assert!(!triggered);
        assert!(!reaction.is_panicking);
    }

    #[test]
    fn panic_recovers_after_duration_expires() {
        let mut reaction = CPedestrianPanicReaction::new();
        reaction.trigger_threat([5.0, 5.0, 0.0], [0.0, 0.0, 0.0], PanicCause::VehicleThreat, 20.0, 1.0);
        assert!(reaction.is_panicking);

        reaction.update([10.0, 10.0, 0.0], 1.2);
        assert!(!reaction.is_panicking, "Panic should expire");
    }
}
