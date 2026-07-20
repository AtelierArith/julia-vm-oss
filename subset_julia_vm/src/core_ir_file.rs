//! Persisted Core IR file format for SubsetJuliaVM.
//!
//! `.sjir` stores the lowered Core IR representation, which can be loaded and
//! compiled to either VM bytecode or AoT native code.
//!
//! # Core IR File Format (`.sjir`)
//!
//! ```text
//! +------------------+
//! | Magic (4 bytes)  |  "SJIR"
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
//! This module provides save/load functionality for persisted lowered programs:
//!
//! ```no_run
//! use subset_julia_vm::core_ir_file;
//! use subset_julia_vm::ir::core::Program;
//!
//! // After compiling Julia source to a Program (via lowering)
//! // let program: Program = compile_julia_source("function f(x) x + 1 end");
//!
//! // Save the lowered program to a Core IR file
//! // core_ir_file::save(&program, "output.sjir").expect("Failed to save");
//!
//! // Later, load the lowered program from Core IR
//! // let loaded = core_ir_file::load("output.sjir").expect("Failed to load");
//! ```
//!
//! See the `sjulia` CLI for complete examples of Core IR compilation/loading.

// Issue #10906 (Phase 1c of #10869): the `.sjir` cache-load boundary — zero
// real unwrap_used/expect_used sites in production code (the two expect_used
// token matches the static scan finds are inside the `//!` doc-comment usage
// example above, not executable code; every real unwrap/expect match is
// inside the cfg(test) module, which carries an explicit allow).
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use crate::ir::core::Program;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Magic bytes identifying a SubsetJuliaVM Core IR file.
pub const MAGIC: &[u8; 4] = b"SJIR";

/// Current persisted Core IR file format version.
/// Version 8 adds explicit Base/package provenance to serialized modules;
/// version 7 gives lowering-generated callables explicit private-helper
/// provenance; version 6 adds recovered enum-member publication state to
/// `Stmt::EnumDef`;
/// version 5 requires package fragments to carry centrally composed definition
/// chronology.
pub const VERSION: u32 = 8;

/// Persisted Core IR file format error.
#[derive(Debug)]
pub enum CoreIrFileError {
    /// I/O error during file operations
    IoError(std::io::Error),
    /// Invalid magic bytes - not a valid SubsetJuliaVM program file
    InvalidMagic,
    /// Unsupported format version
    UnsupportedVersion(u32),
    /// Deserialization error
    DeserializeError(String),
    /// Serialization error
    SerializeError(String),
}

impl std::fmt::Display for CoreIrFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreIrFileError::IoError(e) => write!(f, "I/O error: {}", e),
            CoreIrFileError::InvalidMagic => write!(
                f,
                "Invalid magic bytes - not a valid SubsetJuliaVM Core IR file"
            ),
            CoreIrFileError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "Unsupported Core IR file version: {} (current: {})",
                    v, VERSION
                )
            }
            CoreIrFileError::DeserializeError(e) => write!(f, "Failed to deserialize: {}", e),
            CoreIrFileError::SerializeError(e) => write!(f, "Failed to serialize: {}", e),
        }
    }
}

impl std::error::Error for CoreIrFileError {}

impl From<std::io::Error> for CoreIrFileError {
    fn from(e: std::io::Error) -> Self {
        CoreIrFileError::IoError(e)
    }
}

/// On-disk Core IR file flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct CoreIrFileFlags {
    /// Whether the file includes debug information
    pub has_debug_info: bool,
    /// Whether the file includes source spans
    pub has_spans: bool,
    /// Reserved for future use
    _reserved: u16,
}

impl CoreIrFileFlags {
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

/// Core IR file header.
#[derive(Debug)]
pub struct CoreIrFileHeader {
    /// Format version
    pub version: u32,
    /// Feature flags
    pub flags: CoreIrFileFlags,
    /// Length of the serialized IR data
    pub ir_length: u32,
}

/// Save a lowered Program to a Core IR file.
///
/// # Arguments
///
/// * `program` - The Core IR program to save
/// * `path` - Output file path (should end in .sjir)
///
/// # Returns
///
/// Returns Ok(()) on success, or a CoreIrFileError on failure.
pub fn save<P: AsRef<Path>>(program: &Program, path: P) -> Result<(), CoreIrFileError> {
    save_with_flags(program, path, CoreIrFileFlags::default_flags())
}

/// Save a lowered Program to a Core IR file with custom flags.
pub fn save_with_flags<P: AsRef<Path>>(
    program: &Program,
    path: P,
    flags: CoreIrFileFlags,
) -> Result<(), CoreIrFileError> {
    // Serialize the program to binary format (bincode)
    let ir_bytes =
        bincode::serialize(program).map_err(|e| CoreIrFileError::SerializeError(e.to_string()))?;

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

/// Load a lowered Program from a Core IR file.
///
/// # Arguments
///
/// * `path` - Input file path
///
/// # Returns
///
/// Returns the loaded Program on success, or a CoreIrFileError on failure.
pub fn load<P: AsRef<Path>>(path: P) -> Result<Program, CoreIrFileError> {
    let (program, _header) = load_with_header(path)?;
    Ok(program)
}

/// Load a lowered Program and header from a Core IR file.
pub fn load_with_header<P: AsRef<Path>>(
    path: P,
) -> Result<(Program, CoreIrFileHeader), CoreIrFileError> {
    let mut file = File::open(path)?;

    // Read and verify magic
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(CoreIrFileError::InvalidMagic);
    }

    // Read version
    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version != VERSION {
        return Err(CoreIrFileError::UnsupportedVersion(version));
    }

    // Read flags
    let mut flags_bytes = [0u8; 4];
    file.read_exact(&mut flags_bytes)?;
    let flags = CoreIrFileFlags::from_u32(u32::from_le_bytes(flags_bytes));

    // Read IR length
    let mut length_bytes = [0u8; 4];
    file.read_exact(&mut length_bytes)?;
    let ir_length = u32::from_le_bytes(length_bytes);

    // Read IR data
    let mut ir_bytes = vec![0u8; ir_length as usize];
    file.read_exact(&mut ir_bytes)?;

    // Deserialize program
    let program: Program = bincode::deserialize(&ir_bytes)
        .map_err(|e| CoreIrFileError::DeserializeError(e.to_string()))?;

    let header = CoreIrFileHeader {
        version,
        flags,
        ir_length,
    };

    Ok((program, header))
}

/// Load Core IR from raw bytes (for embedded/in-memory use).
pub fn load_from_bytes(data: &[u8]) -> Result<Program, CoreIrFileError> {
    if data.len() < 16 {
        return Err(CoreIrFileError::InvalidMagic);
    }

    // Verify magic
    if &data[0..4] != MAGIC {
        return Err(CoreIrFileError::InvalidMagic);
    }

    // Read version
    let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if version != VERSION {
        return Err(CoreIrFileError::UnsupportedVersion(version));
    }

    // Read IR length
    let ir_length = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;

    // Verify data length
    if data.len() < 16 + ir_length {
        return Err(CoreIrFileError::DeserializeError(
            "Truncated data".to_string(),
        ));
    }

    // Deserialize program
    let program: Program = bincode::deserialize(&data[16..16 + ir_length])
        .map_err(|e| CoreIrFileError::DeserializeError(e.to_string()))?;

    Ok(program)
}

/// Serialize a Program to bytes (for in-memory use)
pub fn save_to_bytes(program: &Program) -> Result<Vec<u8>, CoreIrFileError> {
    let flags = CoreIrFileFlags::default_flags();

    // Serialize the program
    let ir_bytes =
        bincode::serialize(program).map_err(|e| CoreIrFileError::SerializeError(e.to_string()))?;

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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::ir::core::{Block, Program};
    use crate::span::Span;

    fn empty_program() -> Program {
        Program {
            abstract_types: vec![],
            primitive_types: vec![],
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
        assert!(matches!(result, Err(CoreIrFileError::InvalidMagic)));
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
            Err(CoreIrFileError::UnsupportedVersion(999))
        ));
    }

    #[test]
    fn test_stale_definition_chronology_version_is_rejected_11036() -> Result<(), CoreIrFileError> {
        let program = empty_program();
        let mut bytes = save_to_bytes(&program)?;
        bytes[4..8].copy_from_slice(&(VERSION - 1).to_le_bytes());

        assert!(matches!(
            load_from_bytes(&bytes),
            Err(CoreIrFileError::UnsupportedVersion(version)) if version == VERSION - 1
        ));
        Ok(())
    }

    /// Issue #10906 (Phase 1c of #10869): a `.sjir` blob whose declared
    /// `ir_length` exceeds the bytes actually available (truncated file /
    /// partial write) must be rejected with a typed `DeserializeError`, not a
    /// panic from an out-of-bounds slice.
    #[test]
    fn test_load_from_bytes_truncated_data_is_rejected() {
        let mut data = vec![];
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&VERSION.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // flags
        data.extend_from_slice(&1_000_000u32.to_le_bytes()); // absurd ir_length
                                                             // No IR data follows: the file was cut short.

        let result = load_from_bytes(&data);
        assert!(
            matches!(result, Err(CoreIrFileError::DeserializeError(_))),
            "expected DeserializeError for truncated .sjir data, got: {result:?}"
        );
    }

    /// Issue #10906 (Phase 1c of #10869): a `.sjir` whose HEADER is valid
    /// (right magic/version/declared length) but whose bincode IR payload is
    /// bit-flipped/corrupted must never panic the host — the "cache
    /// deserialize/load" boundary #10869 names as its own entrypoint.
    /// Asserts the load never panics; if it does return an error, it must be
    /// the typed `DeserializeError` variant, not some other failure mode.
    #[test]
    fn test_load_from_bytes_corrupted_payload_never_panics_10906() {
        let program = empty_program();
        let mut bytes = save_to_bytes(&program).expect("serialization should succeed");
        assert!(
            bytes.len() > 16 + 8,
            "empty program's IR payload is too small to exercise payload corruption: {} bytes",
            bytes.len()
        );

        // Corrupt a run of bytes inside the IR payload, past the 16-byte
        // fixed header this function already validates before touching it.
        for b in bytes.iter_mut().skip(16).take(8) {
            *b ^= 0xFF;
        }

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_from_bytes(&bytes)));
        assert!(
            result.is_ok(),
            "loading a corrupted .sjir payload must never panic (Issue #10906)"
        );
        if let Ok(Err(e)) = &result {
            assert!(
                matches!(e, CoreIrFileError::DeserializeError(_)),
                "expected a DeserializeError for a corrupted payload, got: {e:?}"
            );
        }
    }
}
