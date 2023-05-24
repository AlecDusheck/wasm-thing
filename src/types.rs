/// AKA variable-length integer data (varuint).
/// Wasm uses LEB128 formatting for integers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarUInt(u32);

impl From<u32> for VarUInt {
    fn from(x: u32) -> Self {
        VarUInt(x)
    }
}

impl From<VarUInt> for u32 {
    fn from(value: VarUInt) -> u32 {
        value.0
    }
}

impl From<VarUInt> for WasmValueType {
    fn from(value: VarUInt) -> Self {
        let unsigned: u32 = value.into();
        WasmValueType::from(unsigned as u8)
    }
}

pub(crate) enum WasmSection {
    Type(TypeSection),
    Custom(()),
    Import(ImportSection),
    Table(()),
    Memory(()),
    Global(()),
    Start(()),
    Element(()),
}

pub enum WasmValueType {
    I32,
    I64,
    F32,
    F64,
    UNSUPPORTED,
}

impl From<u8> for WasmValueType {
    fn from(x: u8) -> Self {
        use WasmValueType::*;

        match x {
            0x7f => I32,
            0x7e => I64,
            0x7d => F32,
            0x7c => F64,
            _ => unreachable!("Unknown WashValueType: {:0x}", x),
        }
    }
}

pub struct TypeSection {
    pub(crate) items: Vec<WasmFunctionType>,
}

#[derive(Default)]
pub struct WasmFunctionType {
    pub(crate) params: Vec<WasmValueType>,
    pub(crate) returns: Vec<WasmValueType>,
}

pub(crate) enum WasmImportDescriptor {
    Function(VarUInt), // Index into the function types in the type section
    Table(TableType),
    Memory(MemoryType),
    Global(GlobalType),
}

/// WebAssembly Table Type
/// This type is defined by its element type (valtype) and a limits descriptor.
pub(crate) struct TableType {
    pub(crate) element_type: WasmElementType,
    pub(crate) limits: WasmLimits,
}

/// WebAssembly Memory Type
/// This type is defined by a limits descriptor.
pub(crate) struct MemoryType {
    pub(crate) limits: WasmLimits,
}

/// WebAssembly Global Type
/// This type is defined by its value type (valtype) and a mutability flag.
pub struct GlobalType {
    pub(crate) value_type: WasmValueType,
    pub(crate) mutability: Mutability,
}

/// WebAssembly Element Type
/// This is essentially the value type, restricted to funcref and externref.
pub(crate) enum WasmElementType {
    Funcref,
    Externref,
    // Additional types may be added in future WebAssembly extensions
}

/// WebAssembly Limits
/// This is defined by a minimum and an optional maximum.
pub(crate) struct WasmLimits {
    pub(crate) min: u32,
    pub(crate) max: Option<u32>,
}

pub(crate) struct WasmImportEntry {
    pub(crate) module_name: String,
    pub(crate) field_name: String,
    pub(crate) descriptor: WasmImportDescriptor,
}

pub(crate) struct ImportSection {
    pub(crate) items: Vec<WasmImportEntry>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Mutability {
    Immutable,
    Mutable,
}
