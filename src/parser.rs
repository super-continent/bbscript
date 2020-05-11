use bytes::{BufMut, Bytes, BytesMut};

use std::error::Error;
use std::fs::File;

use crate::command_db::GameDB;

pub fn parse_bbscript(db: GameDB, input_file: Bytes) -> Result<(), Box<dyn Error>> {
    let mut out_buffer = BytesMut::new();
    Ok(())
}

fn identify_func() {
    unimplemented!()
}
