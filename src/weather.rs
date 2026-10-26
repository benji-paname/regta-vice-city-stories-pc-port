#![allow(dead_code)]
//! Dynamic weather system and tropical climate simulation for Vice City Stories.
//! Implements 3D rain streaks, storm lightning flashes, wet road dynamics,
//! fog modulation, and weather cycle management.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherType {
    Sunny,
    Cloudy,
    Rainy,
    Thunderstorm,
    Foggy,
    SynthwaveSunset,
}

impl WeatherType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Sunny => "Plein Soleil",
            Self::Cloudy => "Nuageux",
            Self::Rainy => "Pluie Tropicale",
            Self::Thunderstorm => "Orage Tropical",
            Self::Foggy => "Brume Maritime",
            Self::SynthwaveSunset => "Coucher de Soleil 1984",
        }
    }

    pub fn to_index(&self) -> usize {
        match self {
            Self::Sunny => 0,
            Self::Cloudy => 1,
            Self::Rainy => 2,
            Self::Thunderstorm => 3,
            Self::Foggy => 4,
            Self::SynthwaveSunset => 5,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx % 6 {
            0 => Self::Sunny,
            1 => Self::Cloudy,
            2 => Self::Rainy,
            3 => Self::Thunderstorm,
            4 => Self::Foggy,
            5 => Self::SynthwaveSunset,
            _ => Self::Sunny,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RainStreak3D {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub length: f32,
    pub alpha: f32,
}

#[derive(Clone, Debug)]
pub struct WeatherSystem {
    pub current: WeatherType,
    pub target: WeatherType,
    pub transition_timer: f32,
    pub rain_density: f32,
    pub wetness: f32,
    pub fog_density: f32,
    pub wind: [f32; 2],
    pub lightning_timer: f32,
    pub lightning_flash: f32,
    pub lightning_strike_pos: Option<[f32; 3]>,
    pub thunder_event: bool,
    pub rain_streaks: Vec<RainStreak3D>,
}

impl Default for WeatherSystem {
    fn default() -> Self {
        Self {
            current: WeatherType::Sunny,
            target: WeatherType::Sunny,
            transition_timer: 0.0,
            rain_density: 0.0,
            wetness: 0.0,
            fog_density: 0.001,
            wind: [1.5, 0.5],
            lightning_timer: 6.0,
            lightning_flash: 0.0,
            lightning_strike_pos: None,
            thunder_event: false,
            rain_streaks: Vec::with_capacity(256),
        }
    }
}

impl WeatherSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_weather(&mut self, weather: WeatherType) {
        self.current = weather;
        self.target = weather;
        self.transition_timer = 0.0;
        self.apply_target_properties();
    }

    pub fn cycle_next(&mut self) {
        let next_idx = (self.current.to_index() + 1) % 6;
        self.set_weather(WeatherType::from_index(next_idx));
    }

    pub fn is_raining(&self) -> bool {
        matches!(self.current, WeatherType::Rainy | WeatherType::Thunderstorm)
    }

    pub fn is_storming(&self) -> bool {
        self.current == WeatherType::Thunderstorm
    }

    fn apply_target_properties(&mut self) {
        match self.current {
            WeatherType::Sunny => {
                self.rain_density = 0.0;
                self.fog_density = 0.0008;
                self.wind = [1.0, 0.2];
            }
            WeatherType::Cloudy => {
                self.rain_density = 0.0;
                self.fog_density = 0.0018;
                self.wind = [2.0, 1.0];
            }
            WeatherType::Rainy => {
                self.rain_density = 0.85;
                self.fog_density = 0.0035;
                self.wind = [4.5, -2.0];
            }
            WeatherType::Thunderstorm => {
                self.rain_density = 1.0;
                self.fog_density = 0.0055;
                self.wind = [8.0, -5.0];
            }
            WeatherType::Foggy => {
                self.rain_density = 0.05;
                self.fog_density = 0.0120;
                self.wind = [0.5, 0.2];
            }
            WeatherType::SynthwaveSunset => {
                self.rain_density = 0.0;
                self.fog_density = 0.0012;
                self.wind = [1.2, 0.4];
            }
        }
    }

    pub fn update(&mut self, player_pos: [f32; 3], dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0.0 {
            return false;
        }

        self.thunder_event = false;

        // Wetness accumulation & evaporation
        if self.is_raining() {
            self.wetness = (self.wetness + dt * 0.15).min(1.0);
        } else {
            self.wetness = (self.wetness - dt * 0.03).max(0.0);
        }

        // Lightning in thunderstorms
        if self.is_storming() {
            self.lightning_timer -= dt;
            if self.lightning_timer <= 0.0 {
                // Lightning trigger
                self.lightning_flash = 1.0;
                self.lightning_timer = 5.0 + (player_pos[0].abs() * 0.01).sin().abs() * 8.0;
                self.thunder_event = true;
                self.lightning_strike_pos = Some([
                    player_pos[0] + 120.0 * (self.lightning_timer * 3.1).cos(),
                    player_pos[1] + 120.0 * (self.lightning_timer * 3.1).sin(),
                    player_pos[2] + 45.0,
                ]);
            }
        } else {
            self.lightning_flash = 0.0;
            self.lightning_strike_pos = None;
        }

        if self.lightning_flash > 0.0 {
            self.lightning_flash = (self.lightning_flash - dt * 4.0).max(0.0);
        }

        // Rain streak generation and animation
        if self.rain_density > 0.05 {
            let target_count = (self.rain_density * 220.0) as usize;
            while self.rain_streaks.len() < target_count {
                // Spawn new rain streak in bounding volume around player
                let rx = (self.rain_streaks.len() as f32 * 17.3 + player_pos[0]).sin() * 25.0;
                let ry = (self.rain_streaks.len() as f32 * 23.7 + player_pos[1]).cos() * 25.0;
                let rz = ((self.rain_streaks.len() as f32 * 31.1).fract() * 20.0) + 2.0;

                self.rain_streaks.push(RainStreak3D {
                    position: [player_pos[0] + rx, player_pos[1] + ry, player_pos[2] + rz],
                    velocity: [self.wind[0], self.wind[1], -32.0],
                    length: 0.9 + 0.4 * self.rain_density,
                    alpha: 0.4 + 0.3 * self.rain_density,
                });
            }

            // Update streak positions
            for streak in &mut self.rain_streaks {
                streak.position[0] += streak.velocity[0] * dt;
                streak.position[1] += streak.velocity[1] * dt;
                streak.position[2] += streak.velocity[2] * dt;

                // Wrap around player volume
                if streak.position[2] < player_pos[2] - 3.0 {
                    streak.position[2] = player_pos[2] + 18.0;
                    streak.position[0] = player_pos[0] + ((streak.position[0] * 3.7).sin() * 25.0);
                    streak.position[1] = player_pos[1] + ((streak.position[1] * 5.3).cos() * 25.0);
                }
            }
        } else {
            self.rain_streaks.clear();
        }

        self.thunder_event
    }

    /// Modulate sky colors and fog according to current weather and lightning flashes.
    pub fn modulate_atmosphere(
        &self,
        base_top: [f32; 3],
        base_mid: [f32; 3],
        base_bot: [f32; 3],
        base_fog: [f32; 4],
    ) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 4]) {
        let (mut top, mut mid, mut bot, mut fog) = (base_top, base_mid, base_bot, base_fog);

        match self.current {
            WeatherType::Sunny => {
                // Keep base tropical sky
            }
            WeatherType::Cloudy => {
                // Overcast desaturation and slight graying
                for i in 0..3 {
                    top[i] = top[i] * 0.75 + 0.15;
                    mid[i] = mid[i] * 0.70 + 0.20;
                    bot[i] = bot[i] * 0.80 + 0.10;
                    fog[i] = fog[i] * 0.70 + 0.20;
                }
            }
            WeatherType::Rainy => {
                // Dark rainy sky
                for i in 0..3 {
                    top[i] = top[i] * 0.50 + 0.10;
                    mid[i] = mid[i] * 0.50 + 0.12;
                    bot[i] = bot[i] * 0.55 + 0.08;
                    fog[i] = fog[i] * 0.55 + 0.15;
                }
            }
            WeatherType::Thunderstorm => {
                // Menacing dark storm clouds
                for i in 0..3 {
                    top[i] = top[i] * 0.25 + 0.05;
                    mid[i] = mid[i] * 0.30 + 0.08;
                    bot[i] = bot[i] * 0.35 + 0.06;
                    fog[i] = fog[i] * 0.35 + 0.10;
                }

                // Add lightning flash illumination
                if self.lightning_flash > 0.01 {
                    let flash = self.lightning_flash * 0.85;
                    for i in 0..3 {
                        top[i] = (top[i] + flash).min(1.0);
                        mid[i] = (mid[i] + flash).min(1.0);
                        bot[i] = (bot[i] + flash * 0.7).min(1.0);
                        fog[i] = (fog[i] + flash).min(1.0);
                    }
                }
            }
            WeatherType::Foggy => {
                // Heavy maritime mist
                let gray = 0.55;
                for i in 0..3 {
                    top[i] = top[i] * 0.35 + gray * 0.65;
                    mid[i] = mid[i] * 0.30 + gray * 0.70;
                    bot[i] = bot[i] * 0.40 + gray * 0.60;
                    fog[i] = gray;
                }
            }
            WeatherType::SynthwaveSunset => {
                // Vibrant neon magenta/golden orange sunset booster
                top[0] = (top[0] * 1.25).min(1.0);
                top[1] = top[1] * 0.80;
                top[2] = (top[2] * 1.15).min(1.0);
                mid[0] = (mid[0] * 1.20).min(1.0);
                mid[1] = (mid[1] * 0.95).min(1.0);
            }
        }

        (top, mid, bot, fog)
    }

    pub fn fog_density_value(&self) -> f32 {
        self.fog_density
    }

    pub fn road_friction_multiplier(&self) -> f32 {
        // Wet asphalt lowers traction
        1.0 - 0.28 * self.wetness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_cycles_through_all_types() {
        let mut ws = WeatherSystem::new();
        assert_eq!(ws.current, WeatherType::Sunny);
        ws.cycle_next();
        assert_eq!(ws.current, WeatherType::Cloudy);
        ws.cycle_next();
        assert_eq!(ws.current, WeatherType::Rainy);
        assert!(ws.is_raining());
        ws.cycle_next();
        assert_eq!(ws.current, WeatherType::Thunderstorm);
        assert!(ws.is_storming());
    }

    #[test]
    fn rain_density_rises_during_rainy_weather() {
        let mut ws = WeatherSystem::new();
        ws.set_weather(WeatherType::Rainy);
        assert!(ws.rain_density > 0.5);
        ws.update([0.0, 0.0, 5.0], 0.1);
        assert!(!ws.rain_streaks.is_empty(), "Should generate rain streaks");
        assert!(ws.wetness > 0.0, "Road wetness should accumulate");
    }

    #[test]
    fn thunderstorm_triggers_lightning_flash_and_thunder() {
        let mut ws = WeatherSystem::new();
        ws.set_weather(WeatherType::Thunderstorm);
        ws.lightning_timer = 0.05; // Force trigger on next tick

        let thunder = ws.update([10.0, 20.0, 2.0], 0.1);
        assert!(thunder, "Thunderstorm should emit thunder event");
        assert!(ws.lightning_flash > 0.5, "Lightning flash should illuminate");
        assert!(ws.lightning_strike_pos.is_some());
    }

    #[test]
    fn wetness_reduces_road_friction() {
        let mut ws = WeatherSystem::new();
        assert_eq!(ws.road_friction_multiplier(), 1.0);
        ws.wetness = 1.0;
        assert!(ws.road_friction_multiplier() < 0.75);
    }

    #[test]
    fn atmosphere_modulation_modifies_sky_during_storm() {
        let ws = WeatherSystem::new();
        let base_top = [0.36, 0.60, 0.79];
        let base_mid = [0.51, 0.70, 0.89];
        let base_bot = [0.22, 0.41, 0.48];
        let base_fog = [0.55, 0.75, 0.90, 1.0];

        let (top, _, _, _) = ws.modulate_atmosphere(base_top, base_mid, base_bot, base_fog);
        assert_eq!(top, base_top, "Sunny should not alter clear sky");

        let mut storm_ws = WeatherSystem::new();
        storm_ws.set_weather(WeatherType::Thunderstorm);
        let (storm_top, _, _, _) = storm_ws.modulate_atmosphere(base_top, base_mid, base_bot, base_fog);
        assert!(storm_top[0] < base_top[0], "Storm sky should be darker than clear sky");
    }
}
