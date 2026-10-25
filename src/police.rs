#![allow(dead_code)]
//! Police department and wanted level mechanics for VCS-Rust.
//! Re-implements CWanted and CPolice logic faithful to Vice City Stories.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrimeType {
    FirearmDischarge,
    VehicleTheft,
    PedestrianAssault,
    PedestrianHitAndRun,
    OfficerAssault,
    ExplosionDetonated,
}

impl CrimeType {
    pub fn score(&self) -> u32 {
        match self {
            CrimeType::FirearmDischarge => 50,
            CrimeType::VehicleTheft => 90,
            CrimeType::PedestrianAssault => 140,
            CrimeType::PedestrianHitAndRun => 280,
            CrimeType::OfficerAssault => 450,
            CrimeType::ExplosionDetonated => 600,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WantedSystem {
    pub crime_score: u32,
    pub wanted_level: u32,
    pub flash_timer: f32,
    pub evade_timer: f32,
    pub last_crime_pos: Option<[f32; 3]>,
}

impl Default for WantedSystem {
    fn default() -> Self {
        Self {
            crime_score: 0,
            wanted_level: 0,
            flash_timer: 0.0,
            evade_timer: 0.0,
            last_crime_pos: None,
        }
    }
}

impl WantedSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn report_crime(&mut self, crime: CrimeType, pos: [f32; 3]) {
        self.crime_score = self.crime_score.saturating_add(crime.score());
        self.last_crime_pos = Some(pos);
        self.evade_timer = 0.0;
        self.recompute_level();
    }

    pub fn set_wanted_level(&mut self, level: u32) {
        self.wanted_level = level.clamp(0, 6);
        self.crime_score = match self.wanted_level {
            0 => 0,
            1 => 100,
            2 => 300,
            3 => 650,
            4 => 1300,
            5 => 2600,
            _ => 5200,
        };
        self.evade_timer = 0.0;
    }

    fn recompute_level(&mut self) {
        self.wanted_level = if self.crime_score >= 5000 {
            6
        } else if self.crime_score >= 2500 {
            5
        } else if self.crime_score >= 1200 {
            4
        } else if self.crime_score >= 600 {
            3
        } else if self.crime_score >= 250 {
            2
        } else if self.crime_score >= 90 {
            1
        } else {
            0
        };
    }

    pub fn update(&mut self, dt: f32) {
        if self.wanted_level == 0 {
            self.flash_timer = 0.0;
            self.evade_timer = 0.0;
            return;
        }

        self.flash_timer += dt;
        self.evade_timer += dt;

        // After 18 seconds without committing crimes, evade progresses
        if self.evade_timer >= 18.0 {
            self.evade_timer = 0.0;
            if self.wanted_level > 0 {
                self.wanted_level -= 1;
                self.crime_score = match self.wanted_level {
                    0 => 0,
                    1 => 100,
                    2 => 300,
                    3 => 650,
                    4 => 1300,
                    5 => 2600,
                    _ => 5200,
                };
            }
        }
    }

    pub fn stars_active(&self) -> u32 {
        self.wanted_level
    }

    pub fn is_flashing(&self) -> bool {
        if self.wanted_level == 0 {
            return false;
        }
        // Flashes when evading (evade_timer > 3.0s)
        if self.evade_timer > 3.0 {
            (self.flash_timer * 3.3).fract() > 0.5
        } else {
            false
        }
    }

    pub fn clear(&mut self) {
        self.crime_score = 0;
        self.wanted_level = 0;
        self.flash_timer = 0.0;
        self.evade_timer = 0.0;
        self.last_crime_pos = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crime_increases_wanted_score_and_stars() {
        let mut police = WantedSystem::new();
        assert_eq!(police.stars_active(), 0);

        police.report_crime(CrimeType::FirearmDischarge, [100.0, 200.0, 5.0]);
        assert_eq!(police.crime_score, 50);
        assert_eq!(police.stars_active(), 0);

        police.report_crime(CrimeType::VehicleTheft, [100.0, 200.0, 5.0]);
        assert_eq!(police.crime_score, 140);
        assert_eq!(police.stars_active(), 1);

        police.report_crime(CrimeType::PedestrianHitAndRun, [100.0, 200.0, 5.0]);
        assert_eq!(police.crime_score, 420);
        assert_eq!(police.stars_active(), 2);
    }

    #[test]
    fn major_felony_instantly_triggers_three_stars() {
        let mut police = WantedSystem::new();
        police.report_crime(CrimeType::ExplosionDetonated, [0.0, 0.0, 0.0]);
        police.report_crime(CrimeType::OfficerAssault, [0.0, 0.0, 0.0]);
        assert!(police.crime_score >= 1000);
        assert!(police.stars_active() >= 3);
    }

    #[test]
    fn evading_decreases_wanted_level_over_time() {
        let mut police = WantedSystem::new();
        police.set_wanted_level(2);
        assert_eq!(police.stars_active(), 2);

        // Advance 19 seconds of quiet evasion
        police.update(19.0);
        assert_eq!(police.stars_active(), 1);

        // Advance another 19 seconds
        police.update(19.0);
        assert_eq!(police.stars_active(), 0);
    }

    #[test]
    fn flashing_toggles_during_evasion() {
        let mut police = WantedSystem::new();
        police.set_wanted_level(1);
        police.update(1.0);
        assert!(!police.is_flashing());

        police.update(4.0); // Total evade_timer = 5.0 > 3.0
        // flash_timer is running, test fractional state
        police.flash_timer = 0.2; // 0.2 * 3.3 = 0.66 > 0.5 -> flashing
        assert!(police.is_flashing());
        police.flash_timer = 0.1; // 0.1 * 3.3 = 0.33 <= 0.5 -> not flashing
        assert!(!police.is_flashing());
    }
}
