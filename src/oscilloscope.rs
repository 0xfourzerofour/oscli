pub struct Oscilloscope {
    pub playhead: f32,
    playback_speed: f32,
    oscilloscope_range: f32,
}

impl Oscilloscope {
    pub fn new(file: &String) -> Self {
        Self {
            playhead: 0.0,
            playback_speed: 1.0,
            oscilloscope_range: 5.0,
        }
    }
}
