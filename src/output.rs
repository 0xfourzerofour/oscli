use core::mem::MaybeUninit;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use minimp3::{Decoder, Error, Frame};
use ringbuf::{Consumer, Producer, SharedRb};
use std::fs::File;
use std::sync::{Arc, Mutex};

pub struct Output {
    pub buffer: Arc<Vec<i16>>,
    pub sample_rate: cpal::SampleRate,
    pub channels: cpal::ChannelCount,
    pub stream: Option<Stream>,
    pub position: Arc<Mutex<usize>>,
    pub prod: Producer<i32, Arc<SharedRb<i32, Vec<MaybeUninit<i32>>>>>,
    pub cons: Consumer<i32, Arc<SharedRb<i32, Vec<MaybeUninit<i32>>>>>,
}

// unsafe impl Send for Output {}

impl Output {
    pub fn new() -> Self {
        let (prod, cons) = SharedRb::<i32, Vec<_>>::new(1024).split();

        Self {
            buffer: Arc::new(Vec::new()),
            sample_rate: cpal::SampleRate(44100),
            channels: 2,
            stream: None,
            position: Arc::new(Mutex::new(0)),
            prod,
            cons,
        }
    }

    pub fn from_bytes(file: File) -> Self {
        let mut decoder = Decoder::new(file);
        let mut buffer = Vec::new();
        let mut sample_rate = cpal::SampleRate(0);
        let mut channels: cpal::ChannelCount = 1;

        loop {
            match decoder.next_frame() {
                Ok(Frame {
                    mut data,
                    sample_rate: rate,
                    channels: ch,
                    ..
                }) => {
                    sample_rate = cpal::SampleRate(rate as u32);
                    channels = ch as cpal::ChannelCount;
                    buffer.append(&mut data);
                }
                Err(Error::Eof) => break,
                Err(e) => panic!("{:?}", e),
            }
        }

        let (prod, cons) = SharedRb::<i32, Vec<_>>::new(1024).split();

        Self {
            buffer: Arc::new(buffer),
            sample_rate,
            channels,
            stream: None,
            position: Arc::new(Mutex::new(0)),
            prod,
            cons,
        }
    }

    pub fn play(&mut self) {
        if let Some(ref stream) = self.stream {
            stream.play().unwrap();
            return;
        }

        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .expect("no output device available");

        let mut supported_configs_range = device
            .supported_output_configs()
            .expect("error while querying configs");

        let supported_config = supported_configs_range
            .find(|range| {
                range.sample_format() == cpal::SampleFormat::F32
                    && range.max_sample_rate() >= self.sample_rate
                    && range.min_sample_rate() <= self.sample_rate
                    && range.channels() == self.channels
            })
            .expect("Could not find supported audio config")
            .with_sample_rate(self.sample_rate);

        let buffer = self.buffer.clone();
        let position = self.position.clone();

        let (mut prod, cons) = SharedRb::<i32, Vec<_>>::new(1024).split();

        self.cons = cons;

        self.stream = Some(
            device
                .build_output_stream(
                    &supported_config.into(),
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let mut pos = position.lock().unwrap();
                        for sample in data.iter_mut() {
                            let value = if *pos < buffer.len() { buffer[*pos] } else { 0 };
                            *sample = cpal::Sample::from(&value);
                            prod.push(value as i32);
                            *pos += 1;
                        }
                    },
                    move |_err| panic!("ERROR"),
                )
                .expect("Building output stream failed"),
        );
    }

    pub fn set_position(&mut self, seconds: f64) {
        let mut position = self.position.lock().unwrap();
        *position = self.seconds_to_samples(seconds).max(0) as usize;
    }

    pub fn pause(&mut self) {
        if let Some(ref stream) = self.stream {
            stream.pause().unwrap()
        }
    }

    pub fn forward(&mut self, seconds: f64) {
        let number_of_samples = self.seconds_to_samples(seconds);
        let mut position = self.position.lock().unwrap();
        *position = (*position as i32 + number_of_samples).max(0) as usize;
    }

    fn seconds_to_samples(&self, seconds: f64) -> i32 {
        (self.sample_rate.0 as f64 * seconds) as i32 * self.channels as i32
    }
}
