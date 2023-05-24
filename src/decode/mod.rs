use crate::decode::decoder::{Decoder, WasmDecoder};
use crate::module::WasmModule;
use anyhow::Result;
use std::io::Cursor;
use thiserror::Error;

mod data_decoding;
mod decoder;

// Constant for the magic bytes expected at the start of a valid WebAssembly binary
const HEADER_MAGIC_BYTES: [u8; 4] = [0x00, 0x61, 0x73, 0x64];
const FUNCTION_MAGIC_BYTES: [u8; 1] = [0x60];

#[derive(Error, Debug)]
pub enum DecodeError {
    // Constant for the magic bytes expected at the start of a valid WebAssembly binary
    #[error("Not a WebAssembly binary.")]
    Binary { expected: String, found: String },

    // Error variant representing an invalid numerical value
    #[error("An invalid numerical value was found while decoding the WebAssembly binary.")]
    Numeric {
        current_value: u32,
        invalid_byte: u32,
    },

    // Error variant for invalid magic bytes in type section
    #[error("The WebAssembly binary contains a type section with invalid magic bytes.")]
    TypeSectionBytes,

    // Error variant for invalid element types in a table type
    #[error("The WebAssembly binary contains a table type with an invalid element type.")]
    ElementType { invalid_byte: u8 },

    #[error("The WebAssembly binary contains a global type with an invalid mutability byte.")]
    MutabilityByte { invalid_byte: u8 },

    #[error("Unexpected WebAssembly OpCode received")]
    OpCode { opcode: u8 },
}

pub fn decode_bytes(bytes: &[u8]) -> Result<WasmModule> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    let mut module = WasmModule::default();

    decoder.read_validate()?;
    module.version = decoder.read_version()?;

    while !decoder.is_end {
        let (section_type, section_size) = decoder.decode_section_type()?;
        let section = decoder.decode_section(section_type, section_size)?;

        module.consume(section);
    }

    Ok(module)
}
