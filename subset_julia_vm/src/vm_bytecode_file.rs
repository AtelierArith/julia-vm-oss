//! Persisted VM bytecode file format for SubsetJuliaVM.
//!
//! `.sjvmbc` stores a compiled VM `CompiledProgram` for direct interpreter
//! execution. The original Core IR `Program` is stored next to the compiled
//! payload so runtime specialization context can be reconstructed after
//! deserialization.

use crate::ir::core::Program;
use crate::vm::CompiledProgram;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Magic bytes identifying a SubsetJuliaVM VM bytecode file.
pub const MAGIC: &[u8; 4] = b"SJVM";

/// Current persisted VM bytecode file format version.
pub const VERSION: u32 = 3;

#[derive(serde::Serialize, serde::Deserialize)]
struct SerializedVmBytecode {
    program: Program,
    compiled: CompiledProgram,
}

/// Persisted VM bytecode file format error.
#[derive(Debug)]
pub enum VmBytecodeFileError {
    /// I/O error during file operations
    IoError(std::io::Error),
    /// Invalid magic bytes - not a valid SubsetJuliaVM VM bytecode file
    InvalidMagic,
    /// Unsupported format version
    UnsupportedVersion(u32),
    /// Deserialization error
    DeserializeError(String),
    /// Serialization error
    SerializeError(String),
}

impl std::fmt::Display for VmBytecodeFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VmBytecodeFileError::IoError(e) => write!(f, "I/O error: {}", e),
            VmBytecodeFileError::InvalidMagic => write!(
                f,
                "Invalid magic bytes - not a valid SubsetJuliaVM VM bytecode file"
            ),
            VmBytecodeFileError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "Unsupported VM bytecode file version: {} (current: {})",
                    v, VERSION
                )
            }
            VmBytecodeFileError::DeserializeError(e) => write!(f, "Failed to deserialize: {}", e),
            VmBytecodeFileError::SerializeError(e) => write!(f, "Failed to serialize: {}", e),
        }
    }
}

impl std::error::Error for VmBytecodeFileError {}

impl From<std::io::Error> for VmBytecodeFileError {
    fn from(e: std::io::Error) -> Self {
        VmBytecodeFileError::IoError(e)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct VmBytecodeFileFlags {
    has_debug_info: bool,
    has_spans: bool,
    _reserved: u16,
}

impl VmBytecodeFileFlags {
    fn default_flags() -> Self {
        Self {
            has_debug_info: true,
            has_spans: true,
            _reserved: 0,
        }
    }

    fn to_u32(self) -> u32 {
        let mut flags: u32 = 0;
        if self.has_debug_info {
            flags |= 1 << 0;
        }
        if self.has_spans {
            flags |= 1 << 1;
        }
        flags
    }
}

/// Save a compiled VM program to a VM bytecode file.
pub fn save<P: AsRef<Path>>(
    program: &Program,
    compiled: &CompiledProgram,
    path: P,
) -> Result<(), VmBytecodeFileError> {
    let payload = SerializedVmBytecode {
        program: program.clone(),
        compiled: compiled.clone(),
    };
    let payload_bytes = bincode::serialize(&payload)
        .map_err(|e| VmBytecodeFileError::SerializeError(e.to_string()))?;

    let mut file = File::create(path)?;
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    file.write_all(&VmBytecodeFileFlags::default_flags().to_u32().to_le_bytes())?;
    file.write_all(&(payload_bytes.len() as u32).to_le_bytes())?;
    file.write_all(&payload_bytes)?;

    Ok(())
}

/// Load a compiled VM program from a VM bytecode file.
pub fn load<P: AsRef<Path>>(path: P) -> Result<CompiledProgram, VmBytecodeFileError> {
    let mut file = File::open(path)?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(VmBytecodeFileError::InvalidMagic);
    }

    let mut version_bytes = [0u8; 4];
    file.read_exact(&mut version_bytes)?;
    let version = u32::from_le_bytes(version_bytes);
    if version > VERSION {
        return Err(VmBytecodeFileError::UnsupportedVersion(version));
    }

    let mut flags_bytes = [0u8; 4];
    file.read_exact(&mut flags_bytes)?;

    let mut length_bytes = [0u8; 4];
    file.read_exact(&mut length_bytes)?;
    let payload_length = u32::from_le_bytes(length_bytes);

    let mut payload_bytes = vec![0u8; payload_length as usize];
    file.read_exact(&mut payload_bytes)?;

    let payload: SerializedVmBytecode = bincode::deserialize(&payload_bytes)
        .map_err(|e| VmBytecodeFileError::DeserializeError(e.to_string()))?;
    let mut compiled = payload.compiled;
    crate::compile::cache::restore_compile_context_from_program(&mut compiled, &payload.program);
    Ok(compiled)
}
