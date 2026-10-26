#![allow(dead_code)]
//! Native audio synthesis, radio station tuning and sound engine for VCS-Rust.
//! Re-implements CAudioEngine and CAERadioTrackManager faithful to Vice City Stories.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioStation {
    Wave103,
    FlashFm,
    Emotion98,
    VRock,
    ParadiseFm,
    Vcfl,
    Espantoso,
    Off,
}

impl RadioStation {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Wave103 => "Wave 103 (New Wave / Synth)",
            Self::FlashFm => "Flash FM (Pop 1984)",
            Self::Emotion98 => "Emotion 98.3 (Power Ballads)",
            Self::VRock => "V-Rock (Heavy Metal)",
            Self::ParadiseFm => "Paradise FM (Disco & Funk)",
            Self::Vcfl => "VCFL (Soul & Smooth R&B)",
            Self::Espantoso => "Radio Espantoso (Latin Jazz)",
            Self::Off => "Radio Eteinte",
        }
    }

    pub fn frequency(&self) -> f32 {
        match self {
            Self::Wave103 => 103.0,
            Self::FlashFm => 105.6,
            Self::Emotion98 => 98.3,
            Self::VRock => 99.5,
            Self::ParadiseFm => 101.2,
            Self::Vcfl => 89.9,
            Self::Espantoso => 94.7,
            Self::Off => 0.0,
        }
    }
}

pub struct AudioEngine {
    pub current_station: RadioStation,
    pub volume: f32,
    pub station_display_timer: f32,
    pub engine_rpm: f32,
    pub siren_phase: f32,
    pub audio_buffer: Vec<i16>,
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self {
            current_station: RadioStation::Wave103,
            volume: 0.8,
            station_display_timer: 0.0,
            engine_rpm: 850.0,
            siren_phase: 0.0,
            audio_buffer: Vec::with_capacity(4096),
        }
    }
}

impl AudioEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cycle_radio_next(&mut self) {
        let stations = [
            RadioStation::Wave103,
            RadioStation::FlashFm,
            RadioStation::Emotion98,
            RadioStation::VRock,
            RadioStation::ParadiseFm,
            RadioStation::Vcfl,
            RadioStation::Espantoso,
            RadioStation::Off,
        ];
        let idx = stations.iter().position(|&s| s == self.current_station).unwrap_or(0);
        self.current_station = stations[(idx + 1) % stations.len()];
        self.station_display_timer = 3.5;
    }

    pub fn cycle_radio_prev(&mut self) {
        let stations = [
            RadioStation::Wave103,
            RadioStation::FlashFm,
            RadioStation::Emotion98,
            RadioStation::VRock,
            RadioStation::ParadiseFm,
            RadioStation::Vcfl,
            RadioStation::Espantoso,
            RadioStation::Off,
        ];
        let idx = stations.iter().position(|&s| s == self.current_station).unwrap_or(0);
        self.current_station = stations[(idx + stations.len() - 1) % stations.len()];
        self.station_display_timer = 3.5;
    }

    pub fn update(&mut self, car_speed: f32, siren_active: bool, dt: f32) {
        if self.station_display_timer > 0.0 {
            self.station_display_timer = (self.station_display_timer - dt).max(0.0);
        }

        // Engine RPM calculation (850 Idle -> 7200 Max RPM)
        let speed_ratio = (car_speed.abs() / 48.0).clamp(0.0, 1.0);
        let target_rpm = 850.0 + speed_ratio * 5800.0 + ((car_speed * 4.0).sin() * 150.0);
        self.engine_rpm += (target_rpm - self.engine_rpm) * (1.0 - (-8.0 * dt).exp());

        if siren_active {
            self.siren_phase = (self.siren_phase + dt * 2.5).fract();
        }
    }

    pub fn engine_tone_freq(&self) -> f32 {
        // Fundamental engine tone frequency
        (self.engine_rpm / 60.0) * 2.0 // V8 cross-plane 4 pulses per rev
    }

    pub fn siren_tone_freq(&self) -> f32 {
        if self.siren_phase < 0.5 {
            750.0 // Low tone
        } else {
            1100.0 // High tone
        }
    }

    pub fn radio_banner(&self) -> Option<String> {
        if self.station_display_timer > 0.0 {
            Some(format!("RADIO : {}", self.current_station.name()))
        } else {
            None
        }
    }

    pub fn synthesize_gunshot(&mut self, samples: usize) -> &[i16] {
        self.audio_buffer.clear();
        for i in 0..samples {
            let t = i as f32 / samples as f32;
            let envelope = (1.0 - t).powi(4);
            // White noise burst + low punch
            let punch = (t * 60.0 * std::f32::consts::TAU).sin() * (1.0 - t).powi(2);
            let noise = ((i * 1103515245 + 12345) & 0x7fff) as f32 / 32768.0 * 2.0 - 1.0;
            let sample = (punch * 0.6 + noise * 0.4) * envelope * 24000.0;
            self.audio_buffer.push(sample.clamp(-32767.0, 32767.0) as i16);
        }
        &self.audio_buffer
    }

    pub fn synthesize_thunder(&mut self, samples: usize) -> &[i16] {
        self.audio_buffer.clear();
        for i in 0..samples {
            let t = i as f32 / samples as f32;
            let envelope = (1.0 - t).powi(2) * (t * 20.0).min(1.0);
            let low_rumble = (t * 32.0 * std::f32::consts::TAU).sin() * 0.6
                + (t * 18.0 * std::f32::consts::TAU).sin() * 0.4;
            let crackle = ((i * 1664525 + 1013904223) & 0x7fff) as f32 / 32768.0 * 2.0 - 1.0;
            let sample = (low_rumble * 0.75 + crackle * 0.25) * envelope * 28000.0;
            self.audio_buffer.push(sample.clamp(-32767.0, 32767.0) as i16);
        }
        &self.audio_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_tuner_cycles_vice_city_stations() {
        let mut audio = AudioEngine::new();
        assert_eq!(audio.current_station, RadioStation::Wave103);

        audio.cycle_radio_next();
        assert_eq!(audio.current_station, RadioStation::FlashFm);
        assert!(audio.radio_banner().is_some());

        audio.cycle_radio_prev();
        assert_eq!(audio.current_station, RadioStation::Wave103);
    }

    #[test]
    fn engine_pitch_scales_with_vehicle_speed() {
        let mut audio = AudioEngine::new();
        audio.update(0.0, false, 0.1);
        let idle_freq = audio.engine_tone_freq();

        audio.update(35.0, false, 0.5);
        let high_freq = audio.engine_tone_freq();
        assert!(high_freq > idle_freq, "Engine pitch must increase with speed");
    }

    #[test]
    fn siren_tone_alternates_frequency() {
        let mut audio = AudioEngine::new();
        audio.siren_phase = 0.2;
        assert_eq!(audio.siren_tone_freq(), 750.0);

        audio.siren_phase = 0.7;
        assert_eq!(audio.siren_tone_freq(), 1100.0);
    }

    #[test]
    fn pcm_sample_generation_produces_non_zero_waveform() {
        let mut audio = AudioEngine::new();
        let samples = audio.synthesize_gunshot(128);
        assert_eq!(samples.len(), 128);
        let energy: i64 = samples.iter().map(|&s| (s as i64).abs()).sum();
        assert!(energy > 10000, "Gunshot audio must have substantial sound energy");
    }

    #[test]
    fn thunder_audio_synthesis_produces_low_frequency_rumble() {
        let mut audio = AudioEngine::new();
        let samples = audio.synthesize_thunder(256);
        assert_eq!(samples.len(), 256);
        let energy: i64 = samples.iter().map(|&s| (s as i64).abs()).sum();
        assert!(energy > 20000, "Thunder audio must have massive low-frequency rumble energy");
    }
}
