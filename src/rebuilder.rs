use crate::command_db;
use byteorder;
struct BBScript {
    function_count: u32,
    jump_table: Vec<JumpEntry>,
}

struct JumpEntry {
    name: String,
    jump_offset: u32,
}
