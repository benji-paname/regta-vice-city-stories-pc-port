#![allow(dead_code)]
//! Tire friction model based on Pacejka Magic Formula for VCS-Rust.
//! Computes longitudinal grip (traction/braking), lateral cornering force,
//! slip angles, and dynamic skidmark triggers across diverse surfaces.

pub const DEFAULT_PEAK_FRICTION: f32 = 1.15;
pub const SKID_SLIP_ANGLE_THRESHOLD: f32 = 0.28; // ~16 degrees
pub const SKID_SLIP_RATIO_THRESHOLD: f32 = 0.32; // 32% wheel spin / lock

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CTireFrictionModel {
    pub peak_friction: f32,
    pub stiffness_b: f32,
    pub shape_c: f32,
    pub curvature_e: f32,
    pub current_wear: f32,
    pub tire_temperature: f32,
    pub skidmark_active: bool,
}

impl Default for CTireFrictionModel {
    fn default() -> Self {
        Self {
            peak_friction: DEFAULT_PEAK_FRICTION,
            stiffness_b: 10.0,
            shape_c: 1.65,
            curvature_e: -0.95,
            current_wear: 0.0,
            tire_temperature: 25.0, // Celsius
            skidmark_active: false,
        }
    }
}

impl CTireFrictionModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates the Pacejka Magic Formula curve:
    /// y(x) = D * sin(C * atan(B * x - E * (B * x - atan(B * x))))
    pub fn pacejka_curve(&self, slip: f32, peak_d: f32) -> f32 {
        let bx = self.stiffness_b * slip;
        let inner = bx - self.curvature_e * (bx - bx.atan());
        peak_d * (self.shape_c * inner.atan()).sin()
    }

    /// Computes longitudinal traction/braking force (Fx) and lateral cornering force (Fy).
    pub fn compute_friction_forces(
        &self,
        normal_load: f32,
        slip_ratio: f32,
        slip_angle_rad: f32,
        surface_friction: f32,
    ) -> (f32, f32) {
        if normal_load <= 0.0 {
            return (0.0, 0.0);
        }

        let effective_mu = self.peak_friction * surface_friction.clamp(0.1, 1.5);
        let peak_force = normal_load * effective_mu;

        // Longitudinal force Fx
        let raw_fx = self.pacejka_curve(slip_ratio, peak_force);

        // Lateral force Fy (cornering)
        let raw_fy = self.pacejka_curve(slip_angle_rad, peak_force);

        // Combined slip friction ellipse: sqrt((Fx/Fx_max)^2 + (Fy/Fy_max)^2) <= 1
        let fx_norm = raw_fx / (peak_force + 1e-4);
        let fy_norm = raw_fy / (peak_force + 1e-4);
        let total_slip = (fx_norm * fx_norm + fy_norm * fy_norm).sqrt();

        if total_slip > 1.0 {
            (raw_fx / total_slip, raw_fy / total_slip)
        } else {
            (raw_fx, raw_fy)
        }
    }

    pub fn should_emit_skidmark(&self, slip_ratio: f32, slip_angle_rad: f32) -> bool {
        slip_ratio.abs() > SKID_SLIP_RATIO_THRESHOLD || slip_angle_rad.abs() > SKID_SLIP_ANGLE_THRESHOLD
    }

    pub fn update(&mut self, slip_ratio: f32, slip_angle_rad: f32, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        self.skidmark_active = self.should_emit_skidmark(slip_ratio, slip_angle_rad);

        if self.skidmark_active {
            self.tire_temperature = (self.tire_temperature + dt * 45.0).min(180.0);
            self.current_wear = (self.current_wear + dt * 0.005).min(1.0);
        } else {
            // Cool down
            self.tire_temperature = (self.tire_temperature - dt * 8.0).max(25.0);
        }
    }

    pub fn peak_friction_coefficient(&self) -> f32 {
        self.peak_friction * (1.0 - 0.20 * self.current_wear)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pacejka_curve_produces_smooth_traction_force() {
        let model = CTireFrictionModel::new();
        let f_low = model.pacejka_curve(0.05, 1000.0);
        let f_peak = model.pacejka_curve(0.15, 1000.0);
        assert!(f_low > 0.0);
        assert!(f_peak >= f_low);
        assert!(f_peak <= 1000.0);
    }

    #[test]
    fn combined_slip_respects_friction_circle() {
        let model = CTireFrictionModel::new();
        let (fx, fy) = model.compute_friction_forces(3500.0, 0.8, 0.7, 1.0);
        let max_possible = 3500.0 * model.peak_friction;
        let total_force = (fx * fx + fy * fy).sqrt();
        assert!(total_force <= max_possible + 1e-3, "Total force cannot exceed normal * mu");
    }

    #[test]
    fn skidmark_detects_burnout_and_handbrake_drift() {
        let mut model = CTireFrictionModel::new();
        assert!(!model.should_emit_skidmark(0.02, 0.02));

        // Power burnout
        assert!(model.should_emit_skidmark(0.55, 0.0));

        // High slip angle drift
        assert!(model.should_emit_skidmark(0.05, 0.45));

        model.update(0.60, 0.0, 0.1);
        assert!(model.skidmark_active);
        assert!(model.tire_temperature > 25.0);
    }

    #[test]
    fn wet_surface_reduces_peak_forces() {
        let model = CTireFrictionModel::new();
        let (fx_dry, _) = model.compute_friction_forces(3000.0, 0.2, 0.0, 1.0);
        let (fx_wet, _) = model.compute_friction_forces(3000.0, 0.2, 0.0, 0.7);
        assert!(fx_wet < fx_dry, "Wet road must lower available traction");
    }
}
