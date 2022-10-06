use anyhow::Result;
use rodio::{OutputStreamHandle, Sink};
use std::{fmt::Display, fs::File, io::BufReader};

pub struct Track(Sink);

impl Track {
    pub fn try_new(source: File, stream_handle: &OutputStreamHandle) -> Result<Track> {
        let sink = stream_handle.play_once(BufReader::new(source))?;
        Ok(Track(sink))
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

#[derive(Debug, Clone)]
pub struct TrackData {
    pub id: i32,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl Display for TrackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.artist)?;
        write!(f, " - ")?;
        write!(f, "{}", self.title)?;
        if self.title.is_empty() && self.artist.is_empty() {
            write!(f, "({})", self.path)?;
        }
        Ok(())
    }
}
