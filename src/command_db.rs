use bimap::BiMap;
use ron::de::from_reader;
use serde::Deserialize;
use crate::error::BBScriptError;

use std::error::Error;
use std::fs::File;

const DIR_SEPARATOR: &str = r"\";

#[derive(Deserialize, Debug)]
pub struct GameDB {
    functions: Vec<Function>,
}
impl GameDB {
    pub fn new(db_path: Option<&str>, game: &str) -> Result<GameDB, Box<dyn Error>> {
        let cmd_db_path: String =
            String::from(db_path.unwrap_or("static_db")) + DIR_SEPARATOR + game + ".ron";

        let cmd_db_file = File::open(cmd_db_path)?;

        let db: GameDB = from_reader(cmd_db_file)?;

        Ok(db)
    }

    pub fn find_by_id(&self, id_in: u32) -> Result<&Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.id == id_in) {
            return Ok(func);
        } else {
            return Err(BBScriptError::UnknownFunction(id_in.to_string()));
        }
    }

    pub fn find_by_name(&self, name_in: &str) -> Result<&Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.name == name_in) {
            return Ok(func)
        } else {
            return Err(BBScriptError::UnknownFunction(name_in.into()));
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    pub id: u32,
    pub size: u32,
    args: String,
    pub name: String,
    pub code_block: Indentation,
    named_values: BiMap<(u32, u32), (u32, String)>,
}
impl Function {
    // Not recoverable because name has no inherent value
    pub fn get_value(&self, name: (u32, String)) -> Result<u32, BBScriptError> {
        if let Some(value) = self.named_values.get_by_right(&name) {
            return Ok(value.1)
        } else {
            Err(BBScriptError::NoAssociatedValue(name.0.to_string(), name.1))
        }
    }

    // Recoverable, will just use raw value if no name associated
    pub fn get_name(&self, value: (u32, u32)) -> Option<String> {
        if let Some(value) = self.named_values.get_by_left(&value) {
            return Some(value.1.clone())
        } else {
            return None;
        }
    }
}

// use this later when parsing format strings
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
