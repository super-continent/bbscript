mod command_db;
mod error;
mod log;
mod parser;
mod rebuilder;

use clap::{crate_version, AppSettings, Parser};

extern crate pest_derive;

use std::error::Error;
use std::fs::{metadata, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};

use crate::command_db::GameDB;
use crate::error::BBScriptError;
use crate::rebuilder::rebuild_bbscript;

const DB_FOLDER: &str = "static_db";

fn main() {
    if let Err(e) = run() {
        println!("ERROR: {}", e);
        std::process::exit(1);
    };
}

// im sorry for the redundancy here, but it makes the subcommands
// the first thing you enter and i want that structure to be the same
#[derive(Parser)]
#[clap(version = crate_version!(), author = "Made by Pangaea")]
#[clap(setting = AppSettings::SubcommandRequiredElseHelp, color = clap::ColorChoice::Never)]
/// Parses BBScript into an easily moddable format that can be rebuilt into usable BBScript
enum Command {
    /// Parses BBScript files and outputs them to a readable format
    Parse {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME")]
        game: String,
        /// BBScript file to parse into readable format
        #[clap(name = "INPUT", parse(from_os_str))]
        input: PathBuf,
        /// File to write readable script to as output
        #[clap(name = "OUTPUT", parse(from_os_str))]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
        #[clap(short, long)]
        overwrite: bool,
        /// Takes a hex offset from the start of the file specifying where the script actually begins
        #[clap(short, long, parse(try_from_str = parse_hex))]
        start_offset: Option<usize>,
        /// Takes a hex offset from the end of the file specifying where the script actually ends
        #[clap(short, long, parse(try_from_str = parse_hex))]
        end_offset: Option<usize>,
        /// Enables verbose output
        #[clap(short, long)]
        verbose: bool,
        /// Specifies a path where <GAME>.ron configs are stored
        #[clap(short, long, default_value = DB_FOLDER)]
        custom_db_folder: PathBuf,
    },
    /// Rebuilds readable BBScript into BBScript usable by games
    Rebuild {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME")]
        game: String,
        /// Readable script to use as input
        #[clap(name = "INPUT", parse(from_os_str))]
        input: PathBuf,
        /// File to write rebuilt script to as output
        #[clap(name = "OUTPUT", parse(from_os_str))]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
        #[clap(short, long)]
        overwrite: bool,
        /// Enables verbose output
        #[clap(short, long)]
        verbose: bool,
        /// Specifies a path where <GAME>.ron configs are stored
        #[clap(short, long, default_value = DB_FOLDER)]
        custom_db_folder: PathBuf,
    },
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = Command::parse();

    match args {
        Command::Parse {
            game,
            input,
            output,
            overwrite,
            start_offset,
            end_offset,
            verbose,
            custom_db_folder,
        } => {
            confirm_io_files(&input, &output, overwrite)?;
            run_parser(
                game,
                input,
                output,
                start_offset,
                end_offset,
                custom_db_folder,
                verbose,
            )?;
        }
        Command::Rebuild {
            game,
            input,
            output,
            overwrite,
            verbose,
            custom_db_folder,
        } => {
            confirm_io_files(&input, &output, overwrite)?;
            run_rebuilder(game, input, output, custom_db_folder, verbose)?;
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

fn get_byte_vec(name: PathBuf) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut file = File::open(&name)?;
    let meta = metadata(name)?;
    let mut file_buf = vec![0; meta.len() as usize];

    file.read_exact(&mut file_buf)?;

    Ok(file_buf)
}

fn parse_hex(input: &str) -> Result<usize, std::num::ParseIntError> {
    usize::from_str_radix(input, 16)
}

fn run_parser(
    game: String,
    in_path: PathBuf,
    out_path: PathBuf,
    start_offset: Option<usize>,
    end_offset: Option<usize>,
    db_folder: PathBuf,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    verbose!(
        println!("Extracting script info from `{}.ron`...", game),
        verbose
    );

    let mut ron_path = db_folder.join(game);
    ron_path.set_extension("ron");

    let db = GameDB::load(ron_path)?;

    let in_file = get_byte_vec(in_path)?;

    let in_bytes = in_file;
    let file_length = in_bytes.len();

    let in_bytes =
        in_bytes[start_offset.unwrap_or(0)..(file_length - end_offset.unwrap_or(0))].to_owned();

    match db.parse_bbscript_to_string(in_bytes, verbose) {
        Ok(f) => {
            let mut output = File::create(out_path)?;
            output.write_all(&f.as_bytes())?;
        }
        Err(e) => return Err(Box::new(e)),
    }

    Ok(())
}

fn run_rebuilder(
    game: String,
    input: PathBuf,
    output: PathBuf,
    db_folder: PathBuf,
    verbose: bool,
) -> Result<(), Box<dyn Error>> {
    verbose!(
        println!("Extracting script info from `{}.ron`...", game),
        verbose
    );

    let mut ron_path = db_folder.join(game);
    ron_path.set_extension("ron");

    let db = GameDB::load(ron_path)?;

    let mut script = String::new();
    File::open(input)?.read_to_string(&mut script)?;

    match rebuild_bbscript(db, script, verbose) {
        Ok(f) => {
            let mut output = File::create(output)?;
            output.write_all(&f.to_vec())?;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
