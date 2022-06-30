use crate::error::BBScriptError;
use bimap::BiMap;
use ron::de;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::HashMap;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

pub type BBSNumber = i32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArgType {
    /// Unknown argument data.
    /// Typically used only when an [`Instruction`]s `size` field does not match the size of the `args` field
    Unknown(usize),
    /// A 16-byte string
    String16,
    /// A 32-byte string
    String32,
    Number,
    /// A named enum, the name provides access to a [`BiMap<String, i32>`]
    Enum(String),
    /// A tagged value represented by `{ tag: i32, value: i32 }` that will be turned into a variant of [`TaggedValue`].
    /// The first `i32` is the tag, which is typically `0` for a literal value, and `2` for a variable ID
    ///
    /// `AccessedValue`s are treated specially, the value
    /// they contain will be translated to a corresponding name using the `variable_config` field in the [`GameDB`]
    AccessedValue,
}

impl ArgType {
    pub(crate) const STRING32_SIZE: usize = 0x20;
    pub(crate) const STRING16_SIZE: usize = 0x10;

    /// Get the size of the argument type in bytes
    // most types are 4 bytes
    pub const fn size(&self) -> usize {
        use ArgType::*;
        match self {
            Unknown(n) => *n,
            Number => std::mem::size_of::<BBSNumber>(),
            Enum(_) => std::mem::size_of::<BBSNumber>(),
            String16 => 0x10,
            String32 => 0x20,
            AccessedValue => std::mem::size_of::<BBSNumber>() * 2,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TaggedValue {
    Literal(BBSNumber),
    Variable(BBSNumber),
    /// A tagged value whos tag does not match either specified value in the [`GameDB`]
    Improper {
        tag: BBSNumber,
        value: BBSNumber,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstructionInfo {
    #[serde(serialize_with = "ordered_map")]
    Sized(HashMap<u32, SizedInstruction>),
    #[serde(serialize_with = "ordered_map")]
    Unsized(HashMap<u32, UnsizedInstruction>),
}

#[derive(Debug, Clone)]
pub enum GenericInstruction {
    Sized(u32, SizedInstruction),
    Unsized(u32, UnsizedInstruction),
}

impl GenericInstruction {
    #[inline]
    pub fn name(&self) -> Option<String> {
        let name = match self {
            Self::Sized(_, a) => a.name.clone(),
            Self::Unsized(_, a) => a.name.clone(),
        };

        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    #[inline]
    pub fn id(&self) -> u32 {
        match self {
            Self::Sized(id, _) => *id,
            Self::Unsized(id, _) => *id,
        }
    }

    #[inline]
    pub fn size(&self) -> Option<usize> {
        match self {
            Self::Sized(_, a) => Some(a.size),
            _ => None,
        }
    }

    pub fn args(&self) -> SmallVec<[ArgType; 16]> {
        match self {
            Self::Sized(_, i) => i.args(),
            Self::Unsized(_, i) => i.args(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptConfig {
    pub jump_table_ids: Vec<u32>,
    /// The value used for identifying [`TaggedValue::Literal`]s in the scripts
    pub literal_tag: BBSNumber,
    /// The value used for identifying [`TaggedValue::Variable`]s in the scripts
    pub variable_tag: BBSNumber,
    /// A map that allows associating names with specific values of a [`TaggedValue::Variable`]
    pub named_variables: BiMap<BBSNumber, String>,
    /// A map of [`BBSArgType::Enum`] maps for naming specific values
    #[serde(serialize_with = "ordered_enums")]
    pub named_value_maps: HashMap<String, BiMap<BBSNumber, String>>,
    pub(crate) instructions: InstructionInfo,
}

impl ScriptConfig {
    #[inline]
    pub fn new<T: Read>(db_config: T) -> Result<Self, BBScriptError> {
        de::from_reader(db_config).map_err(|e| BBScriptError::GameDBInvalid(e.to_string()))
    }

    pub fn load<T: AsRef<Path>>(config_path: T) -> Result<Self, BBScriptError> {
        let db_file = File::open(&config_path).map_err(|e| {
            BBScriptError::GameDBOpenError(
                format!("{}", config_path.as_ref().display()),
                e.to_string(),
            )
        })?;

        Self::new(db_file)
    }

    #[inline]
    pub fn get_by_name(&self, name: String) -> Option<GenericInstruction> {
        match self.instructions {
            InstructionInfo::Sized(ref a) => a
                .iter()
                .find(|(_, x)| x.name == name)
                .map(|(id, x)| GenericInstruction::Sized(*id, x.clone())),
            InstructionInfo::Unsized(ref a) => a
                .iter()
                .find(|(_, x)| x.name == name)
                .map(|(id, x)| GenericInstruction::Unsized(*id, x.clone())),
        }
    }

    #[inline]
    pub fn get_by_id(&self, id: u32) -> Option<GenericInstruction> {
        match self.instructions {
            InstructionInfo::Sized(ref map) => map
                .get(&id)
                .map(|x| GenericInstruction::Sized(id, x.clone())),
            InstructionInfo::Unsized(ref map) => map
                .get(&id)
                .map(|x| GenericInstruction::Unsized(id, x.clone())),
        }
    }

    pub fn get_enum_value(&self, enum_name: String, variant: String) -> Option<BBSNumber> {
        self.named_value_maps
            .get(&enum_name)
            .map(|e| e.get_by_right(&variant).map(|v| *v))
            .flatten()
    }

    pub fn get_variable_by_name(&self, variable_name: String) -> Option<BBSNumber> {
        self.named_variables
            .get_by_right(&variable_name)
            .map(|x| *x)
    }

    pub fn is_unsized(&self) -> bool {
        match self.instructions {
            InstructionInfo::Unsized(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizedInstruction {
    pub size: usize,
    pub name: String,
    pub code_block: CodeBlock,
    args: SmallVec<[ArgType; 16]>,
}

impl SizedInstruction {
    pub fn args(&self) -> SmallVec<[ArgType; 16]> {
        const INSTRUCTION_SIZE: usize = 0x4;
        let known_args_size: usize = self.args.iter().map(|a| a.size()).sum();

        let mut args = self.args.clone();

        if known_args_size != (self.size - INSTRUCTION_SIZE) {
            // size typically has an extra 4 bytes because of the ID being a u32
            let left_over = (self.size.saturating_sub(INSTRUCTION_SIZE)) - known_args_size;
            args.push(ArgType::Unknown(left_over))
        }

        args
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsizedInstruction {
    pub name: String,
    pub code_block: CodeBlock,
    pub args: SmallVec<[ArgType; 16]>,
}

impl UnsizedInstruction {
    pub fn new() -> Self {
        Self {
            name: "".to_string(),
            code_block: CodeBlock::NoBlock,
            args: SmallVec::new(),
        }
    }

    pub fn from_parsed(args: Vec<ArgType>) -> Self {
        Self {
            name: "".into(),
            code_block: CodeBlock::NoBlock,
            args: args.into(),
        }
    }

    pub fn args_with_known_size(&self, dynamic_size: usize) -> SmallVec<[ArgType; 16]> {
        const INSTRUCTION_SIZE: usize = 0x8;
        let known_args_size: usize = self.args.iter().map(|a| a.size()).sum();

        log::debug!("dynamic instruction size: {dynamic_size}");

        let mut args = self.args.clone();

        if known_args_size > (dynamic_size - INSTRUCTION_SIZE) {
            panic!("dynamic argument size smaller than argument size in config!")
        }

        if known_args_size != (dynamic_size - INSTRUCTION_SIZE) {
            // size typically has an extra 4 bytes because of the ID being a u32
            let left_over = (dynamic_size.saturating_sub(INSTRUCTION_SIZE)) - known_args_size;
            args.push(ArgType::Unknown(left_over))
        }

        args
    }

    pub fn args(&self) -> SmallVec<[ArgType; 16]> {
        self.args.clone()
    }
}

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
            BBScriptError::GameDBOpenError(
                format!("{}", config_path.as_ref().display()),
                e.to_string(),
            )
        })?;

        Self::new(db_file)
    }

    pub fn find_by_id(&self, id_in: u32) -> Result<Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.id == id_in) {
            Ok(func.clone())
        } else {
            Err(BBScriptError::UnknownInstructionName(format!("{}", id_in)))
        }
    }

    pub fn find_by_name(&self, name_in: &str) -> Result<Function, BBScriptError> {
        if let Some(func) = self.functions.iter().find(|x| x.name == name_in) {
            Ok(func.clone())
        } else {
            Err(BBScriptError::UnknownInstructionName(name_in.into()))
        }
    }
}

impl Into<ScriptConfig> for GameDB {
    fn into(self) -> ScriptConfig {
        let mut value_maps = HashMap::new();

        let jump_table_ids: Vec<u32> = self
            .functions
            .iter()
            .filter(|x| x.is_jump_entry())
            .map(|x| x.id)
            .collect();

        let instructions = self
            .functions
            .into_iter()
            .fold(HashMap::new(), |mut map, func| {
                let mut enum_replacements = Vec::new();

                // convert named value maps into the new format using the function name as the name of the enum
                if !func.named_values.is_empty() {
                    let arg_count = func.get_args().len();

                    for i in 0..arg_count {
                        let map = func
                            .named_values
                            .iter()
                            .filter_map(|((idx, val), (_, name))| {
                                if *idx == i as u32 {
                                    Some((val, name))
                                } else {
                                    None
                                }
                            })
                            .fold(BiMap::new(), |mut map, (left, right)| {
                                map.insert(*left, right.clone());
                                map
                            });

                        let mut enum_name = func.name.clone();
                        enum_name.push_str(format!("{i}_{}", func.id).as_str());

                        // only insert new enum if there are no exact duplicates
                        if !value_maps.values().any(|x| *x == map) && !map.is_empty() {
                            value_maps.insert(enum_name, map.clone());
                        }

                        for (k, v) in value_maps.iter() {
                            if *v == map {
                                enum_replacements.push((i, k.clone()));
                            }
                        }
                    }
                }

                let id = func.id;
                let mut instruction: SizedInstruction = func.into();

                // replace Number types with enums
                for (i, e) in enum_replacements {
                    instruction.args[i] = ArgType::Enum(e);
                }

                map.insert(id, instruction);

                map
            });

        ScriptConfig {
            jump_table_ids,
            literal_tag: 0,
            variable_tag: 2,
            named_variables: BiMap::new(),
            named_value_maps: value_maps,
            instructions: InstructionInfo::Sized(instructions),
        }
    }
}

/// Serialize hashmap as BTreeMap for ordered keys
fn ordered_enums<S>(
    value: &HashMap<String, BiMap<BBSNumber, String>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let ordered: std::collections::BTreeMap<_, _> = value
        .iter()
        .map(|(x, y)| {
            let sorted = bimap::BiBTreeMap::from_iter(y.into_iter());
            (x, sorted)
        })
        .collect();
    ordered.serialize(serializer)
}

/// Serialize hashmap as BTreeMap for ordered keys
fn ordered_map<S, K, V>(value: &HashMap<K, V>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    K: Ord + Serialize,
    V: Serialize,
{
    let ordered: std::collections::BTreeMap<_, _> = value.iter().collect();
    ordered.serialize(serializer)
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

impl Into<SizedInstruction> for Function {
    fn into(self) -> SizedInstruction {
        let args = self
            .get_args()
            .into_iter()
            .filter_map(|arg| {
                use ArgType::*;
                match arg {
                    Arg::Int => Some(Number),
                    Arg::String16 => Some(String16),
                    Arg::String32 => Some(String32),
                    Arg::Unknown(_) => None,
                }
            })
            .collect();

        SizedInstruction {
            size: self.size as usize,
            name: self.name,
            code_block: self.code_block,
            args,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Arg {
    String16,
    String32,
    Int,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeBlock {
    Begin,
    #[deprecated]
    BeginJumpEntry,
    End,
    NoBlock,
}
