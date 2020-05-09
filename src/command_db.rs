use bimap::BiMap;
use ron::de::from_reader;
use serde::Deserialize;

use std::error::Error;
use std::fs::File;

const DIR_SEPARATOR: &str = r"\";

#[derive(Deserialize, Debug)]
pub struct GameDB {
    functions: Vec<Function>,
}
impl GameDB {
    pub fn new(db_path: Option<&str>, game_folder: &str) -> Result<GameDB, Box<dyn Error>> {
        let cmd_db_path: String = String::from(db_path.unwrap_or("static_db"))
            + DIR_SEPARATOR
            + game_folder
            + DIR_SEPARATOR
            + "commandDB.ron";

        let cmd_db_file = File::open(cmd_db_path)?;

        let db: GameDB = from_reader(cmd_db_file)?;

        Ok(db)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Function {
    id: u32,
    size: u32,
    args: String,
    name: String,
    code_block: Indentation,
    named_values: BiMap<(u32, u32), (u32, String)>,
}

pub enum Arg {
    String16,
    String32,
    Int,
    Unknown(usize),
}

#[derive(Deserialize, Debug)]
pub enum Indentation {
    Begin,
    End,
    None,
}
