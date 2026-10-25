//! Gameplay additions are opt-in. This switches controller features, not the
//! underlying disc edition; original-mode fidelity remains work in progress.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Gameplay {
    #[default]
    OriginalPs2,
    Modern,
}
impl Gameplay {
    pub fn label(self) -> &'static str {
        match self {
            Self::OriginalPs2 => "Original PS2",
            Self::Modern => "Moderne",
        }
    }
    pub fn modern(self) -> bool {
        self == Self::Modern
    }
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub gameplay: Gameplay,
}
impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes_round_trip_and_default_is_original() {
        assert!(!Settings::default().gameplay.modern());
        for gameplay in [Gameplay::OriginalPs2, Gameplay::Modern] {
            let bytes = serde_json::to_vec(&Settings { gameplay }).unwrap();
            assert_eq!(
                serde_json::from_slice::<Settings>(&bytes).unwrap().gameplay,
                gameplay
            );
        }
        assert_eq!(
            serde_json::from_str::<Settings>("{}").unwrap().gameplay,
            Gameplay::OriginalPs2
        );
    }
}
