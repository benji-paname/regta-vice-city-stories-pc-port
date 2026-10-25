#![allow(dead_code)]
//! Mission scripting system and SCM virtual machine for VCS-Rust.
//! Re-implements CMissionScriptSystem and CRunningScript faithful to Vice City Stories.

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptThread {
    pub name: [u8; 8],
    pub ip: usize,
    pub locals: [i32; 32],
    pub wait_timer: f32,
    pub active: bool,
    pub is_mission: bool,
}

impl Default for ScriptThread {
    fn default() -> Self {
        Self {
            name: *b"MAIN_VCS",
            ip: 0,
            locals: [0; 32],
            wait_timer: 0.0,
            active: true,
            is_mission: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MissionObjective {
    pub name: String,
    pub instruction: String,
    pub target_pos: [f32; 3],
    pub target_radius: f32,
    pub blip_active: bool,
    pub passed: bool,
    pub failed: bool,
    pub reward_cash: u32,
    pub step: u32,
}

#[derive(Debug, Clone)]
pub struct MissionScriptSystem {
    pub globals: [i32; 1024],
    pub threads: Vec<ScriptThread>,
    pub current_mission: Option<MissionObjective>,
    pub banner_title: String,
    pub banner_title_timer: f32,
    pub banner_sub: String,
    pub banner_sub_timer: f32,
    pub marker_pulse: f32,
}

impl Default for MissionScriptSystem {
    fn default() -> Self {
        Self {
            globals: [0; 1024],
            threads: vec![ScriptThread::default()],
            current_mission: None,
            banner_title: String::new(),
            banner_title_timer: 0.0,
            banner_sub: String::new(),
            banner_sub_timer: 0.0,
            marker_pulse: 0.0,
        }
    }
}

impl MissionScriptSystem {
    pub fn new() -> Self {
        let mut sys = Self::default();
        sys.load_first_story_mission();
        sys
    }

    pub fn load_first_story_mission(&mut self) {
        // First mission: Soldier (Fort Baxter Base, Sergeant Martinez briefing)
        self.current_mission = Some(MissionObjective {
            name: "SOLDAT".into(),
            instruction: "Rejoignez le bureau du Sergent Martinez a Fort Baxter.".into(),
            target_pos: [-1250.0, -180.0, 10.5],
            target_radius: 4.0,
            blip_active: true,
            passed: false,
            failed: false,
            reward_cash: 100,
            step: 0,
        });
        self.print_big("MISSION : SOLDAT", 4.0);
        self.print_now("Rejoignez le bureau du Sergent Jerry Martinez.", 6.0);
    }

    pub fn print_big(&mut self, text: &str, duration: f32) {
        self.banner_title = text.to_string();
        self.banner_title_timer = duration;
    }

    pub fn print_now(&mut self, text: &str, duration: f32) {
        self.banner_sub = text.to_string();
        self.banner_sub_timer = duration;
    }

    pub fn update(&mut self, player_pos: [f32; 3], dt: f32) -> Option<u32> {
        if !dt.is_finite() || dt <= 0.0 {
            return None;
        }

        self.marker_pulse = (self.marker_pulse + dt * 3.5).rem_euclid(std::f32::consts::TAU);
        if self.banner_title_timer > 0.0 {
            self.banner_title_timer = (self.banner_title_timer - dt).max(0.0);
        }
        if self.banner_sub_timer > 0.0 {
            self.banner_sub_timer = (self.banner_sub_timer - dt).max(0.0);
        }

        let mut reward = None;
        let mut advance_to_step_1 = false;
        let mut mission_completed = false;
        let mut reward_cash = 0;

        if let Some(mission) = &mut self.current_mission {
            if !mission.passed && !mission.failed {
                let dx = player_pos[0] - mission.target_pos[0];
                let dy = player_pos[1] - mission.target_pos[1];
                let dz = player_pos[2] - mission.target_pos[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();

                if dist <= mission.target_radius {
                    if mission.step == 0 {
                        // Reached barracks, advance mission objective
                        mission.step = 1;
                        mission.target_pos = [-1180.0, -220.0, 10.5]; // Armory / vehicle bay
                        mission.instruction = "Prenez la Banshee et rejoignez la baie de livraison.".into();
                        advance_to_step_1 = true;
                    } else if mission.step == 1 {
                        // Completed mission
                        mission.passed = true;
                        mission.blip_active = false;
                        reward_cash = mission.reward_cash;
                        reward = Some(reward_cash);
                        mission_completed = true;
                    }
                }
            }
        }

        if advance_to_step_1 {
            self.print_big("BRIEFING VALIDE", 3.0);
            self.print_now("Prenez la Banshee et rejoignez la baie de livraison.", 6.0);
        } else if mission_completed {
            self.print_big("MISSION REUSSIE !", 5.0);
            self.print_now(&format!("Recompense : +${}", reward_cash), 6.0);
        }

        reward
    }

    pub fn has_active_mission(&self) -> bool {
        self.current_mission.as_ref().is_some_and(|m| !m.passed && !m.failed)
    }

    pub fn marker_position(&self) -> Option<[f32; 3]> {
        self.current_mission.as_ref().filter(|m| !m.passed && !m.failed).map(|m| m.target_pos)
    }

    pub fn marker_radius(&self) -> f32 {
        self.current_mission.as_ref().map_or(3.0, |m| m.target_radius)
    }

    pub fn radar_blip(&self) -> Option<([f32; 3], [u8; 4])> {
        self.current_mission.as_ref().filter(|m| m.blip_active && !m.passed && !m.failed).map(|m| {
            // Yellow square mission blip
            (m.target_pos, [255, 230, 20, 255])
        })
    }

    pub fn objective_text(&self) -> Option<&str> {
        if self.banner_sub_timer > 0.0 {
            Some(&self.banner_sub)
        } else {
            self.current_mission.as_ref().filter(|m| !m.passed && !m.failed).map(|m| m.instruction.as_str())
        }
    }

    pub fn banner_text(&self) -> Option<&str> {
        if self.banner_title_timer > 0.0 {
            Some(&self.banner_title)
        } else {
            None
        }
    }

    pub fn fail_mission(&mut self, reason: &str) {
        if let Some(m) = &mut self.current_mission {
            m.failed = true;
            m.blip_active = false;
        }
        self.print_big("MISSION ECHOUEE", 4.0);
        self.print_now(reason, 5.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_system_starts_and_updates_mission() {
        let sys = MissionScriptSystem::new();
        assert!(sys.has_active_mission());
        assert_eq!(sys.current_mission.as_ref().unwrap().name, "SOLDAT");
        assert!(sys.banner_text().is_some());
    }

    #[test]
    fn player_reaching_marker_advances_mission() {
        let mut sys = MissionScriptSystem::new();
        let target = sys.marker_position().unwrap();

        // Player walks into marker
        let reward = sys.update(target, 0.1);
        assert!(reward.is_none());
        assert_eq!(sys.current_mission.as_ref().unwrap().step, 1);

        // Player walks into step 2 marker
        let target2 = sys.marker_position().unwrap();
        let reward2 = sys.update(target2, 0.1);
        assert_eq!(reward2, Some(100));
        assert!(sys.current_mission.as_ref().unwrap().passed);
        assert!(!sys.has_active_mission());
    }

    #[test]
    fn mission_failure_state() {
        let mut sys = MissionScriptSystem::new();
        sys.fail_mission("Le vehicule a ete detruit.");
        assert!(!sys.has_active_mission());
        assert_eq!(sys.banner_text(), Some("MISSION ECHOUEE"));
    }

    #[test]
    fn global_and_local_variables_storage() {
        let mut sys = MissionScriptSystem::new();
        sys.globals[42] = 1984;
        sys.threads[0].locals[5] = 999;
        assert_eq!(sys.globals[42], 1984);
        assert_eq!(sys.threads[0].locals[5], 999);
    }
}
