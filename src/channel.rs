use anyhow::Result;
use std::fs::File;

use rodio::{dynamic_mixer::DynamicMixer, OutputStream, OutputStreamHandle};

use crate::audio_source::AudioSource;

pub struct Channel {
    stream: OutputStream,
    stream_handle: OutputStreamHandle,
    source: Option<AudioSource>,
}

impl Channel {
    pub fn try_new() -> Result<Channel> {
        let (stream, stream_handle) = OutputStream::try_default()?;
        Ok(Channel {
            stream,
            stream_handle,
            source: None,
        })
    }

    pub fn try_play(&mut self, source: File) -> Result<()> {
        let track = AudioSource::try_new(source, &self.stream_handle)?;
        self.source = Some(track);
        Ok(())
    }

    pub fn resume(&self) {
        if let Some(ref source) = self.source {
            source.play();
        }
    }

    pub fn pause(&self) {
        if let Some(ref source) = self.source {
            source.pause();
        }
    }

    pub fn stop(&self) {
        if let Some(ref source) = self.source {
            source.stop();
        }
    }

    pub fn stop_2(&self) {
        if let Some(ref source) = self.source {
            source.stop();
        }
    }
}
