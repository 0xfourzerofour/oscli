use anyhow::Result;
use cpal::{
    traits::{DeviceTrait, HostTrait},
    Device, Stream,
};
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
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("no output device available");

        let (stream, stream_handle) = OutputStream::try_from_device(&device)?;

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
}
