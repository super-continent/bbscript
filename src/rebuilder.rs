#![allow(clippy::upper_case_acronyms)]

use std::error::Error;

use crate::{command_db::GameDB, error::BBScriptError};
use crate::verbose;

use byteorder::{WriteBytesExt, LE};
use bytes::{Bytes, BytesMut};
use pest_consume::{match_nodes, Parser};

pub fn rebuild_bbscript(
    db: GameDB,
    script: String,
    verbose: bool,
) -> Result<Bytes, Box<dyn Error>> {
    let parsed = BBSParser::parse(Rule::program, &script)?;
    let root = parsed.single()?;

    verbose!(println!("Parsed program:\n{:#?}", &root), verbose);
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
                let name = func.function_name;
                match name.trim_start_matches("Unknown").parse() {
                    Ok(id) => db.find_by_id(id)?,
                    Err(_) => return Err(name_error)
                }
            }
        };

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
                    let value = info.get_value((index as u32, name.to_string())).unwrap();
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
        let mut string_bytes = BytesMut::from(input.as_str());
        string_bytes.resize(32, 0x0);
        Ok(string_bytes.freeze())
    }

    fn string16(input: Node) -> PResult<Bytes> {
        let mut string_bytes = BytesMut::from(input.as_str());
        string_bytes.resize(16, 0x0);
        Ok(string_bytes.freeze())
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
