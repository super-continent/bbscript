use bytes::{Buf, Bytes, BytesMut};
use hex::encode_upper;

use std::error::Error;
use std::fmt::Write;

use crate::command_db::{Arg, GameDB, Indentation};
use crate::verbose;
use crate::BBScriptError;

const INDENT_LIMIT: usize = 6;

pub fn parse_bbscript(
    db: GameDB,
    mut input_file: Bytes,
    verbose: bool,
) -> Result<BytesMut, Box<dyn Error>> {
    let mut out_buffer = BytesMut::new();

    let state_table_entry_count = input_file.get_u32_le() as usize;
    let advance_amount = state_table_entry_count * 0x24;

    if advance_amount < input_file.len() {
        verbose!(println!("Jumping to offset `{:#X}`", advance_amount), verbose);
        input_file.advance(advance_amount);
    } else {
        return Err(Box::new(BBScriptError::IncorrectJumpTableSize(
            advance_amount.to_string(),
        )));
    }

    let mut indent = 0;
    while input_file.remaining() != 0 {
        let instruction = input_file.get_u32_le();

        verbose!(
            println!(
                "Finding info for instruction `{:08X}` at offset `{:#X}` from end of file",
                instruction,
                input_file.remaining()
            ),
            verbose
        );

        let instruction_info = db.find_by_id(instruction)?;

        out_buffer.write_fmt(format_args!("{:width$}{}: ", "", instruction_info.instruction_name(), width = indent * 4))?;

        // Determine if indented block was ended or was already indented 0 spaces, to make sure newlines applied only after indented blocks
        let mut block_ended = false;

        match instruction_info.code_block {
            Indentation::Begin => {
                if indent < INDENT_LIMIT {
                    indent += 1
                }
            },
            Indentation::BeginJumpEntry => {
                if indent < INDENT_LIMIT {
                    indent += 1
                }
            },
            Indentation::End => {
                if indent > 0 {
                    indent -= 1;
                    if indent == 0 {
                        block_ended = true;
                    }
                }
            },
            Indentation::None => {
                block_ended = false
            },
        }

        verbose!(
            println!("Got instruction: {:#?}", instruction_info),
            verbose
        );

        let args = instruction_info.get_args();
        let args_length = args.len();
        let args = args.iter();

        verbose!(println!("Found Args: {:?}", args), verbose);
        for (index, arg) in args.enumerate() {
            let arg_index = index as u32;
            match arg {
                Arg::String32 => {
                    let mut buf = [0; 32];
                    input_file.copy_to_slice(&mut buf);
                    out_buffer.write_fmt(format_args!(
                        "s32'{}'",
                        buf.iter()
                            .filter(|x| **x != 0)
                            .map(|x| *x as char)
                            .collect::<String>()
                    ))?;
                }
                Arg::String16 => {
                    let mut buf = [0; 16];
                    input_file.copy_to_slice(&mut buf);
                    out_buffer.write_fmt(format_args!(
                        "s16'{}'",
                        buf.iter()
                            .filter(|x| **x != 0)
                            .map(|x| *x as char)
                            .collect::<String>()
                    ))?;
                }
                Arg::Int => {
                    let num = input_file.get_i32_le();
                    if let Some(name) = instruction_info.get_name((arg_index, num)) {
                        out_buffer.write_fmt(format_args!("({})", name))?;
                    } else {
                        out_buffer.write_fmt(format_args!("{}", num))?;
                    }
                }
                Arg::Unknown(size) => {
                    let mut buf = Vec::new();
                    for _ in 0..*size {
                        buf.push(input_file.get_u8());
                    };
                    out_buffer.write_fmt(format_args!("'0x{}'", encode_upper(buf)))?;
                }
            };

            if index != args_length - 1 {
                out_buffer.write_fmt(format_args!(", "))?;
            }
        }
        if !block_ended{
            out_buffer.write_char('\n')?;
        } else {
            out_buffer.write_str("\n\n")?;
        }
    }
    Ok(out_buffer)
}
