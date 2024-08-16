mod error;
mod game_config;
mod parser;
mod rebuilder;

use anyhow::Result as AResult;
use clap::{crate_version, Args, Parser, Subcommand, ValueEnum};
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

const BBCF_CONFIG: &str = include_str!("../static_db/bbcf.ron");
const DBFZ_CONFIG: &str = include_str!("../static_db/dbfz.ron");
const DNF_CONFIG: &str = include_str!("../static_db/dnf.ron");
const GBVS_CONFIG: &str = include_str!("../static_db/gbvs.ron");
const GBVSR_CONFIG: &str = include_str!("../static_db/gbvsr.ron");
const GGREV2_CONFIG: &str = include_str!("../static_db/ggrev2.ron");
const GGST_CONFIG: &str = include_str!("../static_db/ggst.ron");
const P4U2_CONFIG: &str = include_str!("../static_db/p4u2.ron");

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SupportedGame {
    /// Blazblue: Centralfiction
    Bbcf,
    /// Dragon Ball FighterZ
    Dbfz,
    /// DNF Duel
    Dnf,
    /// Granblue Fantasy Versus
    Gbvs,
    /// Granblue Fantasy Versus: Rising
    Gbvsr,
    /// Guilty Gear Xrd Rev2
    Ggrev2,
    /// Guilty Gear Strive
    Ggst,
    /// Persona 4 Arena Ultimax
    P4u2,
}

impl SupportedGame {
    pub fn into_config(self) -> ScriptConfig {
        let result = match self {
            SupportedGame::Bbcf => ScriptConfig::new(BBCF_CONFIG.as_bytes()),
            SupportedGame::Dbfz => ScriptConfig::new(DBFZ_CONFIG.as_bytes()),
            SupportedGame::Dnf => ScriptConfig::new(DNF_CONFIG.as_bytes()),
            SupportedGame::Gbvs => ScriptConfig::new(GBVS_CONFIG.as_bytes()),
            SupportedGame::Gbvsr => ScriptConfig::new(GBVSR_CONFIG.as_bytes()),
            SupportedGame::Ggrev2 => ScriptConfig::new(GGREV2_CONFIG.as_bytes()),
            SupportedGame::Ggst => ScriptConfig::new(GGST_CONFIG.as_bytes()),
            SupportedGame::P4u2 => ScriptConfig::new(P4U2_CONFIG.as_bytes()),
        };

        // all embedded configs should parse correctly so this should be infallible
        result.unwrap()
    }
}

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
    /// Enables reading all numbers in big-endian format, used by PS3 games
    #[clap(global = true, short, long)]
    big_endian: bool,
    #[clap(subcommand)]
    command: SubCmd,
}

#[derive(Args, Debug, Clone)]
#[group(required = true, multiple = false)]
struct ConfigArgs {
    /// A game supported by BBScript internally
    #[arg(short, long, group = "game-config")]
    game: Option<SupportedGame>,
    /// A custom config file stored externally
    #[arg(short, long, group = "game-config")]
    config_file: Option<PathBuf>,
}

/// Parses BBScript into an easily moddable format that can be rebuilt into usable BBScript
#[derive(Subcommand)]
enum SubCmd {
    /// Parses BBScript files and outputs them to a readable format
    Parse {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME", flatten)]
        game: ConfigArgs,
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
        #[clap(flatten)]
        game: ConfigArgs,
        /// Readable script to use as input
        #[arg(name = "INPUT")]
        input: PathBuf,
        /// File to write rebuilt script to as output
        #[arg(name = "OUTPUT")]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
        #[arg(short, long)]
        overwrite: bool,
    },
    /// Parse a script to machine-readable JSON
    ParseJson {
        /// File name of a config within the game DB folder
        #[clap(name = "GAME", flatten)]
        game: ConfigArgs,
        /// BBScript file to parse into readable format
        #[clap(name = "INPUT")]
        input: PathBuf,
        /// File to write readable script to as output
        #[clap(name = "OUTPUT")]
        output: PathBuf,
        /// Enables overwriting the file if a file with the same name as OUTPUT already exists
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
            let game = get_config(game)?;
            run_parser(
                game,
                input,
                output,
                (start_offset, end_offset),
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
            let game = get_config(game)?;
            run_rebuilder(game, input, output, args.big_endian)?;
        }
        SubCmd::ParseJson {
            game,
            input,
            output,
            overwrite,
        } => {
            confirm_io_files(&input, &output, overwrite)?;
            let game = get_config(game)?;
            run_structured_parser(game, input, output, args.big_endian)?;
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

fn get_config(config_args: ConfigArgs) -> AResult<ScriptConfig> {
    match (config_args.game, config_args.config_file) {
        (Some(game), None) => Ok(game.into_config()),
        (None, Some(path)) => Ok(ScriptConfig::load(path)?),
        _ => panic!("this should never happen"),
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
    game: ScriptConfig,
    in_path: PathBuf,
    out_path: PathBuf,
    byte_range: (Option<usize>, Option<usize>),
    big_endian: bool,
    indent_limit: usize,
) -> AResult<()> {
    let db = game;

    let in_file = load_file(in_path)?;

    let in_bytes = in_file;
    let file_length = in_bytes.len();

    let in_bytes =
        in_bytes[byte_range.0.unwrap_or(0)..(file_length - byte_range.1.unwrap_or(0))].to_owned();

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
    game: ScriptConfig,
    input: PathBuf,
    output: PathBuf,
    big_endian: bool,
) -> AResult<()> {
    let db = game;

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

fn run_structured_parser(
    game: ScriptConfig,
    in_path: PathBuf,
    out_path: PathBuf,
    big_endian: bool,
) -> AResult<()> {
    let db = game;

    let in_bytes = load_file(in_path)?;

    let result = if big_endian {
        db.parse::<byteorder::BigEndian>(in_bytes)
    } else {
        db.parse::<byteorder::LittleEndian>(in_bytes)
    }?;

    let mut output = File::create(out_path)?;

    let f = serde_json::to_string_pretty(&result)?;
    output.write_all(f.as_bytes())?;

    Ok(())
}
