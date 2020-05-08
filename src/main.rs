#[macro_use]
use clap::{clap_app, AppSettings, ArgMatches};
use crate::error::BBScriptError;
use std::error::Error;
use std::path::Path;

mod command_db;
mod error;
mod parser;
mod rebuilder;

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
            (@arg unreal_mode: -u --unreal "Enable this flag for files extracted from Unreal UPKs, skips metadata to make parsing work")
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
            return Err(Box::new(e));
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

fn confirm_io_files(args: &ArgMatches) -> Result<(), BBScriptError> {
    let input = args.value_of("INPUT").unwrap();
    let output = args.value_of("OUTPUT").unwrap();
    let overwrite = args.is_present("overwrite");

    if Path::new(input).exists() {
        if !Path::new(output).exists() || overwrite {
            Ok(())
        } else {
            Err(BBScriptError::OutputAlreadyExists(output.into()))
        }
    } else {
        Err(BBScriptError::FileDoesNotExist(input.into()))
    }
}

fn run_parser(args: &ArgMatches) -> Result<(), Box<dyn Error>> {
    let db_folder = args.value_of("db_folder");
    let game = args.value_of("GAME").unwrap();
    let cmd_db = dbg!(command_db::create_db(db_folder, game)?);
    Ok(())
}

fn run_rebuilder(args: &ArgMatches) -> Result<(), Box<dyn Error>> {
    unimplemented!("not writing this today either lol")
}
