use bytes::{Buf, Bytes, BytesMut};
use hex::encode_upper;

use std::fmt::Write;

use crate::command_db::{Arg, CodeBlock, GameDB};
use crate::verbose;
use crate::BBScriptError;

const INDENT_LIMIT: usize = 12;

pub fn parse_bbscript(
    db: GameDB,
    mut input_file: Bytes,
    verbose: bool,
) -> Result<BytesMut, BBScriptError> {
    let mut out_buffer = BytesMut::new();

    let state_table_entry_count = input_file.get_u32_le() as usize;
    let advance_amount = state_table_entry_count * 0x24;

    if advance_amount < input_file.len() {
        verbose!(
            println!("Jumping to offset `{:#X}`", advance_amount),
            verbose
        );
        input_file.advance(advance_amount);
    } else {
        return Err(BBScriptError::IncorrectJumpTableSize(
            advance_amount.to_string(),
        ));
    }

    let mut indent = 0;
    while input_file.remaining() != 0 {
        let instruction = input_file.get_u32_le();

        verbose!(
            println!(
                "Finding info for instruction `{:08X}` (ID: {}) at offset `{:#X}` from end of file",
                instruction,
                instruction,
                input_file.remaining()
            ),
            verbose
        );

        let instruction_info = db.find_by_id(instruction)?;
        let amount_to_indent = indent.clamp(0, INDENT_LIMIT);

        out_buffer
            .write_fmt(format_args!(
                "{:width$}{}: ",
                "",
                instruction_info.instruction_name(),
                width = amount_to_indent * 2
            ))
            .unwrap();

        // Determine if indented block was ended or was already indented 0 spaces, to make sure newlines applied only after indented blocks
        let mut block_ended = false;

        match instruction_info.code_block {
            CodeBlock::Begin => {
                indent += 1
            }
            CodeBlock::BeginJumpEntry => {
                indent += 1
            }
            CodeBlock::End => {
                if indent > 0 {
                    indent -= 1;
                    if indent == 0 {
                        block_ended = true;
                    }
                }
            }
            CodeBlock::NoBlock => block_ended = false,
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
                    out_buffer
                        .write_fmt(format_args!(
                            "s32'{}'",
                            buf.iter()
                                .filter(|x| **x != 0)
                                .map(|x| *x as char)
                                .collect::<String>()
                        ))
                        .unwrap();
                }
                Arg::String16 => {
                    let mut buf = [0; 16];
                    input_file.copy_to_slice(&mut buf);
                    out_buffer
                        .write_fmt(format_args!(
                            "s16'{}'",
                            buf.iter()
                                .filter(|x| **x != 0)
                                .map(|x| *x as char)
                                .collect::<String>()
                        ))
                        .unwrap();
                }
                Arg::Int => {
                    let num = input_file.get_i32_le();
                    if let Some(name) = instruction_info.get_name((arg_index, num)) {
                        out_buffer.write_fmt(format_args!("({})", name)).unwrap();
                    } else {
                        out_buffer.write_fmt(format_args!("{}", num)).unwrap();
                    }
                }
                Arg::Unknown(size) => {
                    let mut buf = Vec::new();
                    for _ in 0..*size {
                        buf.push(input_file.get_u8());
                    }
                    out_buffer
                        .write_fmt(format_args!("'0x{}'", encode_upper(buf)))
                        .unwrap();
                }
            };

            if index != args_length - 1 {
                out_buffer.write_fmt(format_args!(", ")).unwrap();
            }
        }

        if !block_ended {
            out_buffer.write_char('\n').unwrap();
        } else {
            out_buffer.write_str("\n\n").unwrap();
        }
    }
    Ok(out_buffer)
}
