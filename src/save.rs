//! Serialization and quick-save state persistence for VCS-Rust.
//! Re-implements CGenericGameStorage logic faithful to Vice City Stories.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSaveState {
    pub player_pos: [f32; 3],
    pub player_yaw: f32,
    pub player_pitch: f32,
    pub health: u32,
    pub armor: u32,
    pub money: u32,
    pub wanted_level: u32,
    pub current_weapon: usize,
    pub ammo: [u32; 9],
    pub hour: u32,
    pub minute: u32,
    pub car_pos: [f32; 3],
    pub car_heading: f32,
    pub car_health: f32,
}

impl Default for GameSaveState {
    fn default() -> Self {
        Self {
            player_pos: [0.0, 0.0, 5.0],
            player_yaw: 0.0,
            player_pitch: 0.0,
            health: 100,
            armor: 0,
            money: 1000,
            wanted_level: 0,
            current_weapon: 1, // Pistol
            ammo: [999, 120, 300, 48, 250, 20, 10, 15, 200],
            hour: 12,
            minute: 0,
            car_pos: [0.0, 0.0, 5.0],
            car_heading: 0.0,
            car_health: 1000.0,
        }
    }
}

impl GameSaveState {
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(path, json)
    }

    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let content = fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_save_state_roundtrips_to_json() {
        let state = GameSaveState {
            player_pos: [142.5, -350.2, 12.8],
            player_yaw: 1.57,
            player_pitch: -0.1,
            health: 85,
            armor: 50,
            money: 25400,
            wanted_level: 2,
            current_weapon: 3,
            ammo: [999, 90, 250, 32, 200, 15, 8, 10, 150],
            hour: 20,
            minute: 45,
            car_pos: [140.0, -345.0, 12.5],
            car_heading: 0.78,
            car_health: 750.0,
        };

        let temp_path = std::env::temp_dir().join("test_vcs_save.json");
        state.save_to_file(&temp_path).expect("Failed to save state");
        let loaded = GameSaveState::load_from_file(&temp_path).expect("Failed to load state");
        let _ = fs::remove_file(&temp_path);

        assert_eq!(state, loaded);
    }

    #[test]
    fn invalid_save_file_returns_error() {
        let temp_path = std::env::temp_dir().join("invalid_vcs_save.json");
        fs::write(&temp_path, "{ corrupt_json: true }").unwrap();
        let res = GameSaveState::load_from_file(&temp_path);
        let _ = fs::remove_file(&temp_path);
        assert!(res.is_err());
    }
}
