use serde::{Deserialize, Serialize};
use serde_json;

enum Arg {
    String16,
    String32,
    Int,
    Unknown(usize),
}

struct Instruction {
    name: String,
    arguments: Vec<Arg>,
}

pub fn parse_bbscript() {}

fn identify_func() -> Option<Instruction> {
    unimplemented!()
}
