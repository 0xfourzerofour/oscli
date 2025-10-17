use anyhow::{bail, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    ChannelCount, SampleRate, Stream,
};
use ringbuf::{traits::{Consumer, Observer, Producer, Split}, HeapCons, HeapProd, HeapRb};
use std::{
    fs::File,
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
        codecs::DecoderOptions,
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
    pub min_left: f32,
    pub max_left: f32,
    pub min_right: f32,
    pub max_right: f32,
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
    producer: Option<HeapProd<f32>>,
    consumer: Option<HeapCons<f32>>,
    decoding_thread: Option<JoinHandle<()>>,
    is_playing: Arc<AtomicBool>,
    is_done: Arc<AtomicBool>,
}

impl Media {
    pub fn try_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_path = path.as_ref().to_string_lossy().to_string();
        let file = File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
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
        let codec_params = track.codec_params.clone();
        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())?;
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

        // Reopen the file for playback since the reader is now at EOF after computing peaks
        let file = File::open(&path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let probed = get_probe().format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let reader = probed.format;
        let decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())?;

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
        let block_size = 32;
        let num_blocks = (duration_samples / block_size) as usize + 1;
        let mut peaks = Vec::with_capacity(num_blocks);
        let mut tmp_decoder = symphonia::default::get_codecs()
            .make(&decoder.codec_params(), &DecoderOptions::default())?;

        loop {
            let packet = match reader.next_packet() {
                Ok(p) => p,
                Err(SymphError::ResetRequired) => continue,
                Err(SymphError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => bail!("Decode error: {}", e),
            };
            if packet.track_id() != reader.default_track().unwrap().id {
                continue;
            }

            match tmp_decoder.decode(&packet) {
                Ok(decoded) => {
                    let buf: AudioBufferRef = decoded;
                    let num_channels = buf.spec().channels.count();

                    // Extract left and right channel samples
                    let (left_samples, right_samples): (Vec<f32>, Vec<f32>) = match buf {
                        AudioBufferRef::F32(buffer) => {
                            let left = buffer.chan(0).to_vec();
                            let right = if num_channels > 1 { buffer.chan(1).to_vec() } else { left.clone() };
                            (left, right)
                        },
                        AudioBufferRef::S16(buffer) => {
                            let left: Vec<f32> = buffer.chan(0).iter().map(|&s| s as f32 / 32768.0).collect();
                            let right: Vec<f32> = if num_channels > 1 {
                                buffer.chan(1).iter().map(|&s| s as f32 / 32768.0).collect()
                            } else {
                                left.clone()
                            };
                            (left, right)
                        },
                        AudioBufferRef::S32(buffer) => {
                            let left: Vec<f32> = buffer.chan(0).iter().map(|&s| s as f32 / 2147483648.0).collect();
                            let right: Vec<f32> = if num_channels > 1 {
                                buffer.chan(1).iter().map(|&s| s as f32 / 2147483648.0).collect()
                            } else {
                                left.clone()
                            };
                            (left, right)
                        },
                        _ => continue, // Skip other formats for brevity
                    };

                    for i in 0..(left_samples.len() / block_size as usize) {
                        let start = i * block_size as usize;
                        let end = ((i + 1) * block_size as usize).min(left_samples.len());

                        let mut min_left = f32::MAX;
                        let mut max_left = f32::MIN;
                        let mut min_right = f32::MAX;
                        let mut max_right = f32::MIN;

                        for j in start..end {
                            min_left = min_left.min(left_samples[j]);
                            max_left = max_left.max(left_samples[j]);
                            min_right = min_right.min(right_samples[j]);
                            max_right = max_right.max(right_samples[j]);
                        }

                        peaks.push(Peak { min_left, max_left, min_right, max_right });
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
            .map(|c| c.with_sample_rate(self.sample_rate))
            .or_else(|| {
                eprintln!("No exact config match, using default config");
                device.default_output_config().ok()
            })
            .ok_or(anyhow::anyhow!("No config"))?;

        let mut consumer = self
            .consumer
            .take()
            .ok_or(anyhow::anyhow!("Consumer taken"))?;
        let position = Arc::clone(&self.position);
        let is_done = Arc::clone(&self.is_done);

        let mut callback_count = 0;
        let stream = device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                callback_count += 1;
                if callback_count % 100 == 0 {
                    eprintln!("Audio callback #{}, buffer size: {}", callback_count, data.len());
                }
                let mut samples_read = 0;
                for sample in data.iter_mut() {
                    if let Some(value) = consumer.try_pop() {
                        *sample = value;
                        samples_read += 1;
                    } else {
                        *sample = 0.0;
                        if is_done.load(Ordering::Relaxed) {
                            // Optional: stop
                        }
                    }
                }
                // Update position based on samples actually played
                position.fetch_add(samples_read, Ordering::Relaxed);
                if callback_count % 100 == 0 {
                    eprintln!("Read {} samples from ringbuf", samples_read);
                }
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
        let track_id = self.track_id;

        let thread = thread::spawn(move || {
            eprintln!("Decoding thread started");
            let mut packet_count = 0;
            while is_playing.load(Ordering::Relaxed) && !is_done.load(Ordering::Relaxed) {
                let packet = match reader.next_packet() {
                    Ok(p) => p,
                    Err(SymphError::IoError(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
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
                    packet_count += 1;
                    if packet_count % 100 == 0 {
                        eprintln!("Decoded {} packets", packet_count);
                    }
                    let buf: AudioBufferRef = decoded;

                    // Get number of channels and frames
                    let num_channels = buf.spec().channels.count();
                    let num_frames = buf.frames();

                    // Interleave channels into a single Vec<f32>
                    let mut samples: Vec<f32> = Vec::with_capacity(num_frames * num_channels);

                    match buf {
                        AudioBufferRef::F32(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    samples.push(buffer.chan(chan_idx)[frame_idx]);
                                }
                            }
                        },
                        AudioBufferRef::U8(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push((s as f32 - 128.0) / 128.0);
                                }
                            }
                        },
                        AudioBufferRef::U16(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push((s as f32 - 32768.0) / 32768.0);
                                }
                            }
                        },
                        AudioBufferRef::U24(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push((s.inner() as f32 - 8388608.0) / 8388608.0);
                                }
                            }
                        },
                        AudioBufferRef::U32(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push((s as f32 - 2147483648.0) / 2147483648.0);
                                }
                            }
                        },
                        AudioBufferRef::S8(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push(s as f32 / 128.0);
                                }
                            }
                        },
                        AudioBufferRef::S16(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push(s as f32 / 32768.0);
                                }
                            }
                        },
                        AudioBufferRef::S24(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push(s.inner() as f32 / 8388608.0);
                                }
                            }
                        },
                        AudioBufferRef::S32(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push(s as f32 / 2147483648.0);
                                }
                            }
                        },
                        AudioBufferRef::F64(buffer) => {
                            for frame_idx in 0..num_frames {
                                for chan_idx in 0..num_channels {
                                    let s = buffer.chan(chan_idx)[frame_idx];
                                    samples.push(s as f32);
                                }
                            }
                        },
                    };

                    let mut pushed = 0;
                    for &sample in &samples {
                        // Break out if we're no longer playing (e.g., during seek)
                        if !is_playing.load(Ordering::Relaxed) {
                            break;
                        }
                        while producer.is_full() {
                            // Check again in case we're paused/seeking
                            if !is_playing.load(Ordering::Relaxed) {
                                break;
                            }
                            thread::sleep(std::time::Duration::from_millis(10));
                        }
                        if producer.try_push(sample).is_ok() {
                            pushed += 1;
                        }
                    }
                    if packet_count % 100 == 0 {
                        eprintln!("Pushed {} samples to ringbuf", pushed);
                    }
                }
            }
        });

        self.decoding_thread = Some(thread);
        Ok(())
    }

    pub fn seek(&mut self, time_secs: f64) -> Result<()> {
        eprintln!("Seek to {} seconds", time_secs);

        // Stop playback and decoding
        self.pause()?;
        self.is_done.store(false, Ordering::Relaxed);

        // Wait for decoding thread to stop
        if let Some(thread) = self.decoding_thread.take() {
            thread.join().ok();
        }

        let target_sample = (time_secs * self.sample_rate.0 as f64) as u64 * self.channels as u64;

        // Reopen and seek the file
        let file = File::open(&self.file_path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let probed = get_probe().format(
            &Hint::new().with_extension("mp3"),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let codec_params = probed.format.default_track().unwrap().codec_params.clone();
        self.reader = Some(probed.format);
        self.decoder = Some(symphonia::default::get_codecs().make(
            &codec_params,
            &DecoderOptions::default(),
        )?);

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

        // Drop old stream and recreate with new ringbuf
        self.stream = None;
        let buffer_capacity = (self.sample_rate.0 as usize) * (self.channels as usize) * 2;
        let rb = HeapRb::<f32>::new(buffer_capacity);
        let (producer, consumer) = rb.split();
        self.producer = Some(producer);
        self.consumer = Some(consumer);

        // Restart playback
        self.play()?;
        Ok(())
    }

    pub fn play(&mut self) -> Result<()> {
        eprintln!("Play called");
        if self.stream.is_none() {
            eprintln!("Creating stream...");
            self.into_stream()?;
            eprintln!("Stream created");
            self.is_playing.store(true, Ordering::Relaxed);
            eprintln!("Starting decoding...");
            self.start_decoding()?;
            eprintln!("Decoding started");
            // Give the decoder a moment to fill the buffer
            thread::sleep(std::time::Duration::from_millis(100));
        } else {
            eprintln!("Stream already exists, resuming");
            self.is_playing.store(true, Ordering::Relaxed);
            if self.decoding_thread.is_none() {
                self.start_decoding()?;
            }
        }

        if let Some(stream) = &self.stream {
            eprintln!("Calling stream.play()");
            stream.play()?;
            eprintln!("Stream playing!");
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
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let probed = get_probe().format(
            &Hint::new().with_extension("mp3"),
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )?;
        let codec_params = probed.format.default_track().unwrap().codec_params.clone();
        self.reader = Some(probed.format);

        self.decoder = Some(symphonia::default::get_codecs().make(
            &codec_params,
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
