use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs::File;
use std::path::Path;

use crate::error::BBScriptError;

const DIR_SEPARATOR: &str = r"\";

// TODO: Figure out a way to create a struct GameDB that parses all the JSON and
// contains a good interface for getting data about each instruction from the ID number
pub struct GameDB {}
impl GameDB {
    pub fn new(game_folder: &str) -> Result<GameDB, Box<dyn Error>> {
        unimplemented!();
        let cmd_db_path: String = String::from("static_db")
            + DIR_SEPARATOR
            + game_folder
            + DIR_SEPARATOR
            + "commandDB.json";
        let command_db = File::open(cmd_db_path);
    }
}

// Just for testing, delete this after I have all the typing and stuff figured out. Gonna be using struct GameDB instead
pub fn create_db(db_folder: Option<&str>, game: &str) -> Result<(), Box<dyn Error>> {
    let cmd_db_path: String = String::from(db_folder.unwrap_or("static_db"))
        + DIR_SEPARATOR
        + game
        + DIR_SEPARATOR
        + "commandDB.json";
    let command_db = File::open(cmd_db_path)?;

    Ok(())
}

// Data types I might use for this, might also just end up converting all the JSON to RON to make it easier to hardcode typing
#[derive(Serialize, Deserialize)]
struct BBSFunc {
    function_id: u32,
    data: FunctionData,
}

#[derive(Serialize, Deserialize)]
struct FunctionData {
    pub name: Option<String>,
    pub format: Option<String>,
    pub size: u32,
}
