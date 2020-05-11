use bytes::{Buf, Bytes, BytesMut};

use std::error::Error;
use std::fmt::Write;

use crate::command_db::{Arg, GameDB, Indentation};
use crate::verbose;
use crate::BBScriptError;

pub fn parse_bbscript(
    db: GameDB,
    mut input_file: Bytes,
    verbose: bool,
) -> Result<BytesMut, Box<dyn Error>> {
    let mut out_buffer = BytesMut::new();

    let state_table_entry_count = input_file.get_u32_le() as usize;
    let advance = state_table_entry_count * 0x24;

    if advance < input_file.len() {
        verbose!(println!("Jumping to offset `{:#X}`", advance), verbose);
        input_file.advance(advance);
    } else {
        return Err(Box::new(BBScriptError::IncorrectJumpTableSize(
            advance.to_string(),
        )));
    }

    let mut indentation = 0;
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

        out_buffer.write_fmt(format_args!(
            "{:>width$} ",
            instruction_info.instruction_name(),
            width = indentation
        ))?;

        match instruction_info.code_block {
            Indentation::Begin => indentation += 4,
            Indentation::End => {
                if indentation >= 4 {
                    indentation -= 4
                };
            },
            Indentation::None => {},
        }

        verbose!(
            println!("Got instruction: {:#?}", instruction_info),
            verbose
        );

        let args = instruction_info.get_args();
        let args = args.iter();

        verbose!(println!("Found Args: {:?}", args), verbose);
        let arg_index = 0;
        for arg in args {
            match arg {
                Arg::String32 => {
                    let mut buf = [0; 32];
                    input_file.copy_to_slice(&mut buf);
                    out_buffer.write_fmt(format_args!(
                        "'{}', ",
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
                        "'{}', ",
                        buf.iter()
                            .filter(|x| **x != 0)
                            .map(|x| *x as char)
                            .collect::<String>()
                    ))?;
                }
                Arg::Int => {
                    let num = input_file.get_i32_le();
                    if let Some(name) = instruction_info.get_name((arg_index, num)) {
                        out_buffer.write_fmt(format_args!("{}, ", name))?;
                    } else {
                        out_buffer.write_fmt(format_args!("{}, ", num))?;
                    }
                }
                Arg::Unknown(size) => {
                    let mut buf: Vec<u8> = Vec::with_capacity(*size as usize);
                    input_file.copy_to_slice(&mut buf);
                    input_file.advance(*size as usize);
                    out_buffer.write_fmt(format_args!(
                        "0x{} ", // FIXME: Fix hex output for unknown arguments
                        buf.iter()
                            .map(|x| format!("{:#>2X}", x) )
                            .collect::<String>()
                    ))?;
                }
            };
        }
        out_buffer.write_char('\n')?;
    }
    Ok(out_buffer)
}
