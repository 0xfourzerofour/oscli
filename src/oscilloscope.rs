use std::io::{BufReader, Sink};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use rodio::OutputStreamHandle;

pub struct Oscilloscope {
    pub handler: OutputStreamHandle,
    playhead: f32,
    playback_speed: f32,
    oscilloscope_range: f32,
}

impl Oscilloscope {
    pub fn new(file: &String) -> Self {
        let (_stream, handle) = rodio::OutputStream::try_default().unwrap();
        let sink = rodio::Sink::try_new(&handle).unwrap();

        let file = std::fs::File::open(file).unwrap();
        sink.append(rodio::Decoder::new(BufReader::new(file)).unwrap());

        sink.detach();

        Self {
            handler: handle,
            playhead: 0.0,
            playback_speed: 1.0,
            oscilloscope_range: 5.0,
        }
    }
}
