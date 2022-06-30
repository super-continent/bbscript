#![allow(clippy::upper_case_acronyms)]

use crate::{game_config::GameDB, error::BBScriptError};

use byteorder::{WriteBytesExt, LE};
use bytes::{Bytes, BytesMut};
use pest_consume::{match_nodes, Parser};

pub fn rebuild_bbscript(
    db: GameDB,
    script: String,
) -> Result<Bytes, BBScriptError> {
    let parsed = BBSParser::parse(Rule::program, &script)?;
    let root = parsed.single()?;

    // verbose!(println!("Parsed program:\n{:#?}", &root), verbose);
    let program = BBSParser::program(root)?;

    let file = assemble_script(program, &db)?;

    Ok(file)
}

fn assemble_script(program: Vec<BBSFunction>, db: &GameDB) -> Result<Bytes, BBScriptError> {
    let mut offset: u32 = 0x0;
    let mut table_entry_count: u32 = 0;
    let mut jump_table: Vec<u8> = Vec::new();
    let mut script_buffer: Vec<u8> = Vec::new();

    
    for func in program {
        let info = match db.find_by_name(&func.function_name) {
            Ok(f) => f,
            Err(name_error) => {
                let name = &func.function_name;
                match name.trim_start_matches("Unknown").parse() {
                    Ok(id) => db.find_by_id(id)?,
                    Err(_) => return Err(name_error),
                }
            }
        };

        log::trace!("building instruction `{}`", func.function_name.as_str());
        
        if func.total_size() as u32 != info.size {
            return Err(BBScriptError::IncorrectFunctionSize(
                func.function_name.to_string(),
                func.total_size(),
                info.size as usize,
            ));
        }

        script_buffer.write_u32::<LE>(info.id).unwrap();

        if info.is_jump_entry() {
            if let Some(ArgValue::String32(name)) = func.args.get(0) {
                jump_table.extend_from_slice(&name.to_vec());
                jump_table.write_u32::<LE>(offset).unwrap();
                table_entry_count += 1;
            }
        }

        for (index, arg) in func.args.iter().enumerate() {
            match arg {
                ArgValue::String32(string) => script_buffer.append(&mut string.to_vec()),
                ArgValue::String16(string) => script_buffer.append(&mut string.to_vec()),
                ArgValue::Raw(data) => script_buffer.append(&mut data.to_vec()),
                ArgValue::Int(num) => script_buffer.write_i32::<LE>(*num).unwrap(),
                ArgValue::Named(name) => {
                    let value = info
                        .get_value((index as u32, name.to_string()))
                        .expect("info.get_value call from named");
                    script_buffer.write_i32::<LE>(value).unwrap();
                }
            };
        }
        offset = script_buffer.len() as u32;
    }
    let mut result_vec = Vec::new();
    result_vec.write_u32::<LE>(table_entry_count).unwrap();
    result_vec.append(&mut jump_table);
    result_vec.append(&mut script_buffer);

    let result = Bytes::from(result_vec);

    Ok(result)
}

#[derive(Debug)]
struct BBSFunction {
    function_name: String,
    args: Vec<ArgValue>,
}

impl BBSFunction {
    pub fn total_size(&self) -> usize {
        const BASE_SIZE: usize = 0x4;

        let arg_size_sum: usize = self
            .args
            .iter()
            .map(|arg| match arg {
                ArgValue::String32(_) => 32,
                ArgValue::String16(_) => 16,
                ArgValue::Raw(bytes) => bytes.len(),
                _ => 4,
            })
            .sum();

        BASE_SIZE + arg_size_sum
    }
}

#[derive(Debug)]
enum ArgValue {
    String32(Bytes),
    String16(Bytes),
    Named(String),
    Int(i32),
    Raw(Bytes),
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
            [function_name(function_name), args(args)] => BBSFunction { function_name, args },
            [function_name(function_name)] => BBSFunction { function_name, args: none }
        );

        Ok(func)
    }

    fn function_name(input: Node) -> PResult<String> {
        Ok(input.as_str().into())
    }

    fn args(input: Node) -> PResult<Vec<ArgValue>> {
        Ok(match_nodes!(input.into_children();
            [arg(args)..,] => args.collect()
        ))
    }

    fn arg(input: Node) -> PResult<ArgValue> {
        Ok(match_nodes!(input.into_children();
            [string32(string)] => ArgValue::String32(string),
            [string16(string)] => ArgValue::String16(string),
            [named_value(name)] => ArgValue::Named(name),
            [raw_data(data)] => ArgValue::Raw(data),
            [num(val)] => ArgValue::Int(val),
        ))
    }

    fn string32(input: Node) -> PResult<Bytes> {
        Ok(string_to_bytes_of_size(input.as_str(), 32))
    }

    fn string16(input: Node) -> PResult<Bytes> {
        Ok(string_to_bytes_of_size(input.as_str(), 16))
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