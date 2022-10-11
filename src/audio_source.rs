use anyhow::Result;
use rodio::{OutputStreamHandle, Sink};
use std::{fs::File, io::BufReader};

pub struct AudioSource(Sink);

impl AudioSource {
    pub fn try_new(source: File, stream_handle: &OutputStreamHandle) -> Result<AudioSource> {
        let sink = stream_handle.play_once(BufReader::new(source))?;
        Ok(AudioSource(sink))
    }

    pub fn play(&self) {
        self.0.play();
    }

    pub fn pause(&self) {
        self.0.pause();
    }

    pub fn stop(&self) {
        self.0.stop();
    }
}
