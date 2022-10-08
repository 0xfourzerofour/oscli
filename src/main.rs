use oscli::{channel::Channel, renderer::run};
use pollster;
use std::path::PathBuf;
use std::{ffi::OsString, fs::File};

use clap::{arg, Command};

fn main() {
    pollster::block_on(run())

    // Continued program logic goes here...
}
