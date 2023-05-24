use crate::decode::{DecodeError, FUNCTION_MAGIC_BYTES, HEADER_MAGIC_BYTES};
use crate::types::{
    GlobalType, ImportSection, MemoryType, Mutability, TableType, TypeSection, VarUInt,
    WasmElementType, WasmFunctionType, WasmImportDescriptor, WasmImportEntry, WasmLimits,
    WasmSection, WasmValueType,
};
use anyhow::Result;
use std::io::{Cursor, Read};

macro_rules! decode_dummy_section {
    ($name:ident, $section:ident, $docs:expr) => {
        doc_comment! {
            concat!("Decode a ", $docs, "section. This function will read from the provided reader and create a `WasmSection` variant corresponding section that contains no data."),
            fn $name(&mut self, size: u32) -> Result<WasmSection> {
                let mut custom_section = vec![0; size as usize];
                self.reader.read_exact(&mut custom_section)?;

                Ok(WasmSection::$section(()))
            }
        }
    };
}

macro_rules! read_bytes_const {
    ($reader:expr, $size:expr) => {{
        let mut buf = [0; $size];
        $reader.read_exact(&mut buf)?;
        buf
    }};
}

macro_rules! read_bytes {
    ($reader:expr, $size:expr) => {{
        let mut buf = vec![0; $size as usize];
        $reader.read_exact(&mut buf)?;
        buf
    }};
}

pub(crate) struct Decoder<'a> {
    reader: Cursor<&'a [u8]>,
}

impl<'a> Decoder<'a> {
    pub(crate) fn new(reader: Cursor<&'a [u8]>) -> Self {
        Self { reader }
    }

    fn decode_varuint(&mut self) -> Result<VarUInt> {
        let mut value = 0;

        for i in 0.. {
            let byte = read_bytes_const!(self.reader, 1)[0] as u32;
            let shifted = (byte & 0x7f)
                .checked_shl(i * 7)
                .ok_or(DecodeError::Numeric {
                    current_value: value,
                    invalid_byte: byte,
                })?;
            value += shifted;

            if byte & 0x80 == 0 {
                break;
            }
        }

        Ok(VarUInt::from(value))
    }
}

pub(crate) trait WasmDecoder<'a> {
    fn read_validate(&mut self) -> Result<()>;
    fn read_version(&mut self) -> Result<u32>;
    fn decode_type_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_custom_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_import_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_table_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_memory_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_global_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_start_section(&mut self, size: u32) -> Result<WasmSection>;
    fn decode_element_section(&mut self, size: u32) -> Result<WasmSection>;

    fn decode_table_type(&mut self) -> Result<TableType>;
    fn decode_memory_type(&mut self) -> Result<MemoryType>;
    fn decode_global_type(&mut self) -> Result<GlobalType>;
}

impl<'a> WasmDecoder<'a> for Decoder<'a> {
    fn read_validate(&mut self) -> Result<()> {
        let magic_bytes = read_bytes_const!(self.reader, 4);

        if magic_bytes == HEADER_MAGIC_BYTES {
            return Ok(());
        }

        Err(DecodeError::Binary {
            found: String::from_utf8(Vec::from(magic_bytes)).unwrap_or_default(),
            expected: String::from_utf8(Vec::from(HEADER_MAGIC_BYTES)).unwrap_or_default(),
        }
        .into())
    }

    fn read_version(&mut self) -> Result<u32> {
        let version_bytes = read_bytes_const!(self.reader, 4);
        Ok(u32::from_le_bytes(version_bytes))
    }

    /// Decode the type section of a WebAssembly binary.
    /// Layout:
    /// (1) type count (varuint)
    /// (2) type
    ///     - (3) magic header, 0x60
    ///     - (4) arg count (varuint)
    ///         - type (varuint)
    ///     - (5) returns count (varuint)
    ///         - type (varuint)
    ///
    fn decode_type_section(&mut self, size: u32) -> Result<WasmSection> {
        let section_bytes = read_bytes!(self.reader, size);

        let mut section_decoder = Decoder::new(Cursor::new(&section_bytes));

        let size: u32 = section_decoder.decode_varuint()?.into();
        let items = (0..size)
            .map(|_| {
                // "...Function types are encoded by the byte 0x60
                // followed by the respective vectors of parameter and result types."
                let type_magic_bytes = read_bytes_const!(section_decoder.reader, 1);
                if type_magic_bytes != FUNCTION_MAGIC_BYTES {
                    return Err(DecodeError::TypeSectionBytes.into());
                }

                let read_value_types =
                    |count: u32,
                     decoder: &mut Decoder|
                     -> Result<Vec<WasmValueType>, anyhow::Error> {
                        (0..count)
                            .map(|_| Ok(WasmValueType::from(decoder.decode_varuint()?)))
                            .collect()
                    };

                let param_count: u32 = section_decoder.decode_varuint()?.into();
                let return_count: u32 = section_decoder.decode_varuint()?.into();

                Ok(WasmFunctionType {
                    params: read_value_types(param_count, &mut section_decoder)?,
                    returns: read_value_types(return_count, &mut section_decoder)?,
                })
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()?;

        Ok(WasmSection::Type(TypeSection { items }))
    }

    decode_dummy_section!(decode_custom_section, Custom, "Custom");

    /// Decode the import section of a WebAssembly binary.
    ///
    /// Layout:
    /// (1) import count (varuint)
    /// (2) imports
    ///     - (3) module name length (varuint)
    ///     - (4) module name (string)
    ///     - (5) field name length (varuint)
    ///     - (6) field name (string)
    ///     - (7) import kind (byte)
    ///     - (8) import descriptor (based on kind)
    ///
    fn decode_import_section(&mut self, size: u32) -> Result<WasmSection> {
        let section_bytes = read_bytes!(self.reader, size);
        let mut section_decoder = Decoder::new(Cursor::new(&section_bytes));

        let count: u32 = section_decoder.decode_varuint()?.into();
        let mut items = Vec::new();

        for _ in 0..count {
            let module_name_length: u32 = section_decoder.decode_varuint()?.into();
            let module_name =
                String::from_utf8(read_bytes!(section_decoder.reader, module_name_length))?;

            let field_name_length: u32 = section_decoder.decode_varuint()?.into();
            let field_name =
                String::from_utf8(read_bytes!(section_decoder.reader, field_name_length))?;

            let import_kind = read_bytes_const!(section_decoder.reader, 1)[0];

            let import_descriptor = match import_kind {
                0x00 => WasmImportDescriptor::Function(section_decoder.decode_varuint()?),
                0x01 => WasmImportDescriptor::Table(section_decoder.decode_table_type()?),
                0x02 => WasmImportDescriptor::Memory(section_decoder.decode_memory_type()?),
                0x03 => WasmImportDescriptor::Global(section_decoder.decode_global_type()?),
                _ => return Err(anyhow::anyhow!("Invalid import kind: {}", import_kind)),
            };

            items.push(WasmImportEntry {
                module_name,
                field_name,
                descriptor: import_descriptor,
            });
        }

        Ok(WasmSection::Import(ImportSection { items }))
    }

    fn decode_table_type(&mut self) -> Result<TableType> {
        // The element type is represented by a byte. According to the WebAssembly specification,
        // 0x70 corresponds to `funcref` in the MVP.
        let element_type_byte = read_bytes_const!(self.reader, 1)[0];
        let element_type = match element_type_byte {
            0x70 => WasmElementType::Funcref,
            // In future versions of WebAssembly, there might be additional element types.
            _ => {
                return Err(DecodeError::ElementType {
                    invalid_byte: element_type_byte,
                }
                .into())
            }
        };

        // The limits are represented by a byte flag that indicates whether a maximum is present,
        // followed by the minimum (and optionally the maximum) represented as varuints.
        let flags = read_bytes_const!(self.reader, 1)[0];
        let min = self.decode_varuint()?.into();
        let max = if flags & 0x01 != 0 {
            Some(self.decode_varuint()?.into())
        } else {
            None
        };

        Ok(TableType {
            element_type,
            limits: WasmLimits { min, max },
        })
    }

    fn decode_memory_type(&mut self) -> Result<MemoryType> {
        // The limits are represented by a byte flag that indicates whether a maximum is present,
        // followed by the minimum (and optionally the maximum) represented as varuints.
        let flags = read_bytes_const!(self.reader, 1)[0];
        let min = self.decode_varuint()?.into();
        let max = if flags & 0x01 != 0 {
            Some(self.decode_varuint()?.into())
        } else {
            None
        };

        Ok(MemoryType {
            limits: WasmLimits { min, max },
        })
    }

    fn decode_global_type(&mut self) -> Result<GlobalType> {
        // The value type is represented by a single byte.
        let value_type_byte = read_bytes_const!(self.reader, 1)[0];
        let value_type = WasmValueType::from(VarUInt::from(value_type_byte as u32));

        // The mutability is represented by a single byte.
        let mutability_byte = read_bytes_const!(self.reader, 1)[0];
        let mutability = match mutability_byte {
            0x00 => Mutability::Immutable,
            0x01 => Mutability::Mutable,
            _ => {
                return Err(DecodeError::MutabilityByte {
                    invalid_byte: mutability_byte,
                }
                .into())
            }
        };

        Ok(GlobalType {
            value_type,
            mutability,
        })
    }

    decode_dummy_section!(decode_table_section, Table, "Table");
    decode_dummy_section!(decode_memory_section, Memory, "Memory");
    decode_dummy_section!(decode_global_section, Global, "Global");
    decode_dummy_section!(decode_start_section, Start, "Start");
    decode_dummy_section!(decode_element_section, Element, "Element");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_decoder() {
        let cursor = Cursor::new(&[1, 2, 3, 4][..]);
        let decoder = Decoder::new(cursor);
        assert_eq!(decoder.reader.get_ref().len(), 4);
    }

    #[test]
    fn test_decode_varuint() {
        let cursor = Cursor::new(&[0b10000001, 0b00000001][..]);
        let mut decoder = Decoder::new(cursor);
        let result = decoder.decode_varuint().unwrap();
        assert_eq!(u32::from(result), 129); // 128 (second byte) + 1 (first byte)
    }

    #[test]
    fn test_read_validate() {
        let cursor = Cursor::new(&HEADER_MAGIC_BYTES[..]);
        let mut decoder = Decoder::new(cursor);
        assert!(decoder.read_validate().is_ok());
    }

    #[test]
    fn test_read_validate_failed() {
        let cursor = Cursor::new(&[1, 2, 3, 4][..]);
        let mut decoder = Decoder::new(cursor);
        assert!(decoder.read_validate().is_err());
    }

    #[test]
    fn test_read_version() {
        let cursor = Cursor::new(&[1, 0, 0, 0][..]);
        let mut decoder = Decoder::new(cursor);
        let version = decoder.read_version().unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_decode_import_section() {
        // First, set up a valid encoded import section.
        // This represents a single import from a module named "mod" with field "field",
        // importing a function with type index 0.
        let data = [
            0x01, // Import count (1)
            0x03, 0x6d, 0x6f, 0x64, // Module name length (3), module name ("mod")
            0x05, 0x66, 0x69, 0x65, 0x6c, 0x64, // Field name length (5), field name ("field")
            0x00, // Import kind (0 = function)
            0x00, // Function type index (0)
        ];

        // Create a decoder with a cursor over the encoded data.
        let mut decoder = Decoder::new(Cursor::new(&data[..]));

        // Decode the import section.
        let import_section = decoder.decode_import_section(data.len() as u32).unwrap();

        // Check the result.
        match import_section {
            WasmSection::Import(import_section) => {
                assert_eq!(import_section.items.len(), 1);
                let import_entry = &import_section.items[0];
                assert_eq!(import_entry.module_name, "mod");
                assert_eq!(import_entry.field_name, "field");
                match import_entry.descriptor {
                    WasmImportDescriptor::Function(type_index) => {
                        assert_eq!(u32::from(type_index), 0);
                    }
                    _ => panic!("Expected function import descriptor"),
                }
            }
            _ => panic!("Expected import section"),
        }
    }
}
