// use anyhow::{Error, Result};
// use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
// use cpal::{Sample, SampleFormat, Stream, StreamConfig};
// use minimp3::{Decoder, Frame};
// use oscli::{channel::Channel, renderer::run};
// use pollster;
// use std::path::PathBuf;
// use std::{ffi::OsString, fs::File};

// fn main() -> Result<Stream, Error> {

//     pollster::block_on(run())
// }

use std::fs::File;
use std::path::Path;

use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

mod output;

fn main() {
    let file = Box::new(File::open(Path::new("freefall.mp3")).unwrap());

    let mss = MediaSourceStream::new(file, Default::default());

    let hint = Hint::new();

    let format_opts: FormatOptions = Default::default();
    let metadata_opts: MetadataOptions = Default::default();
    let decoder_opts: DecoderOptions = Default::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .unwrap();

    let mut format = probed.format;

    let track = format.default_track().unwrap();

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)
        .unwrap();

    let track_id = track.id;

    let mut sample_buf = None;

    loop {
        let packet = format.next_packet().unwrap();

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                if sample_buf.is_none() {
                    let spec = *audio_buf.spec();

                    let duration = audio_buf.capacity() as u64;

                    sample_buf.replace(output::try_open(spec, duration).unwrap());
                }

                if let Some(buf) = &mut sample_buf {
                    buf.write(audio_buf).unwrap()
                }
            }
            Err(Error::DecodeError(_)) => (),
            Err(_) => break,
        }
    }
}
