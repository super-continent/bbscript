use bytes::{Buf, Bytes};
use smallvec::SmallVec;

use std::fmt::Write;

use crate::game_config::{
    ArgType, BBSNumber, CodeBlock, ScriptConfig, SizedInstruction, TaggedValue, UnsizedInstruction,
};
use crate::BBScriptError;
use crate::HashMap;

const INDENT_LIMIT: usize = 12;
const INDENT_SPACES: usize = 2;

#[derive(Debug, Clone)]
pub enum ArgValue {
    Unknown(SmallVec<[u8; 128]>),
    Number(BBSNumber),
    String16(String),
    String32(String),
    AccessedValue(TaggedValue),
    Enum(String, BBSNumber),
}

#[derive(Debug, Clone)]
pub struct InstructionValue {
    pub id: u32,
    pub name: Option<String>,
    pub args: SmallVec<[ArgValue; 16]>,
    pub code_block: CodeBlock,
}

fn arg_to_string(config: &ScriptConfig, arg: &ArgValue) -> Result<String, BBScriptError> {
    match arg {
        ArgValue::Unknown(data) => Ok(format!("0x{}", hex::encode_upper(data))),
        ArgValue::Number(num) => Ok(format!("{num}")),
        ArgValue::String16(s) => Ok(format!("s16'{s}'")),
        ArgValue::String32(s) => Ok(format!("s32'{s}'")),
        ArgValue::AccessedValue(_tagged @ TaggedValue::Improper { tag, value }) => {
            Ok(format!("BadTag({tag}, {value})"))
        }
        // get named value
        ArgValue::AccessedValue(_tagged @ TaggedValue::Variable(val)) => Ok(format!(
            "Mem({})",
            config
                .named_variables
                .get_by_left(val)
                .unwrap_or(&format!("{val}"))
        )),
        ArgValue::AccessedValue(_tagged @ TaggedValue::Literal(val)) => Ok(format!("Val({val})")),
        ArgValue::Enum(name, val) => match config.named_value_maps.get(name) {
            Some(map) => map
                .get_by_left(val)
                .map_or(Ok(format!("{val}")), |name| Ok(format!("({name})"))),
            None => return Err(BBScriptError::BadEnumReference(name.clone())),
        },
    }
}

impl ScriptConfig {
    pub fn parse_to_string<T: Into<Bytes>>(&self, input: T) -> Result<String, BBScriptError> {
        let program = self.parse(input)?;
        let mut out = String::new();

        let mut indent = 0;
        let mut block_ended = false;
        for instruction in program {
            // indent the text
            out.write_fmt(format_args!(
                "{:indent$}",
                "",
                indent = (indent.clamp(0, INDENT_LIMIT) * (INDENT_SPACES))
            ))?;

            let instruction_name = if let Some(name) = instruction.name {
                name
            } else {
                format!("Unknown{}", instruction.id)
            };

            out.write_fmt(format_args!("{}: ", instruction_name))?;

            let mut args = instruction.args.iter().peekable();
            while let Some(arg) = args.next() {
                out.write_fmt(format_args!("{}", arg_to_string(self, arg)?))?;

                if args.peek().is_some() {
                    out.write_fmt(format_args!(", "))?;
                }
            }

            out.write_char('\n')?;

            match instruction.code_block {
                CodeBlock::Begin => indent += 1,
                CodeBlock::End => {
                    if indent > 0 {
                        indent -= 1;
                        if indent == 0 {
                            block_ended = true;
                        }
                    }
                }
                _ => {}
            }

            if block_ended {
                out.write_char('\n')?;
                block_ended = false;
            }
        }

        Ok(out)
    }

    pub fn parse<T: Into<Bytes>>(&self, input: T) -> Result<Vec<InstructionValue>, BBScriptError> {
        const JUMP_ENTRY_LENGTH: usize = 0x24;

        let mut input: Bytes = input.into();

        // get jump table size in bytes
        let jump_table_size: usize = JUMP_ENTRY_LENGTH
            * self
                .jump_table_ids
                .iter()
                .map(|_| input.get_u32_le() as usize)
                .sum::<usize>();

        log::debug!("jump table size: {jump_table_size}");

        if jump_table_size as usize >= input.len() {
            return Err(BBScriptError::IncorrectJumpTableSize(
                jump_table_size.to_string(),
            ));
        }

        input.advance(jump_table_size as usize);

        // parse the actual scripts
        self.parse_script(&mut input)
    }

    fn parse_script(&self, input: &mut Bytes) -> Result<Vec<InstructionValue>, BBScriptError> {
        use crate::game_config::InstructionInfo;
        match &self.instructions {
            InstructionInfo::Sized(id_map) => {
                let mut program = Vec::with_capacity(input.len() / 2);

                while input.remaining() != 0 {
                    program.push(self.parse_sized(id_map, input)?);
                }

                Ok(program)
            }
            InstructionInfo::Unsized(id_map) => {
                let mut program = Vec::with_capacity(input.len() / 2);

                while input.remaining() != 0 {
                    program.push(self.parse_unsized(id_map, input)?);
                }

                Ok(program)
            }
        }
    }

    fn parse_sized(
        &self,
        id_map: &HashMap<u32, SizedInstruction>,
        input: &mut Bytes,
    ) -> Result<InstructionValue, BBScriptError> {
        let instruction_id = input.get_u32_le();

        let instruction = if let Some(instruction) = id_map.get(&instruction_id) {
            instruction
        } else {
            return Err(BBScriptError::UnknownInstructionID(instruction_id));
        };

        let instruction_name = if instruction.name.is_empty() {
            None
        } else {
            Some(instruction.name.clone())
        };

        let args = instruction
            .args()
            .into_iter()
            .map(|arg_type| self.parse_argument(arg_type, input))
            .collect();

        let instruction = InstructionValue {
            id: instruction_id,
            name: instruction_name,
            args,
            code_block: instruction.code_block,
        };
        log::trace!("instruction: {:#?}", instruction);

        Ok(instruction)
    }

    fn parse_unsized(
        &self,
        id_map: &HashMap<u32, UnsizedInstruction>,
        input: &mut Bytes,
    ) -> Result<InstructionValue, BBScriptError> {
        log::debug!("offset {:#X} from end of file", input.remaining());

        let instruction_id = input.get_u32_le();
        let instruction_size = input.get_u32_le();

        log::info!(
            "finding info for instruction with ID {instruction_id} and size {instruction_size}"
        );

        let instruction = if let Some(instruction) = id_map.get(&instruction_id) {
            instruction.clone()
        } else {
            log::warn!("instruction with ID {instruction_id} not in config!");
            UnsizedInstruction::new()
        };

        let instruction_name = if instruction.name.is_empty() {
            None
        } else {
            Some(instruction.name.clone())
        };

        let args = instruction
            .args_with_known_size(instruction_size as usize)
            .into_iter()
            .map(|arg_type| self.parse_argument(arg_type, input))
            .collect();

        let instruction = InstructionValue {
            id: instruction_id,
            name: instruction_name,
            args,
            code_block: instruction.code_block,
        };
        log::trace!("instruction: {:#?}", instruction);

        Ok(instruction)
    }

    fn parse_argument(&self, arg_type: ArgType, input: &mut Bytes) -> ArgValue {
        match arg_type {
            // get SmallVec of bytes
            ArgType::Unknown(n) => ArgValue::Unknown((0..n).map(|_| input.get_u8()).collect()),
            ArgType::String16 => {
                let mut buf = [0; ArgType::STRING16_SIZE];
                input.copy_to_slice(&mut buf);

                ArgValue::String16(process_string_buf(&buf))
            }
            ArgType::String32 => {
                let mut buf = [0; ArgType::STRING32_SIZE];
                input.copy_to_slice(&mut buf);

                ArgValue::String32(process_string_buf(&buf))
            }
            ArgType::Number => ArgValue::Number(input.get_i32_le()),
            ArgType::Enum(s) => ArgValue::Enum(s.clone(), input.get_i32_le()),
            ArgType::AccessedValue => {
                let tag = input.get_i32_le();

                if tag == self.literal_tag {
                    ArgValue::AccessedValue(TaggedValue::Literal(input.get_i32_le()))
                } else if tag == self.variable_tag {
                    ArgValue::AccessedValue(TaggedValue::Variable(input.get_i32_le()))
                } else {
                    log::warn!(
                        "found improperly tagged AccessedValue, most likely just two Numbers"
                    );
                    ArgValue::AccessedValue(TaggedValue::Improper {
                        tag,
                        value: input.get_i32_le(),
                    })
                }
            }
        }
    }
}

fn process_string_buf(buf: &[u8]) -> String {
    buf.iter()
        .filter(|x| **x != 0)
        .map(|x| *x as char)
        .collect::<String>()
        .replace(r"'", r"\'")
}
