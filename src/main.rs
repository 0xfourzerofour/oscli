use oscli::renderer::run;
use pollster;
use std::ffi::OsString;
use std::path::PathBuf;

use clap::{arg, Command};

fn cli() -> Command {
    Command::new("oscli")
        .about("A Command Line Oscilloscope")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .subcommand(
            Command::new("view")
                .about("command to view a selection file")
                .arg(arg!(<FILE> "The file to view"))
                .short_flag('f')
                .arg_required_else_help(true),
        )
}

fn main() {
    let matches = cli().get_matches();

    match matches.subcommand() {
        Some(("view", sub_matches)) => {
            let file_name = sub_matches.get_one::<String>("FILE").expect("required");

            pollster::block_on(run(file_name))
        }
        _ => unreachable!(),
    }

    // Continued program logic goes here...
}
