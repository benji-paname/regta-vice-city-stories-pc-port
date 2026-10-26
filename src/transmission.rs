#![allow(dead_code)]
//! Transmission dynamics and gearbox simulation for VCS-Rust vehicles.
//! Provides 5 forward speeds + reverse, optimal torque transfer, shift points,
//! and realistic engine RPM curve.

pub const IDLE_RPM: f32 = 850.0;
pub const REDLINE_RPM: f32 = 6800.0;
pub const MAX_RPM: f32 = 7400.0;
pub const SHIFT_UP_RPM: f32 = 6100.0;
pub const SHIFT_DOWN_RPM: f32 = 2300.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CTransmissionDynamics {
    pub current_gear: i8, // -1: Rev, 0: N, 1..5: Forward
    pub gear_ratios: [f32; 6], // [Rev, 1st, 2nd, 3rd, 4th, 5th]
    pub final_drive: f32,
    pub rpm: f32,
    pub target_rpm: f32,
    pub shift_timer: f32,
    pub clutch_engaged: bool,
}

impl Default for CTransmissionDynamics {
    fn default() -> Self {
        Self {
            current_gear: 1,
            gear_ratios: [3.50, 3.82, 2.15, 1.45, 1.00, 0.78],
            final_drive: 3.42,
            rpm: IDLE_RPM,
            target_rpm: IDLE_RPM,
            shift_timer: 0.0,
            clutch_engaged: true,
        }
    }
}

impl CTransmissionDynamics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn gear_ratio(&self) -> f32 {
        match self.current_gear {
            -1 => self.gear_ratios[0],
            1 => self.gear_ratios[1],
            2 => self.gear_ratios[2],
            3 => self.gear_ratios[3],
            4 => self.gear_ratios[4],
            5 => self.gear_ratios[5],
            _ => 1.0,
        }
    }

    pub fn torque_multiplier(&self) -> f32 {
        if !self.clutch_engaged {
            return 0.0;
        }
        self.gear_ratio() * self.final_drive
    }

    pub fn is_at_redline(&self) -> bool {
        self.rpm >= REDLINE_RPM
    }

    pub fn shift_up(&mut self) -> bool {
        if self.current_gear >= 1 && self.current_gear < 5 && self.shift_timer <= 0.0 {
            self.current_gear += 1;
            self.shift_timer = 0.18; // Shift delay
            self.clutch_engaged = false;
            return true;
        }
        false
    }

    pub fn shift_down(&mut self) -> bool {
        if self.current_gear > 1 && self.shift_timer <= 0.0 {
            self.current_gear -= 1;
            self.shift_timer = 0.18;
            self.clutch_engaged = false;
            return true;
        }
        false
    }

    pub fn update(&mut self, speed_ms: f32, throttle: f32, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        if self.shift_timer > 0.0 {
            self.shift_timer = (self.shift_timer - dt).max(0.0);
            if self.shift_timer <= 0.0 {
                self.clutch_engaged = true;
            }
        }

        let speed = speed_ms.abs();

        // Direction / Gear selection
        if speed_ms < -0.2 && throttle < 0.0 {
            self.current_gear = -1;
        } else if self.current_gear == -1 && speed_ms >= 0.0 {
            self.current_gear = 1;
        }

        if self.current_gear >= 1 && self.shift_timer <= 0.0 {
            // Forward automatic shifting
            if self.rpm > SHIFT_UP_RPM && self.current_gear < 5 && speed > 5.0 {
                self.shift_up();
            } else if self.rpm < SHIFT_DOWN_RPM && self.current_gear > 1 && speed < 6.0 {
                self.shift_down();
            }
        }

        // Calculate expected RPM from speed and current gear ratio
        // Wheel diameter ~ 0.65m => Circumference ~ 2.04m
        let wheel_rot_rps = speed / 2.04;
        let drive_rpm = wheel_rot_rps * 60.0 * self.torque_multiplier();

        let base_rpm = if throttle.abs() > 0.01 {
            IDLE_RPM + throttle.abs() * 1200.0
        } else {
            IDLE_RPM
        };

        self.target_rpm = (drive_rpm + base_rpm * 0.3).clamp(IDLE_RPM, MAX_RPM);

        // Smooth transition
        let smoothing = if self.clutch_engaged { 14.0 } else { 6.0 };
        self.rpm += (self.target_rpm - self.rpm) * (1.0 - (-smoothing * dt).exp());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmission_starts_at_first_gear_and_idle_rpm() {
        let trans = CTransmissionDynamics::new();
        assert_eq!(trans.current_gear, 1);
        assert_eq!(trans.rpm, IDLE_RPM);
        assert!(trans.torque_multiplier() > 10.0);
    }

    #[test]
    fn transmission_upshifts_as_speed_increases() {
        let mut trans = CTransmissionDynamics::new();
        // Accelerate through gear 1 up to high speed
        for speed in [5.0, 15.0, 25.0, 40.0, 55.0] {
            for _ in 0..60 {
                trans.update(speed, 1.0, 0.016);
            }
        }
        assert!(trans.current_gear >= 3, "Gear should have upshifted under speed");
        assert!(trans.rpm > IDLE_RPM);
    }

    #[test]
    fn reverse_gear_selected_on_backward_motion() {
        let mut trans = CTransmissionDynamics::new();
        trans.update(-5.0, -1.0, 0.1);
        assert_eq!(trans.current_gear, -1);
        assert!(trans.torque_multiplier() > 0.0);
    }

    #[test]
    fn clutch_disengages_during_shift() {
        let mut trans = CTransmissionDynamics::new();
        assert!(trans.shift_up());
        assert!(!trans.clutch_engaged);
        assert_eq!(trans.torque_multiplier(), 0.0);
        trans.update(20.0, 1.0, 0.25);
        assert!(trans.clutch_engaged);
    }
}
