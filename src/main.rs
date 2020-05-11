mod command_db;
mod error;
mod parser;
mod rebuilder;

#[macro_use]
use clap::{clap_app, AppSettings, ArgMatches};
use bytes::Bytes;

use std::error::Error;
use std::fs::{metadata, File};
use std::io::prelude::*;
use std::path::Path;

use crate::command_db::GameDB;
use crate::error::BBScriptError;
use crate::parser::parse_bbscript;

const VERSION: &str = "0.1.0";

fn main() {
    if let Err(e) = run() {
        println!("error: {}", e)
    };
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = clap_app!( BBScript =>
        (version: VERSION)
        (author: "Made by Pangaea")
        (about: "Parses BBScript into an easily moddable format that can be rebuilt into usable BBScript")
        (@subcommand parse =>
            (about: "Parses BBScript files and outputs them to a readable format")
            (version: VERSION)
            (@arg db_folder: -d --dbfolder "Path to folder containing game DB folders")
            (@arg overwrite: -o --overwrite "Enables overwriting the file if a file with the same name as OUTPUT already exists")
            (@arg begin_offset: +takes_value -b --begin_offset "Takes a hex offset from the start of the file specifying where the actual script begins")
            (@arg end_offset: +takes_value -e --end_offset "Takes a hex offset from the end of the file specifying where the script actually ends")
            (@arg GAME: +required "Subfolder of the game DB path specifying which game to read the commandDB and named value files from")
            (@arg INPUT: +required "Sets input file")
            (@arg OUTPUT: +required "Sets file to write as output")
        )
        (@subcommand rebuild =>
            (about: "Rebuilds readable BBScript into BBScript usable by games")
            (version: VERSION)
            (@arg db_folder: -d --dbfolder "Path to folder containing game DB folders")
            (@arg GAME: +required "Subfolder of the game DB path specifying which game to read the commandDB and named value files from")
            (@arg INPUT: +required "Sets input file")
            (@arg OUTPUT: +required "Sets file to write as output")
        )
    ).setting(AppSettings::SubcommandRequiredElseHelp)
     .get_matches();

    if let Some(subcmd) = args.subcommand_name() {
        let matches = args.subcommand_matches(subcmd).unwrap();

        if let Err(e) = confirm_io_files(matches) {
            return Err(e);
        }
        if let Err(e) = match subcmd {
            "parse" => run_parser(matches),
            "rebuild" => run_rebuilder(matches),
            _ => Ok(()),
        } {
            return Err(e);
        }
    }
    Ok(())
}

fn confirm_io_files(args: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let input = args.value_of("INPUT").unwrap();
    let output = args.value_of("OUTPUT").unwrap();
    let overwrite = args.is_present("overwrite");

    if Path::new(input).exists() && !metadata(input)?.is_dir() {
        if !Path::new(output).exists() || overwrite {
            Ok(())
        } else {
            Err(Box::new(BBScriptError::OutputAlreadyExists(output.into())))
        }
    } else {
        Err(Box::new(BBScriptError::BadInputFile(input.into())))
    }
}

fn get_offsets(begin: Option<&str>, end: Option<&str>) -> (Option<usize>, Option<usize>) {
    let begin_num = match begin {
        Some(start) => {
            let start = start.trim_start_matches("0x");
            if let Ok(n) = usize::from_str_radix(start, 16) {
                Some(n)
            } else {
                None
            }
        }
        None => None,
    };

    let end_num = match begin {
        Some(end) => {
            let end = end.trim_start_matches("0x");
            if let Ok(n) = usize::from_str_radix(end, 16) {
                Some(n)
            } else {
                None
            }
        }
        None => None,
    };

    (begin_num, end_num)
}

fn run_parser(args: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let db_folder = args.value_of("db_folder");
    let game = args.value_of("GAME").unwrap();
    let db = GameDB::new(db_folder, game)?;

    let in_path = args.value_of("INPUT").unwrap();
    let mut in_file = File::open(in_path)?;
    let mut in_bytes = Vec::<u8>::new();

    in_file.read_to_end(&mut in_bytes)?;

    let in_bytes = Bytes::from(in_bytes);

    let (start, end) = get_offsets(args.value_of("start_offset"), args.value_of("end_offset"));

    let in_bytes =
        in_bytes.slice(start.unwrap_or(0)..(metadata(in_path)?.len() as usize - end.unwrap_or(0)));

    if let Err(e) = parse_bbscript(db, in_bytes) {
        return Err(e);
    }

    Ok(())
}

fn run_rebuilder(args: &ArgMatches) -> Result<(), Box<dyn Error>> {
    unimplemented!("not writing this today lol")
}
