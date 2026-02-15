use std::path::PathBuf;
use kira::{
    AudioManager, AudioManagerSettings, DefaultBackend,
    sound::static_sound::{StaticSoundData, StaticSoundHandle},
    sound::PlaybackState,
    Tween,
};

pub struct AudioEngine {
    manager: AudioManager<DefaultBackend>,
    current_handle: Option<StaticSoundHandle>,
    current_file: Option<PathBuf>,
    current_volume: f32,
    duration: f64,
    stopped: bool,
}

impl AudioEngine {
    pub fn new() -> Self {
        let manager = AudioManager::<DefaultBackend>::new(AudioManagerSettings::default())
            .expect("Failed to initialize audio manager");

        Self {
            manager,
            current_handle: None,
            current_file: None,
            current_volume: 0.0,
            duration: 0.0,
            stopped: false,
        }
    }

    pub fn play_song(&mut self, path: &PathBuf) -> Result<(), String> {
        if let Some(handle) = &mut self.current_handle {
            let _ = handle.stop(Tween::default());
        }
        self.current_handle = None;

        let sound_data = StaticSoundData::from_file(path)
            .map_err(|e| format!("Failed to load audio file: {}", e))?;

        self.duration = sound_data.duration().as_secs_f64();

        let mut handle = self.manager
            .play(sound_data)
            .map_err(|e| format!("Failed to play audio: {}", e))?;

        let _ = handle.set_volume(self.current_volume, Tween::default());

        self.current_handle = Some(handle);
        self.current_file = Some(path.clone());
        self.stopped = false;
        Ok(())
    }

    pub fn play(&mut self) {
        if let Some(handle) = &mut self.current_handle {
            if self.stopped {
                let _ = handle.seek_to(0.0);
                let _ = handle.resume(Tween::default());
                self.stopped = false;
            } else {
                match handle.state() {
                    PlaybackState::Paused | PlaybackState::Pausing => {
                        let _ = handle.resume(Tween::default());
                    }
                    PlaybackState::Stopped | PlaybackState::Stopping => {
                        if let Some(path) = self.current_file.clone() {
                            let _ = self.play_song(&path);
                        }
                    }
                    _ => {}
                }
            }
        } else if let Some(path) = self.current_file.clone() {
            let _ = self.play_song(&path);
        }
    }

    pub fn pause(&mut self) {
        if let Some(handle) = &mut self.current_handle {
            let _ = handle.pause(Tween::default());
        }
    }

    pub fn stop(&mut self) {
        if let Some(handle) = &mut self.current_handle {
            let _ = handle.pause(Tween::default());
            let _ = handle.seek_to(0.0);
            self.stopped = true;
        }
    }

    pub fn unload(&mut self) {
        if let Some(handle) = &mut self.current_handle {
            let _ = handle.stop(Tween::default());
        }
        self.current_handle = None;
        self.current_file = None;
        self.duration = 0.0;
        self.stopped = false;
    }

    pub fn set_volume(&mut self, volume_linear: f32) {
        let db = if volume_linear > 0.0 {
            20.0 * volume_linear.log10()
        } else {
            -80.0
        };
        self.current_volume = db;

        if let Some(handle) = &mut self.current_handle {
            let _ = handle.set_volume(db, Tween::default());
        }
    }

    pub fn seek(&mut self, position: f64) {
        if let Some(handle) = &mut self.current_handle {
            let _ = handle.seek_to(position);
        } else if let Some(path) = self.current_file.clone() {
            if self.play_song(&path).is_ok() {
                if let Some(handle) = &mut self.current_handle {
                    let _ = handle.seek_to(position);
                    let _ = handle.pause(Tween::default());
                }
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        if self.stopped {
            return false;
        }
        self.current_handle
            .as_ref()
            .map(|h| matches!(h.state(), PlaybackState::Playing | PlaybackState::Resuming))
            .unwrap_or(false)
    }

    pub fn get_position(&self) -> f64 {
        self.current_handle
            .as_ref()
            .map(|h| h.position())
            .unwrap_or(0.0)
    }

    pub fn get_duration(&self) -> f64 {
        self.duration
    }

    pub fn is_finished(&self) -> bool {
        self.current_handle
            .as_ref()
            .map(|h| matches!(h.state(), PlaybackState::Stopped | PlaybackState::Stopping))
            .unwrap_or(false)
    }

    pub fn current_file(&self) -> Option<&PathBuf> {
        self.current_file.as_ref()
    }
}
