#![allow(clippy::upper_case_acronyms)]

use std::io::Write;

use crate::{
    error::BBScriptError,
    game_config::{ArgType, GenericInstruction, ScriptConfig, UnsizedInstruction},
};

use byteorder::{ByteOrder, WriteBytesExt};
use bytes::{Bytes, BytesMut};
use pest_consume::{match_nodes, Parser};

pub fn rebuild_bbscript<B: ByteOrder>(
    db: ScriptConfig,
    script: String,
) -> Result<Vec<u8>, BBScriptError> {
    let root = BBSParser::parse(Rule::program, &script)
        .and_then(|p| p.single())
        .map_err(Box::new)?;

    log::trace!("Parsed program AST:\n{:#?}", &root);
    let program = BBSParser::program(root).map_err(Box::new)?;

    let file = assemble_script::<B>(program, &db)?;

    Ok(file)
}

fn assemble_script<B: ByteOrder>(
    program: Vec<BBSFunction>,
    db: &ScriptConfig,
) -> Result<Vec<u8>, BBScriptError> {
    // current position of the reader
    let mut offset: u32 = 0x0;
    let mut script_buffer: Vec<u8> = Vec::new();

    // TODO: figure out behavior around eliminating duplicate state jump entries
    // let mut previous_jump_entries = std::collections::HashSet::new();

    let mut jump_entry_count = 0;
    let mut jump_table_buffer: Vec<u8> = Vec::new();

    for instruction in program {
        log::debug!("finding info for {}", instruction.name);
        let instruction_info = if let Some(i) = db.get_by_name(&instruction.name) {
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
                    instruction_size,
                ));
            }
        }

        script_buffer.write_u32::<B>(instruction_info.id()).unwrap();

        // if dynamically sized, the function size is written after the ID
        if db.is_unsized() {
            let instruction_dynamic_size = instruction.total_size() + 0x4;
            script_buffer
                .write_u32::<B>(instruction_dynamic_size as u32)
                .unwrap();
        }

        // build state jump table
        if db.is_jump_entry_id(instruction_info.id()) {
            if let Some(ParserValue::String32(name)) = instruction.args.get(0) {
                // this check deduplicates jump table entries
                // if previous_jump_entries.insert(name.clone())

                jump_table_buffer.write_all(name).unwrap();
                jump_table_buffer.write_u32::<B>(offset).unwrap();
                jump_entry_count += 1;
            }
        }

        for (index, arg) in instruction.args.iter().enumerate() {
            log::trace!(
                "writing arg {} of value `{:?}` from instruction `{}`",
                index,
                arg,
                &instruction.name
            );

            match arg {
                ParserValue::String32(string) | ParserValue::String16(string) => {
                    script_buffer.append(&mut string.to_vec())
                }
                ParserValue::Raw(data) => script_buffer.append(&mut data.to_vec()),
                &ParserValue::Number(num) => script_buffer.write_i32::<B>(num).unwrap(),
                ParserValue::Named(variant) => {
                    let enum_name =
                        if let Some(ArgType::Enum(name)) = instruction_info.args().get(index) {
                            name.to_string()
                        } else {
                            return Err(BBScriptError::NoEnum(index, instruction_info.id()));
                        };

                    if let Some(value) = db.get_enum_value(enum_name.clone(), variant.to_string()) {
                        script_buffer.write_i32::<B>(value).unwrap();
                    } else {
                        return Err(BBScriptError::NoAssociatedValue(
                            variant.to_string(),
                            enum_name,
                        ));
                    }
                }
                &ParserValue::Mem(var_id) => {
                    script_buffer.write_i32::<B>(db.variable_tag).unwrap();
                    script_buffer.write_i32::<B>(var_id).unwrap();
                }
                ParserValue::NamedMem(var_name) => {
                    let var_id = if let Some(var_id) = db.get_variable_by_name(var_name.to_string())
                    {
                        var_id
                    } else {
                        return Err(BBScriptError::NoVariableName(var_name.to_string()));
                    };

                    script_buffer.write_i32::<B>(db.variable_tag).unwrap();
                    script_buffer.write_i32::<B>(var_id).unwrap();
                }
                &ParserValue::Val(val) => {
                    script_buffer.write_i32::<B>(db.literal_tag).unwrap();
                    script_buffer.write_i32::<B>(val).unwrap();
                }
                &ParserValue::BadTag(tag, val) => {
                    log::trace!(
                        "Got bad tag {tag} with value {val} at offset {}",
                        script_buffer.len()
                    );
                    script_buffer.write_i32::<B>(tag).unwrap();
                    script_buffer.write_i32::<B>(val).unwrap();
                }
            };
        }
        offset = script_buffer.len() as u32;
    }
    let mut result = Vec::new();

    result.write_u32::<B>(jump_entry_count as u32).unwrap();
    result.append(&mut jump_table_buffer);
    result.append(&mut script_buffer);

    let result = result;

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
                ParserValue::BadTag(_, _) => 8,
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
    BadTag(i32, i32),
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
            ParserValue::BadTag(_, _) => AccessedValue,
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
            [unknown_tag(tag), tagged_value(val)] => ParserValue::BadTag(tag, val),
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

    fn unknown_tag(input: Node) -> PResult<i32> {
        match input.as_str().parse::<i32>() {
            Ok(num) => Ok(num),
            Err(e) => Err(input.error(e)),
        }
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
