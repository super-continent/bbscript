mod error;
mod game_config;
mod parser;
mod rebuilder;

use anyhow::Result as AResult;
use clap::{crate_version, Parser, Subcommand};
use game_config::ScriptConfig;

extern crate pest_derive;

use std::fs::{metadata, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use crate::error::BBScriptError;
#[cfg(feature = "old-cfg-converter")]
use crate::game_config::GameDB;
use crate::rebuilder::rebuild_bbscript;

type HashMap<K, V> = std::collections::HashMap<K, V>;

const DB_FOLDER: &str = "static_db";

fn main() {
    if let Err(e) = run() {
        println!("ERROR: {}", e);
        std::process::exit(1);
    };
}

#[derive(Parser)]
#[clap(version = crate_version!(), author = "Made by Pangaea")]
#[clap(color = clap::ColorChoice::Never)]
#[clap(arg_required_else_help(true), subcommand_required(true))]
struct MainCli {
    /// Verbose output level, ranges from 0 to 5
    #[clap(global = true, short, long, action = clap::ArgAction::Count)]
    verbosity: u8,
    /// Specifies a path where <GAME>.ron configs are stored
    #[clap(global = true, short, long, default_value = DB_FOLDER, env = "BBSCRIPT_DB_DIR")]
    custom_db_folder: PathBuf,
    /// Enables reading all numbers in big-endian format, used by PS3 games
    #[clap(global = true, short, long)]
    big_endian: bool,
    #[clap(subcommand)]
    command: SubCmd,
}

/// Parses BBScript into an easily moddable format that can be rebuilt into usable BBScript
#[derive(Subcommand)]
enum SubCmd {
    /// Parses BBScript files and outputs them to a readable format
    Parse {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME")]
        game: String,
        /// BBScript file to parse into readable format
        #[clap(name = "INPUT")]
        input: PathBuf,
        /// File to write readable script to as output
        #[clap(name = "OUTPUT")]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
        #[clap(short, long)]
        overwrite: bool,
        /// Takes a hex offset from the start of the file specifying where the script actually begins
        #[arg(short, long, value_parser(parse_hex))]
        start_offset: Option<usize>,
        /// Takes a hex offset from the end of the file specifying where the script actually ends
        #[clap(short, long, value_parser(parse_hex))]
        end_offset: Option<usize>,
        #[arg(short, long, default_value_t = 12)]
        indent_limit: usize,
    },
    /// Rebuilds readable BBScript into BBScript usable by games
    Rebuild {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME")]
        game: String,
        /// Readable script to use as input
        #[clap(name = "INPUT")]
        input: PathBuf,
        /// File to write rebuilt script to as output
        #[clap(name = "OUTPUT")]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
        #[clap(short, long)]
        overwrite: bool,
    },
    /// Convert old configs from past BBScript versions into the newer (>v1.0.0) format
    #[cfg(feature = "old-cfg-converter")]
    Convert {
        #[clap(name = "GAME")]
        game: String,
        #[clap(name = "OUTPUT")]
        output: PathBuf,
        #[clap(short, long)]
        overwrite: bool,
    },
}

fn run() -> AResult<()> {
    let args = MainCli::parse();

    let level = log_level_from_verbosity(args.verbosity);
    simple_logger::SimpleLogger::new()
        .with_level(level)
        .without_timestamps()
        .init()?;

    match args.command {
        SubCmd::Parse {
            game,
            input,
            output,
            overwrite,
            start_offset,
            end_offset,
            indent_limit,
        } => {
            confirm_io_files(&input, &output, overwrite)?;
            run_parser(
                game,
                input,
                output,
                start_offset,
                end_offset,
                args.custom_db_folder,
                args.big_endian,
                indent_limit,
            )?;
        }
        SubCmd::Rebuild {
            game,
            input,
            output,
            overwrite,
        } => {
            confirm_io_files(&input, &output, overwrite)?;
            run_rebuilder(game, input, output, args.custom_db_folder, args.big_endian)?;
        }
        #[cfg(feature = "old-cfg-converter")]
        SubCmd::Convert {
            game,
            output,
            overwrite,
        } => {
            run_converter(game, output, overwrite)?;
        }
    }
    Ok(())
}

fn confirm_io_files(
    input: &PathBuf,
    output: &PathBuf,
    overwrite: bool,
) -> Result<(), BBScriptError> {
    if Path::new(input).is_file() {
        if !Path::new(output).exists() || overwrite {
            Ok(())
        } else {
            Err(BBScriptError::OutputAlreadyExists(
                output.to_string_lossy().into(),
            ))
        }
    } else {
        Err(BBScriptError::BadInputFile(input.to_string_lossy().into()))
    }
}

/// Attempts to return a `Vec<u8>` of a files contents
fn load_file(name: PathBuf) -> AResult<Vec<u8>> {
    let mut file = File::open(&name)?;
    let meta = metadata(name)?;
    let mut file_buf = Vec::with_capacity(meta.len() as usize);

    file.read_to_end(&mut file_buf)?;

    Ok(file_buf)
}

fn parse_hex(input: &str) -> Result<usize, std::num::ParseIntError> {
    usize::from_str_radix(input, 16)
}

/// Get a LevelFilter from -v occurences
/// `Error` is excluded as the program doesn't call `log::error!()`
fn log_level_from_verbosity(verbosity: u8) -> log::LevelFilter {
    use log::LevelFilter::*;

    match verbosity {
        0 => Off,
        1 => Warn,
        2 => Info,
        3 => Debug,
        _ => Trace,
    }
}

fn run_parser(
    game: String,
    in_path: PathBuf,
    out_path: PathBuf,
    start_offset: Option<usize>,
    end_offset: Option<usize>,
    db_folder: PathBuf,
    big_endian: bool,
    indent_limit: usize,
) -> AResult<()> {
    log::info!("Extracting script info from `{}.ron`...", game);

    let mut ron_path = db_folder.join(game);
    ron_path.set_extension("ron");

    let db = ScriptConfig::load(ron_path)?;

    let in_file = load_file(in_path)?;

    let in_bytes = in_file;
    let file_length = in_bytes.len();

    let in_bytes =
        in_bytes[start_offset.unwrap_or(0)..(file_length - end_offset.unwrap_or(0))].to_owned();

    let result = if big_endian {
        db.parse_to_string::<byteorder::BigEndian>(in_bytes, indent_limit)
    } else {
        db.parse_to_string::<byteorder::LittleEndian>(in_bytes, indent_limit)
    };

    match result {
        Ok(f) => {
            let mut output = File::create(out_path)?;
            output.write_all(f.as_bytes())?;
        }
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

fn run_rebuilder(
    game: String,
    input: PathBuf,
    output: PathBuf,
    db_folder: PathBuf,
    big_endian: bool,
) -> AResult<()> {
    log::info!("Extracting script info from `{}.ron`...", game);

    let mut ron_path = db_folder.join(game);
    ron_path.set_extension("ron");

    let db = ScriptConfig::load(ron_path)?;

    let mut script = String::new();
    File::open(input)?.read_to_string(&mut script)?;

    let result = if big_endian {
        rebuild_bbscript::<byteorder::BigEndian>(db, script)
    } else {
        rebuild_bbscript::<byteorder::LittleEndian>(db, script)
    };

    match result {
        Ok(f) => {
            let mut output = File::create(output)?;
            output.write_all(&f)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

#[cfg(feature = "old-cfg-converter")]
fn run_converter(game: String, out: PathBuf, overwrite: bool) -> AResult<()> {
    let mut ron_path = PathBuf::from(DB_FOLDER).join(game);
    ron_path.set_extension("ron");

    confirm_io_files(&ron_path, &out, overwrite)?;

    let new_db: ScriptConfig = GameDB::load(ron_path)?.into();

    let string = ron::ser::to_string_pretty(&new_db, ron::ser::PrettyConfig::new())?;

    File::create(out)?.write_all(string.as_bytes())?;

    Ok(())
}
