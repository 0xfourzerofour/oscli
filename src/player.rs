use anyhow::Result;
use std::fs::File;

use rodio::{OutputStream, OutputStreamHandle};

use crate::track::Track;

pub struct Player {
    stream: OutputStream,
    stream_handle: OutputStreamHandle,
    track: Option<Track>,
}

impl Player {
    pub fn try_new() -> Result<Player> {
        let (stream, stream_handle) = OutputStream::try_default()?;
        Ok(Player {
            stream,
            stream_handle,
            track: None,
        })
    }

    pub fn try_play(&mut self, source: File) -> Result<()> {
        let track = Track::try_new(source, &self.stream_handle)?;
        self.track = Some(track);
        Ok(())
    }

    pub fn resume(&self) {
        if let Some(ref track) = self.track {
            track.play();
        }
    }

    pub fn pause(&self) {
        if let Some(ref track) = self.track {
            track.pause();
        }
    }

    pub fn stop(&self) {
        if let Some(ref track) = self.track {
            track.stop();
        }
    }
}
