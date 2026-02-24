//! Bytecode file format for SubsetJuliaVM.
//!
//! This module provides serialization and deserialization of Julia programs
//! to a binary format (.sjbc files). The format stores the Core IR representation
//! which can be loaded and compiled to either VM bytecode or AoT native code.
//!
//! # File Format
//!
//! ```text
//! +------------------+
//! | Magic (4 bytes)  |  "SJBC"
//! +------------------+
//! | Version (4 bytes)|  u32 format version
//! +------------------+
//! | Flags (4 bytes)  |  u32 feature flags
//! +------------------+
//! | IR Length (4 b)  |  u32 length of serialized IR
//! +------------------+
//! | IR Data (N bytes)|  bincode-serialized Core IR
//! +------------------+
//! ```
//!
//! # Usage
//!
//! The bytecode module provides save/load functionality for compiled programs:
//!
//! ```no_run
//! use subset_julia_vm::bytecode;
//! use subset_julia_vm::ir::core::Program;
//!
//! // After compiling Julia source to a Program (via lowering)
//! // let program: Program = compile_julia_source("function f(x) x + 1 end");
//!
//! // Save the program to a bytecode file
//! // bytecode::save(&program, "output.sjbc").expect("Failed to save");
//!
//! // Later, load the program from bytecode
//! // let loaded = bytecode::load("output.sjbc").expect("Failed to load");
//! ```
//!
//! See the `sjulia` CLI for a complete example of bytecode compilation and loading.

use crate::ir::core::Program;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Magic bytes identifying a SubsetJuliaVM bytecode file
pub const MAGIC: &[u8; 4] = b"SJBC";

/// Current bytecode format version
pub const VERSION: u32 = 2;

/// Bytecode format error
#[derive(Debug)]
pub enum BytecodeError {
    /// I/O error during file operations
    IoError(std::io::Error),
    /// Invalid magic bytes - not a valid bytecode file
    InvalidMagic,
    /// Unsupported format version
    UnsupportedVersion(u32),
    /// Deserialization error
    DeserializeError(String),
    /// Serialization error
    SerializeError(String),
}

impl std::fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeError::IoError(e) => write!(f, "I/O error: {}", e),
            BytecodeError::InvalidMagic => {
                write!(f, "Invalid magic bytes - not a valid .sjbc file")
            }
            BytecodeError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "Unsupported bytecode version: {} (current: {})",
                    v, VERSION
                )
            }
            BytecodeError::DeserializeError(e) => write!(f, "Failed to deserialize: {}", e),
            BytecodeError::SerializeError(e) => write!(f, "Failed to serialize: {}", e),
        }
    }
}

impl std::error::Error for BytecodeError {}

impl From<std::io::Error> for BytecodeError {
    fn from(e: std::io::Error) -> Self {
        BytecodeError::IoError(e)
    }
}

/// Bytecode file flags
#[derive(Debug, Clone, Copy, Default)]
pub struct BytecodeFlags {
    /// Whether the file includes debug information
    pub has_debug_info: bool,
    /// Whether the file includes source spans
    pub has_spans: bool,
    /// Reserved for future use
    _reserved: u16,
}

impl BytecodeFlags {
    /// Create default flags (all features enabled for compatibility)
    pub fn default_flags() -> Self {
        Self {
            has_debug_info: true,
            has_spans: true,
            _reserved: 0,
        }
    }

    /// Encode flags to u32
    fn to_u32(&self) -> u32 {
        let mut flags: u32 = 0;
        if self.has_debug_info {
            flags |= 1 << 0;
        }
        if self.has_spans {
            flags |= 1 << 1;
        }
        flags
    }

    /// Decode flags from u32
    fn from_u32(value: u32) -> Self {
        Self {
            has_debug_info: (value & (1 << 0)) != 0,
            has_spans: (value & (1 << 1)) != 0,
            _reserved: 0,
        }
    }
}

/// Bytecode file header
#[derive(Debug)]
pub struct BytecodeHeader {
    /// Format version
    pub version: u32,
    /// Feature flags
    pub flags: BytecodeFlags,
    /// Length of the serialized IR data
    pub ir_length: u32,
}

/// Save a Program to a bytecode file
///
/// # Arguments
///
/// * `program` - The Core IR program to save
/// * `path` - Output file path (should end in .sjbc)
///
/// # Returns
///
/// Returns Ok(()) on success, or a BytecodeError on failure.
pub fn save<P: AsRef<Path>>(program: &Program, path: P) -> Result<(), BytecodeError> {
    save_with_flags(program, path, BytecodeFlags::default_flags())
}

/// Save a Program to a bytecode file with custom flags
pub fn save_with_flags<P: AsRef<Path>>(
    program: &Program,
    path: P,
    flags: BytecodeFlags,
) -> Result<(), BytecodeError> {
    // Serialize the program to binary format (bincode)
    let ir_bytes =
        bincode::serialize(program).map_err(|e| BytecodeError::SerializeError(e.to_string()))?;

    let mut file = File::create(path)?;

    // Write header
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    file.write_all(&flags.to_u32().to_le_bytes())?;
    file.write_all(&(ir_bytes.len() as u32).to_le_bytes())?;

    // Write serialized IR
    file.write_all(&ir_bytes)?;

    Ok(())
}

/// Load a Program from a bytecode file
///
/// # Arguments
///
/// * `path` - Input file path
///
/// # Returns
///
/// Returns the loaded Program on success, or a BytecodeError on failure.
pub fn load<P: AsRef<Path>>(path: P) -> Result<Program, BytecodeError> {
    let (program, _header) = load_with_header(path)?;
    Ok(program)
}

/// Load a Program and header from a bytecode file
pub fn load_with_header<P: AsRef<Path>>(
    path: P,
) -> Result<(Program, BytecodeHeader), BytecodeError> {
    let mut file = File::open(path)?;

    // Read and verify magic
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(BytecodeError::InvalidMagic);
    }

    // Read version
    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version > VERSION {
        return Err(BytecodeError::UnsupportedVersion(version));
    }

    // Read flags
    let mut flags_bytes = [0u8; 4];
    file.read_exact(&mut flags_bytes)?;
    let flags = BytecodeFlags::from_u32(u32::from_le_bytes(flags_bytes));

    // Read IR length
    let mut length_bytes = [0u8; 4];
    file.read_exact(&mut length_bytes)?;
    let ir_length = u32::from_le_bytes(length_bytes);

    // Read IR data
    let mut ir_bytes = vec![0u8; ir_length as usize];
    file.read_exact(&mut ir_bytes)?;

    // Deserialize program
    let program: Program = bincode::deserialize(&ir_bytes)
        .map_err(|e| BytecodeError::DeserializeError(e.to_string()))?;

    let header = BytecodeHeader {
        version,
        flags,
        ir_length,
    };

    Ok((program, header))
}

/// Load bytecode from raw bytes (for embedded/in-memory use)
pub fn load_from_bytes(data: &[u8]) -> Result<Program, BytecodeError> {
    if data.len() < 16 {
        return Err(BytecodeError::InvalidMagic);
    }

    // Verify magic
    if &data[0..4] != MAGIC {
        return Err(BytecodeError::InvalidMagic);
    }

    // Read version
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version > VERSION {
        return Err(BytecodeError::UnsupportedVersion(version));
    }

    // Read IR length
    let ir_length = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

    // Verify data length
    if data.len() < 16 + ir_length {
        return Err(BytecodeError::DeserializeError(
            "Truncated data".to_string(),
        ));
    }

    // Deserialize program
    let program: Program = bincode::deserialize(&data[16..16 + ir_length])
        .map_err(|e| BytecodeError::DeserializeError(e.to_string()))?;

    Ok(program)
}

/// Serialize a Program to bytes (for in-memory use)
pub fn save_to_bytes(program: &Program) -> Result<Vec<u8>, BytecodeError> {
    let flags = BytecodeFlags::default_flags();

    // Serialize the program
    let ir_bytes =
        bincode::serialize(program).map_err(|e| BytecodeError::SerializeError(e.to_string()))?;

    let mut result = Vec::with_capacity(16 + ir_bytes.len());

    // Write header
    result.extend_from_slice(MAGIC);
    result.extend_from_slice(&VERSION.to_le_bytes());
    result.extend_from_slice(&flags.to_u32().to_le_bytes());
    result.extend_from_slice(&(ir_bytes.len() as u32).to_le_bytes());

    // Write IR data
    result.extend_from_slice(&ir_bytes);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Program};
    use crate::span::Span;

    fn empty_program() -> Program {
        Program {
            abstract_types: vec![],
            type_aliases: vec![],
            structs: vec![],
            functions: vec![],
            base_function_count: 0,
            modules: vec![],
            usings: vec![],
            macros: vec![],
            enums: vec![],
            main: Block {
                stmts: vec![],
                span: Span::new(0, 0, 1, 1, 0, 0),
            },
        }
    }

    #[test]
    fn test_save_load_bytes() {
        let program = empty_program();
        let bytes = save_to_bytes(&program).unwrap();
        let loaded = load_from_bytes(&bytes).unwrap();
        assert_eq!(program, loaded);
    }

    #[test]
    fn test_magic_bytes() {
        let program = empty_program();
        let bytes = save_to_bytes(&program).unwrap();
        assert_eq!(&bytes[0..4], MAGIC);
    }

    #[test]
    fn test_version() {
        let program = empty_program();
        let bytes = save_to_bytes(&program).unwrap();
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(version, VERSION);
    }

    #[test]
    fn test_invalid_magic() {
        let invalid_data = b"XXXX\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let result = load_from_bytes(invalid_data);
        assert!(matches!(result, Err(BytecodeError::InvalidMagic)));
    }

    #[test]
    fn test_unsupported_version() {
        // Create data with future version
        let mut data = vec![];
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&999u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let result = load_from_bytes(&data);
        assert!(matches!(
            result,
            Err(BytecodeError::UnsupportedVersion(999))
        ));
    }
}
