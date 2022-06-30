#![allow(clippy::upper_case_acronyms)]

use crate::{
    error::BBScriptError,
    game_config::{
        ArgType, GenericInstruction, ScriptConfig, UnsizedInstruction,
    },
    HashMap,
};

use byteorder::{WriteBytesExt, LE};
use bytes::{Bytes, BytesMut};
use pest_consume::{match_nodes, Parser};

pub fn rebuild_bbscript(db: ScriptConfig, script: String) -> Result<Bytes, BBScriptError> {
    let parsed = BBSParser::parse(Rule::program, &script)?;
    let root = parsed.single()?;

    // verbose!(println!("Parsed program:\n{:#?}", &root), verbose);
    let program = BBSParser::program(root)?;

    let file = assemble_script(program, &db)?;

    Ok(file)
}

struct JumpTable {
    id_list: Vec<u32>,
    entries: HashMap<u32, Vec<u8>>,
}

impl JumpTable {
    pub fn new(id_list: Vec<u32>) -> Self {
        Self {
            id_list,
            entries: HashMap::new(),
        }
    }

    #[inline]
    pub fn is_entry_id(&self, id: u32) -> bool {
        self.id_list.contains(&id)
    }

    pub fn add_table_entry(&mut self, id: u32, offset: u32, jump_name: &Bytes) {
        assert!(self.is_entry_id(id));
        assert!(jump_name.len() == ArgType::STRING32_SIZE);

        let table = self.entries.entry(id).or_default();

        table.extend_from_slice(&jump_name);
        table.write_u32::<LE>(offset).unwrap();
    }

    pub fn to_table_bytes(mut self) -> Vec<u8> {
        const JUMP_ENTRY_LENGTH: usize = 0x24;
        let mut result = Vec::new();

        for id in &self.id_list {
            let entry = self.entries.entry(*id).or_default();

            let entry_count = entry.len() / JUMP_ENTRY_LENGTH;
            result.write_u32::<LE>(entry_count as u32).unwrap();
        }

        for id in self.id_list {
            let entry = self.entries.entry(id).or_default();

            result.append(entry);
        }

        result
    }
}

fn assemble_script(program: Vec<BBSFunction>, db: &ScriptConfig) -> Result<Bytes, BBScriptError> {
    let mut offset: u32 = 0x0;
    let mut script_buffer: Vec<u8> = Vec::new();
    let mut jump_tables = JumpTable::new(db.jump_table_ids.clone());

    for instruction in program {
        log::debug!("finding info for {}", instruction.name);
        let instruction_info = if let Some(i) = db.get_by_name(instruction.name.clone()) {
            i
        } else {
            log::trace!("could not locate instruction by name, trying by ID");
            if let Ok(id) = instruction.name.trim_start_matches("Unknown").parse() {
                if let Some(i) = db.get_by_id(id) {
                    i
                } else {
                    log::warn!("could not locate instruction {id} in config, using dynamic instruction size!");
                    let args = instruction.args.iter().map(|x| x.to_arg_type()).collect();
                    GenericInstruction::Unsized(id, UnsizedInstruction::from_parsed(args))
                }
            } else {
                return Err(BBScriptError::UnknownInstructionName(
                    instruction.name.clone(),
                ));
            }
        };

        log::trace!("building instruction `{}`", instruction.name.as_str());

        // if the instruction is sized, check that its size matches the config entry
        if let Some(instruction_size) = instruction_info.size() {
            if instruction.total_size() != instruction_size {
                return Err(BBScriptError::IncorrectFunctionSize(
                    instruction.name.to_string(),
                    instruction.total_size(),
                    instruction_size as usize,
                ));
            }
        }

        script_buffer
            .write_u32::<LE>(instruction_info.id())
            .unwrap();

        // if dynamically sized, the function size is written after the ID
        if db.is_unsized() {
            let instruction_dynamic_size = instruction.total_size() + 0x4;
            script_buffer
                .write_u32::<LE>(instruction_dynamic_size as u32)
                .unwrap();
        }

        if jump_tables.is_entry_id(instruction_info.id()) {
            if let Some(ParserValue::String32(name)) = instruction.args.get(0) {
                jump_tables.add_table_entry(instruction_info.id(), offset, name);
            }
        }

        for (index, arg) in instruction.args.iter().enumerate() {
            match arg {
                ParserValue::String32(string) | ParserValue::String16(string) => {
                    script_buffer.append(&mut string.to_vec())
                }
                ParserValue::Raw(data) => script_buffer.append(&mut data.to_vec()),
                &ParserValue::Number(num) => script_buffer.write_i32::<LE>(num).unwrap(),
                ParserValue::Named(variant) => {
                    let enum_name =
                        if let Some(ArgType::Enum(name)) = instruction_info.args().get(index) {
                            name.to_string()
                        } else {
                            return Err(BBScriptError::NoEnum(index, instruction_info.id()));
                        };

                    if let Some(value) = db.get_enum_value(enum_name.clone(), variant.to_string()) {
                        script_buffer.write_i32::<LE>(value).unwrap();
                    } else {
                        return Err(BBScriptError::NoAssociatedValue(
                            variant.to_string(),
                            enum_name,
                        ));
                    }
                }
                &ParserValue::Mem(var_id) => {
                    script_buffer.write_i32::<LE>(db.variable_tag).unwrap();
                    script_buffer.write_i32::<LE>(var_id).unwrap();
                }
                ParserValue::NamedMem(var_name) => {
                    let var_id = if let Some(var_id) = db.get_variable_by_name(var_name.to_string())
                    {
                        var_id
                    } else {
                        return Err(BBScriptError::NoVariableName(var_name.to_string()));
                    };

                    script_buffer.write_i32::<LE>(db.variable_tag).unwrap();
                    script_buffer.write_i32::<LE>(var_id).unwrap();
                }
                &ParserValue::Val(val) => {
                    script_buffer.write_i32::<LE>(db.literal_tag).unwrap();
                    script_buffer.write_i32::<LE>(val).unwrap();
                }
            };
        }
        offset = script_buffer.len() as u32;
    }
    let mut result_vec = Vec::new();

    result_vec.append(&mut jump_tables.to_table_bytes());

    result_vec.append(&mut script_buffer);

    let result = Bytes::from(result_vec);

    Ok(result)
}

#[derive(Debug)]
struct BBSFunction {
    name: String,
    args: Vec<ParserValue>,
}

impl BBSFunction {
    pub fn total_size(&self) -> usize {
        const BASE_SIZE: usize = 0x4;

        let arg_size_sum: usize = self
            .args
            .iter()
            .map(|arg| match arg {
                ParserValue::String32(_) => 32,
                ParserValue::String16(_) => 16,
                ParserValue::Raw(bytes) => bytes.len(),
                ParserValue::Mem(_) => 8,
                ParserValue::NamedMem(_) => 8,
                ParserValue::Val(_) => 8,
                ParserValue::Named(_) => 4,
                ParserValue::Number(_) => 4,
            })
            .sum();

        BASE_SIZE + arg_size_sum
    }
}

#[derive(Debug)]
enum ParserValue {
    String32(Bytes),
    String16(Bytes),
    Named(String),
    Number(i32),
    Raw(Bytes),
    NamedMem(String),
    Mem(i32),
    Val(i32),
}

impl ParserValue {
    pub fn to_arg_type(&self) -> ArgType {
        use ArgType::*;
        match self {
            ParserValue::String32(_) => String32,
            ParserValue::String16(_) => String16,
            ParserValue::Named(_) => panic!("this should never happen"),
            ParserValue::Number(_) => Number,
            ParserValue::Raw(data) => Unknown(data.len()),
            ParserValue::NamedMem(_) => AccessedValue,
            ParserValue::Mem(_) => AccessedValue,
            ParserValue::Val(_) => AccessedValue,
        }
    }
}

type Node<'i> = pest_consume::Node<'i, Rule, ()>;
type PResult<T> = Result<T, pest_consume::Error<Rule>>;

#[derive(Parser)]
#[grammar = "readable_bbscript.pest"]
struct BBSParser;

#[pest_consume::parser]
impl BBSParser {
    fn EOI(_input: Node) -> PResult<()> {
        Ok(())
    }

    fn program(input: Node) -> PResult<Vec<BBSFunction>> {
        Ok(match_nodes!(input.into_children();
            [function(functions)..,EOI(_)] => functions.collect(),
        ))
    }

    fn function(input: Node) -> PResult<BBSFunction> {
        let input = input.into_children();
        let none = Vec::new();

        let func = match_nodes!(input;
            [function_name(name), args(args)] => BBSFunction { name, args },
            [function_name(name)] => BBSFunction { name, args: none }
        );

        Ok(func)
    }

    fn function_name(input: Node) -> PResult<String> {
        Ok(input.as_str().into())
    }

    fn args(input: Node) -> PResult<Vec<ParserValue>> {
        Ok(match_nodes!(input.into_children();
            [arg(args)..,] => args.collect()
        ))
    }

    fn arg(input: Node) -> PResult<ParserValue> {
        Ok(match_nodes!(input.into_children();
            [string32(string)] => ParserValue::String32(string),
            [string16(string)] => ParserValue::String16(string),
            [named_var(string)] => ParserValue::NamedMem(string),
            [var_id(val)] => ParserValue::Mem(val),
            [tagged_value(val)] => ParserValue::Val(val),
            [named_value(name)] => ParserValue::Named(name),
            [raw_data(data)] => ParserValue::Raw(data),
            [num(val)] => ParserValue::Number(val),
        ))
    }

    fn string32(input: Node) -> PResult<Bytes> {
        Ok(string_to_bytes_of_size(input.as_str(), 32))
    }

    fn string16(input: Node) -> PResult<Bytes> {
        Ok(string_to_bytes_of_size(input.as_str(), 16))
    }

    fn named_var(input: Node) -> PResult<String> {
        Ok(input.as_str().into())
    }

    fn var_id(input: Node) -> PResult<i32> {
        match input.as_str().parse::<i32>() {
            Ok(num) => Ok(num),
            Err(e) => Err(input.error(e)),
        }
    }

    fn tagged_value(input: Node) -> PResult<i32> {
        match input.as_str().parse::<i32>() {
            Ok(num) => Ok(num),
            Err(e) => Err(input.error(e)),
        }
    }

    fn named_value(input: Node) -> PResult<String> {
        Ok(input.as_str().into())
    }

    fn raw_data(input: Node) -> PResult<Bytes> {
        match hex::decode(input.as_str()) {
            Ok(data) => Ok(Bytes::from(data)),
            Err(e) => Err(input.error(e)),
        }
    }

    fn num(input: Node) -> PResult<i32> {
        match input.as_str().parse::<i32>() {
            Ok(num) => Ok(num),
            Err(e) => Err(input.error(e)),
        }
    }
}

fn string_to_bytes_of_size<T: AsRef<str>>(input: T, size: usize) -> Bytes {
    let processed_string = unescaped(input);

    let mut string_bytes = BytesMut::from(processed_string.as_str());
    string_bytes.resize(size, 0x0);

    string_bytes.freeze()
}

fn unescaped<T: AsRef<str>>(string: T) -> String {
    string.as_ref().replace(r"\'", r"'")
}
