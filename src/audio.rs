#[cfg(feature = "audio")]
use rodio::{
    DeviceSinkBuilder, MixerDeviceSink, Player,
    source::{SineWave, Source},
};

#[cfg(feature = "audio")]
pub struct Audio {
    _sink: Option<MixerDeviceSink>,
    player: Option<Player>,
    muted: bool,
    volume: f32,
}

#[cfg(feature = "audio")]
impl Audio {
    pub fn new() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(sink) => {
                let player = Player::connect_new(sink.mixer());
                player.append(SineWave::new(440.0).repeat_infinite());
                player.set_volume(0.35);
                player.pause();
                Self {
                    _sink: Some(sink),
                    player: Some(player),
                    muted: false,
                    volume: 0.35,
                }
            }
            Err(_) => Self {
                _sink: None,
                player: None,
                muted: false,
                volume: 0.35,
            },
        }
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.apply_volume();
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.apply_volume();
    }

    pub fn set_active(&self, active: bool) {
        if let Some(player) = &self.player {
            if active && !self.muted {
                player.play();
            } else {
                player.pause();
            }
        }
    }

    fn apply_volume(&mut self) {
        if let Some(player) = &self.player {
            player.set_volume(if self.muted { 0.0 } else { self.volume });
        }
    }
}

#[cfg(feature = "audio")]
impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}
