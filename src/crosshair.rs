#![allow(dead_code)]
//! Weapon crosshair HUD, dynamic spread reticles, and optical sights for VCS-Rust.
//! Implements CWeaponCrosshairHUD.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReticleStyle {
    StandardCross,
    ShotgunCircle,
    RocketLock,
    SniperMilDot,
    None,
}

#[derive(Clone, Debug)]
pub struct CWeaponCrosshairHUD {
    pub style: ReticleStyle,
    pub base_gap: f32,
    pub current_spread: f32,
    pub target_spread: f32,
    pub range_meters: f32,
    pub reticle_color: [u8; 4],
    pub is_locked: bool,
}

impl Default for CWeaponCrosshairHUD {
    fn default() -> Self {
        Self {
            style: ReticleStyle::StandardCross,
            base_gap: 6.0,
            current_spread: 6.0,
            target_spread: 6.0,
            range_meters: 45.0,
            reticle_color: [0, 240, 255, 230], // Vice City cyan
            is_locked: false,
        }
    }
}

impl CWeaponCrosshairHUD {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_weapon(&mut self, weapon_index: usize) {
        match weapon_index {
            1 | 2 | 4 => {
                // Pistol, SMG, Assault Rifle
                self.style = ReticleStyle::StandardCross;
                self.base_gap = 6.0;
                self.reticle_color = [0, 240, 255, 230];
            }
            3 => {
                // Shotgun
                self.style = ReticleStyle::ShotgunCircle;
                self.base_gap = 18.0;
                self.reticle_color = [255, 230, 40, 230];
            }
            5 => {
                // Sniper
                self.style = ReticleStyle::SniperMilDot;
                self.base_gap = 0.0;
                self.reticle_color = [50, 255, 50, 240];
            }
            6 => {
                // Rocket
                self.style = ReticleStyle::RocketLock;
                self.base_gap = 22.0;
                self.reticle_color = [255, 50, 50, 230];
            }
            _ => {
                self.style = ReticleStyle::None;
            }
        }
    }

    pub fn update_spread(&mut self, is_firing: bool, is_moving: bool, dt: f32) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }

        let mut desired = self.base_gap;
        if is_moving {
            desired += 8.0;
        }
        if is_firing {
            desired += 16.0;
        }

        self.target_spread = desired;
        // Smoothly interpolate current spread
        self.current_spread += (self.target_spread - self.current_spread) * (1.0 - (-12.0 * dt).exp());
    }

    pub fn set_range_distance(&mut self, distance_m: f32) {
        self.range_meters = distance_m.clamp(1.0, 999.0);
    }

    pub fn reticle_dimensions(&self) -> (f32, f32) {
        match self.style {
            ReticleStyle::StandardCross => (self.current_spread, 14.0),
            ReticleStyle::ShotgunCircle => (self.current_spread * 1.5, self.current_spread * 1.5),
            ReticleStyle::RocketLock => (32.0, 32.0),
            ReticleStyle::SniperMilDot => (280.0, 280.0),
            ReticleStyle::None => (0.0, 0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_selection_sets_appropriate_reticle() {
        let mut hud = CWeaponCrosshairHUD::new();
        hud.set_weapon(1);
        assert_eq!(hud.style, ReticleStyle::StandardCross);

        hud.set_weapon(3);
        assert_eq!(hud.style, ReticleStyle::ShotgunCircle);

        hud.set_weapon(5);
        assert_eq!(hud.style, ReticleStyle::SniperMilDot);

        hud.set_weapon(6);
        assert_eq!(hud.style, ReticleStyle::RocketLock);
    }

    #[test]
    fn spread_expands_when_firing_and_moving() {
        let mut hud = CWeaponCrosshairHUD::new();
        hud.set_weapon(1);
        let initial = hud.current_spread;

        hud.update_spread(true, true, 0.1);
        assert!(hud.current_spread > initial, "Spread must bloom when moving and firing");
    }

    #[test]
    fn rangefinder_clamps_to_valid_distances() {
        let mut hud = CWeaponCrosshairHUD::new();
        hud.set_range_distance(-50.0);
        assert_eq!(hud.range_meters, 1.0);
        hud.set_range_distance(1500.0);
        assert_eq!(hud.range_meters, 999.0);
    }
}
