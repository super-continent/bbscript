use byteorder::{ByteOrder, ReadBytesExt};
use bytes::Buf;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use std::fmt::Write;
use std::io::Cursor;

use crate::game_config::{
    ArgType, BBSNumber, CodeBlock, Instruction, ScriptConfig, SizedInstruction, SizedString,
    TaggedValue, UnsizedInstruction,
};
use crate::BBScriptError;
use crate::HashMap;

const INDENT_SPACES: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArgValue {
    Unknown(SmallVec<[u8; 16]>),
    Number(BBSNumber),
    String16(SizedString<16>),
    String32(SizedString<32>),
    AccessedValue(TaggedValue),
    Enum(String, BBSNumber),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InstructionIdentifier {
    Name(String),
    Id(u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstructionValue {
    pub identifier: InstructionIdentifier,
    pub args: SmallVec<[ArgValue; 8]>,
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
                .unwrap_or(&val.to_string())
        )),
        ArgValue::AccessedValue(_tagged @ TaggedValue::Literal(val)) => Ok(format!("Val({val})")),
        ArgValue::Enum(name, val) => match config.named_value_maps.get(name) {
            Some(map) => map
                .get_by_left(val)
                .map_or(Ok(format!("{val}")), |name| Ok(format!("({name})"))),
            None => Err(BBScriptError::BadEnumReference(name.clone())),
        },
    }
}

impl ScriptConfig {
    pub fn parse_to_string<B: ByteOrder>(
        &self,
        input: impl AsRef<[u8]>,
        indent_limit: usize,
    ) -> Result<String, BBScriptError> {
        let program = self.parse::<B>(input.as_ref())?;
        let mut out = String::new();

        let mut last_block_type: Option<String> = None;
        let mut last_block_type_valid = false;
        let mut indent = 0;
        let mut block_ended = false;
        for instruction in program {
            let instruction_info = match instruction.identifier {
                InstructionIdentifier::Name(name) => self
                    .get_by_name(&name)
                    .ok_or(BBScriptError::UnknownInstructionName(name)),
                InstructionIdentifier::Id(id) => self
                    .get_by_id(id)
                    .ok_or(BBScriptError::UnknownInstructionID(id)),
            }?;

            match instruction_info.block_type() {
                CodeBlock::BeginNonrecursive => {
                    if last_block_type_valid && last_block_type == instruction_info.name() && indent > 0 {
                        indent -= 1;
                        last_block_type_valid = false;
                    }
                },
                CodeBlock::End => {
                    if indent > 0 {
                        last_block_type_valid = false;
                        indent -= 1;
                        if indent == 0 {
                            block_ended = true;
                        }
                    }
                }
                _ => {}
            }

            // indent the text
            out.write_fmt(format_args!(
                "{:indent$}",
                "",
                indent = (indent.clamp(0, indent_limit) * (INDENT_SPACES))
            ))?;

            let instruction_name = if let Some(name) = instruction_info.name() {
                name
            } else {
                format!("Unknown{}", instruction_info.id())
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

            match instruction_info.block_type() {
                CodeBlock::BeginNonrecursive | CodeBlock::Begin => {
                    indent += 1;
                    last_block_type = instruction_info.name();
                    last_block_type_valid = true;
                },
                _ => {}
            }

            if block_ended {
                out.write_char('\n')?;
                block_ended = false;
            }
        }

        Ok(out)
    }

    pub fn parse<B: ByteOrder>(
        &self,
        input: impl AsRef<[u8]>,
    ) -> Result<Vec<InstructionValue>, BBScriptError> {
        const JUMP_ENTRY_LENGTH: usize = 0x24;

        let mut input = input.as_ref();

        // get jump table size in bytes
        let jump_table_size: usize = JUMP_ENTRY_LENGTH
            * self
                .jump_table_ids
                .iter()
                .map(|_| input.read_u32::<B>().unwrap() as usize)
                .sum::<usize>();

        log::debug!("jump table size: {jump_table_size}");

        if jump_table_size >= input.len() {
            return Err(BBScriptError::IncorrectJumpTableSize(
                jump_table_size.to_string(),
            ));
        }

        input.advance(jump_table_size);

        // parse the actual scripts
        self.parse_script::<B>(input)
    }

    fn parse_script<B: ByteOrder>(
        &self,
        bytes: impl AsRef<[u8]>,
    ) -> Result<Vec<InstructionValue>, BBScriptError> {
        use crate::game_config::InstructionInfo;

        let mut input = Cursor::new(bytes.as_ref());
        let mut program = Vec::with_capacity(bytes.as_ref().len() / 2);

        match &self.instructions {
            InstructionInfo::Sized(id_map) => {
                while input.remaining() != 0 {
                    program.push(self.parse_sized::<B>(id_map, &mut input)?);
                }

                Ok(program)
            }
            InstructionInfo::Unsized(id_map) => {
                while input.remaining() != 0 {
                    program.push(self.parse_unsized::<B>(id_map, &mut input)?);
                }

                Ok(program)
            }
        }
    }

    fn parse_sized<B: ByteOrder>(
        &self,
        id_map: &HashMap<u32, SizedInstruction>,
        input: &mut Cursor<&[u8]>,
    ) -> Result<InstructionValue, BBScriptError> {
        let instruction_id = input.read_u32::<B>()?;

        let instruction = id_map
            .get(&instruction_id)
            .ok_or(BBScriptError::UnknownInstructionID(instruction_id))?;

        let instruction_identifier = if let Some(name) = instruction.name() {
            InstructionIdentifier::Name(name)
        } else {
            InstructionIdentifier::Id(instruction_id)
        };

        let args = instruction
            .args()
            .into_iter()
            .map(|arg_type| self.parse_argument::<B>(arg_type, input))
            .collect();

        let instruction = InstructionValue {
            identifier: instruction_identifier,
            args,
        };

        log::trace!("instruction: {:#?}", instruction);

        Ok(instruction)
    }

    fn parse_unsized<B: ByteOrder>(
        &self,
        id_map: &HashMap<u32, UnsizedInstruction>,
        input: &mut Cursor<&[u8]>,
    ) -> Result<InstructionValue, BBScriptError> {
        log::debug!("offset {:#X} from end of file", input.remaining());

        let instruction_id = input.read_u32::<B>().unwrap();
        let instruction_size = input.read_u32::<B>().unwrap();

        log::info!(
            "finding info for instruction with ID {instruction_id} and size {instruction_size}"
        );

        let instruction = if let Some(instruction) = id_map.get(&instruction_id) {
            instruction.clone()
        } else {
            log::warn!("instruction with ID {instruction_id} not in config!");
            UnsizedInstruction::new()
        };

        let instruction_identifier = if let Some(name) = instruction.name() {
            InstructionIdentifier::Name(name)
        } else {
            InstructionIdentifier::Id(instruction_id)
        };

        let args = instruction
            .args_with_known_size(instruction_size as usize)
            .into_iter()
            .map(|arg_type| self.parse_argument::<B>(arg_type, input))
            .collect();

        let instruction = InstructionValue {
            identifier: instruction_identifier,
            args,
        };
        log::trace!("instruction: {:#?}", instruction);

        Ok(instruction)
    }

    fn parse_argument<B: ByteOrder>(
        &self,
        arg_type: ArgType,
        input: &mut Cursor<&[u8]>,
    ) -> ArgValue {
        match arg_type {
            // get SmallVec of bytes
            ArgType::Unknown(n) => {
                ArgValue::Unknown((0..n).map(|_| input.read_u8().unwrap()).collect())
            }
            ArgType::String16 => {
                let mut buf = [0; ArgType::STRING16_SIZE];
                input.copy_to_slice(&mut buf);

                ArgValue::String16(SizedString(process_string_buf(&buf)))
            }
            ArgType::String32 => {
                let mut buf = [0; ArgType::STRING32_SIZE];
                input.copy_to_slice(&mut buf);

                ArgValue::String32(SizedString(process_string_buf(&buf)))
            }
            ArgType::Number => ArgValue::Number(input.read_i32::<B>().unwrap()),
            ArgType::Enum(s) => ArgValue::Enum(s.clone(), input.read_i32::<B>().unwrap()),
            ArgType::AccessedValue => {
                let tag = input.read_i32::<B>().unwrap();

                if tag == self.literal_tag {
                    ArgValue::AccessedValue(TaggedValue::Literal(input.read_i32::<B>().unwrap()))
                } else if tag == self.variable_tag {
                    ArgValue::AccessedValue(TaggedValue::Variable(input.read_i32::<B>().unwrap()))
                } else {
                    log::warn!(
                        "found improperly tagged AccessedValue, most likely just two Numbers"
                    );
                    ArgValue::AccessedValue(TaggedValue::Improper {
                        tag,
                        value: input.read_i32::<B>().unwrap(),
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
        .replace('\'', r"\'")
}
