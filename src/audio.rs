use anyhow::{bail, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait},
    ChannelCount, SampleRate, Stream,
};
use ringbuf::{traits::Split, Consumer, HeapRb, Producer};
use std::{
    fs::File,
    io::{BufReader, Seek},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};
use symphonia::{
    core::{
        audio::{AudioBufferRef, Signal},
        codecs::{DecoderOptions, CODEC_TYPE_NULL},
        errors::Error as SymphError,
        formats::{FormatOptions, FormatReader, SeekMode, SeekTo},
        io::MediaSourceStream,
        meta::MetadataOptions,
        probe::Hint,
    },
    default::get_probe,
};

#[derive(Clone)]
pub struct Peak {
    pub min: f32,
    pub max: f32,
}

pub struct Media {
    pub file_path: String,
    pub reader: Option<Box<dyn FormatReader>>,
    pub decoder: Option<Box<dyn symphonia::core::codecs::Decoder>>,
    pub track_id: u32,
    pub sample_rate: SampleRate,
    pub channels: ChannelCount,
    pub duration_samples: u64,
    pub peaks: Vec<Peak>,
    pub position: Arc<AtomicU32>,
    stream: Option<Stream>,
    producer: Option<Producer<f32>>,
    consumer: Option<Consumer<f32>>,
    decoding_thread: Option<JoinHandle<()>>,
    is_playing: Arc<AtomicBool>,
    is_done: Arc<AtomicBool>,
}

impl Media {
    pub fn try_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_string_lossy().to_string();
        let file = File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(BufReader::new(file)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let probed = get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let mut reader = probed.format;

        let track = reader.default_track().ok_or(anyhow::anyhow!("No track"))?;
        let track_id = track.id;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;
        let sample_rate = SampleRate(
            track
                .codec_params
                .sample_rate
                .ok_or(anyhow::anyhow!("No sample rate"))?,
        );
        let channels = track
            .codec_params
            .channels
            .ok_or(anyhow::anyhow!("No channels"))?
            .count() as u16;
        let duration_samples = track
            .codec_params
            .n_frames
            .ok_or(anyhow::anyhow!("No duration"))?
            * channels as u64;

        let peaks = Self::compute_peaks(&mut reader, &decoder, duration_samples)?;

        let buffer_capacity = (sample_rate.0 as usize) * (channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (producer, consumer) = rb.split();

        Ok(Self {
            file_path,
            reader: Some(reader),
            decoder: Some(decoder),
            track_id,
            sample_rate,
            channels,
            duration_samples,
            peaks,
            position: Arc::new(AtomicU32::new(0)),
            stream: None,
            producer: Some(producer),
            consumer: Some(consumer),
            decoding_thread: None,
            is_playing: Arc::new(AtomicBool::new(false)),
            is_done: Arc::new(AtomicBool::new(false)),
        })
    }

    fn compute_peaks(
        reader: &mut Box<dyn FormatReader>,
        decoder: &Box<dyn symphonia::core::codecs::Decoder>,
        duration_samples: u64,
    ) -> Result<Vec<Peak>> {
        let block_size = 1024;
        let num_blocks = (duration_samples / block_size) as usize + 1;
        let mut peaks = Vec::with_capacity(num_blocks);
        let mut tmp_decoder = symphonia::default::get_codecs()
            .make(&decoder.codec_params(), &DecoderOptions::default())?;

        loop {
            let packet = match reader.next_packet() {
                Ok(p) => p,
                Err(SymphError::ResetRequired) => continue,
                Err(SymphError::Eof) => break,
                Err(e) => bail!("Decode error: {}", e),
            };
            if packet.track_id() != reader.default_track().unwrap().id {
                continue;
            }

            match tmp_decoder.decode(&packet) {
                Ok(decoded) => {
                    let buf: AudioBufferRef = decoded;
                    let planes = buf.planes();
                    let samples = planes.planes()[0];

                    for chunk in samples.chunks(block_size as usize) {
                        let mut min = f32::MAX;
                        let mut max = f32::MIN;
                        for &s in chunk {
                            min = min.min(s);
                            max = max.max(s);
                        }
                        peaks.push(Peak { min, max });
                    }
                }
                Err(SymphError::IoError(_)) => continue,
                Err(SymphError::DecodeError(_)) => continue,
                Err(e) => bail!("Decode error: {}", e),
            }
        }
        Ok(peaks)
    }

    pub fn into_stream(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(anyhow::anyhow!("No device"))?;

        let mut supported_configs = device.supported_output_configs()?;
        let config = supported_configs
            .find(|r| {
                r.sample_format() == cpal::SampleFormat::F32
                    && r.max_sample_rate() >= self.sample_rate
                    && r.min_sample_rate() <= self.sample_rate
                    && r.channels() == self.channels
            })
            .ok_or(anyhow::anyhow!("No config"))?
            .with_sample_rate(self.sample_rate);

        let consumer = self
            .consumer
            .take()
            .ok_or(anyhow::anyhow!("Consumer taken"))?;
        let position = Arc::clone(&self.position);
        let is_done = Arc::clone(&self.is_done);

        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut pos = position.load(Ordering::Relaxed) as usize;
                for sample in data.iter_mut() {
                    if let Some(value) = consumer.pop() {
                        *sample = value;
                        pos += 1;
                    } else {
                        *sample = 0.0;
                        if is_done.load(Ordering::Relaxed) {
                            // Optional: stop
                        }
                    }
                }
                position.store(pos as u32, Ordering::Relaxed);
            },
            |err| eprintln!("Stream error: {:?}", err),
            None,
        )?;

        self.stream = Some(stream);
        Ok(())
    }

    pub fn start_decoding(&mut self) -> Result<()> {
        let mut reader = self.reader.take().ok_or(anyhow::anyhow!("Reader taken"))?;
        let mut decoder = self
            .decoder
            .take()
            .ok_or(anyhow::anyhow!("Decoder taken"))?;
        let mut producer = self
            .producer
            .take()
            .ok_or(anyhow::anyhow!("Producer taken"))?;
        let is_playing = Arc::clone(&self.is_playing);
        let is_done = Arc::clone(&self.is_done);
        let position = Arc::clone(&self.position);
        let track_id = self.track_id;

        let thread = thread::spawn(move || {
            while is_playing.load(Ordering::Relaxed) && !is_done.load(Ordering::Relaxed) {
                let packet = match reader.next_packet() {
                    Ok(p) => p,
                    Err(SymphError::Eof) => {
                        is_done.store(true, Ordering::Relaxed);
                        break;
                    }
                    Err(e) => {
                        eprintln!("Decode error: {:?}", e);
                        break;
                    }
                };
                if packet.track_id() != track_id {
                    continue;
                }

                if let Ok(decoded) = decoder.decode(&packet) {
                    let buf: AudioBufferRef = decoded;
                    let planes = buf.planes();
                    let samples = planes.planes()[0];

                    for &sample in samples {
                        while producer.is_full() {
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                        producer.push(sample).ok();
                    }
                    position.fetch_add(samples.len() as u32, Ordering::Relaxed);
                }
            }
        });

        self.decoding_thread = Some(thread);
        Ok(())
    }

    pub fn seek(&mut self, time_secs: f64) -> Result<()> {
        self.pause()?;
        let target_sample = (time_secs * self.sample_rate.0 as f64) as u64 * self.channels as u64;

        let file = File::open(&self.file_path)?;
        let mss = MediaSourceStream::new(Box::new(BufReader::new(file)), Default::default());
        let probed = get_probe().format(
            &Hint::new().with_extension("mp3"),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        self.reader = Some(probed.format);

        if let Some(reader) = &mut self.reader {
            reader.seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: symphonia::core::units::Time::from(time_secs),
                    track_id: Some(self.track_id),
                },
            )?;
        }

        self.position.store(target_sample as u32, Ordering::Relaxed);
        self.is_done.store(false, Ordering::Relaxed);

        let buffer_capacity = (self.sample_rate.0 as usize) * (self.channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (producer, consumer) = rb.split();
        self.producer = Some(producer);
        self.consumer = Some(consumer);

        self.start_decoding()?;
        self.play()?;
        Ok(())
    }

    pub fn play(&mut self) -> Result<()> {
        if self.stream.is_none() {
            self.into_stream()?;
        }
        if let Some(stream) = &self.stream {
            self.is_playing.store(true, Ordering::Relaxed);
            if self.decoding_thread.is_none() {
                self.start_decoding()?;
            }
            while self.consumer.as_ref().unwrap().len() < (self.sample_rate.0 as usize / 2) {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            stream.play()?;
            Ok(())
        } else {
            bail!("No stream");
        }
    }

    pub fn pause(&self) -> Result<()> {
        self.is_playing.store(false, Ordering::Relaxed);
        if let Some(stream) = &self.stream {
            stream.pause()?;
            Ok(())
        } else {
            bail!("No stream");
        }
    }

    pub fn reset(&mut self) -> Result<()> {
        self.pause()?;
        self.position.store(0, Ordering::Relaxed);
        self.is_done.store(false, Ordering::Relaxed);
        if let Some(thread) = self.decoding_thread.take() {
            thread.join().ok();
        }

        let file = File::open(&self.file_path)?;
        let mss = MediaSourceStream::new(Box::new(BufReader::new(file)), Default::default());
        let probed = get_probe().format(
            &Hint::new().with_extension("mp3"),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        self.reader = Some(probed.format);

        self.decoder = Some(symphonia::default::get_codecs().make(
            &probed.format.default_track().unwrap().codec_params,
            &DecoderOptions::default(),
        )?);

        let buffer_capacity = (self.sample_rate.0 as usize) * (self.channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (producer, consumer) = rb.split();
        self.producer = Some(producer);
        self.consumer = Some(consumer);

        Ok(())
    }
}
