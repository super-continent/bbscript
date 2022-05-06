use crate::error::BBScriptError;
use bimap::BiMap;
use ron::de;
use serde::Deserialize;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct GameDB {
    functions: Vec<Function>,
}
impl GameDB {
    pub fn new<T: Read>(db_config: T) -> Result<Self, BBScriptError> {
        de::from_reader(db_config).map_err(|e| BBScriptError::GameDBInvalid(e.to_string()))
    }

    pub fn load<T: AsRef<Path>>(config_path: T) -> Result<Self, BBScriptError> {
        let db_file = File::open(&config_path).map_err(|e| {
            BBScriptError::GameDBOpenError(format!("{}", config_path.as_ref().display()), e.to_string())
        })?;

        Self::new(db_file)
    }

    pub fn find_by_id(&self, id_in: u32) -> Result<Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.id == id_in) {
            Ok(func.clone())
        } else {
            Err(BBScriptError::UnknownFunction(format!("{}", id_in)))
        }
    }

    pub fn find_by_name(&self, name_in: &str) -> Result<Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.name == name_in) {
            Ok(func.clone())
        } else {
            Err(BBScriptError::UnknownFunction(name_in.into()))
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Function {
    pub id: u32,
    pub size: u32,
    args: String,
    pub name: String,
    pub code_block: CodeBlock,
    named_values: BiMap<(u32, i32), (u32, String)>,
}

impl Function {
    // Not recoverable because name has no inherent value
    pub fn get_value(&self, name: (u32, String)) -> Result<i32, BBScriptError> {
        if let Some(value) = self.named_values.get_by_right(&name) {
            Ok(value.1)
        } else {
            Err(BBScriptError::NoAssociatedValue(name.0.to_string(), name.1))
        }
    }

    // Recoverable, will just use raw value if no name associated
    pub fn get_name(&self, value: (u32, i32)) -> Option<String> {
        self.named_values.get_by_left(&value).map(|v| v.1.clone())
    }

    pub fn get_args(&self) -> Vec<Arg> {
        let arg_string = &self.args;

        let mut arg_accumulator = Vec::<Arg>::new();
        let mut arg_string = arg_string.as_bytes();
        let mut size_of_args = 0;

        while !arg_string.is_empty() {
            match arg_string {
                [b'i', ..] => {
                    size_of_args += 4;
                    arg_accumulator.push(Arg::Int);
                    arg_string = &arg_string[1..];
                }
                [b'1', b'6', b's', ..] => {
                    size_of_args += 16;
                    arg_accumulator.push(Arg::String16);
                    arg_string = &arg_string[3..];
                }
                [b'3', b'2', b's', ..] => {
                    size_of_args += 32;
                    arg_accumulator.push(Arg::String32);
                    arg_string = &arg_string[3..]
                }
                _ => arg_string = &arg_string[1..],
            }
        }
        if size_of_args < self.size - 4 && self.size >= 4 {
            let left_over = self.size - size_of_args - 4;
            arg_accumulator.push(Arg::Unknown(left_over));
        }

        arg_accumulator
    }

    pub fn instruction_name(&self) -> String {
        if self.name.is_empty() {
            format!("Unknown{}", &self.id)
        } else {
            self.name.to_string()
        }
    }

    pub fn is_jump_entry(&self) -> bool {
        self.code_block == CodeBlock::BeginJumpEntry
    }
}

#[derive(Debug, Clone)]
pub enum Arg {
    String16,
    String32,
    Int,
    Unknown(u32),
}

#[derive(Deserialize, Debug, PartialEq, Clone)]
pub enum CodeBlock {
    Begin,
    BeginJumpEntry,
    End,
    NoBlock,
}
